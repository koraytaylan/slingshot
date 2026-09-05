//! HTTP/2 flow accounting for the product's single request on stream 1.
//! Reservations precede writes. A cancelled/failed write requires discarding
//! the connection, never restoring credit and guessing which bytes arrived.

const INITIAL_WINDOW: i64 = 65_535;
const MAXIMUM_WINDOW: i64 = 0x7fff_ffff;
const DATA_FRAME_BYTES: usize = 16_384;

/// Invalid stream, credit, SETTINGS value or exceeded flow window.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("the author HTTP/2 flow window is invalid")]
pub struct FlowRefusal;

/// Independent connection and request-stream send windows. The stream window
/// is signed because a SETTINGS reduction may legitimately make it negative.
#[derive(Debug)]
pub struct SendWindows {
    connection: i64,
    stream: i64,
    initial_stream: i64,
}

impl Default for SendWindows {
    fn default() -> Self {
        Self::new()
    }
}

impl SendWindows {
    /// Applies flow-control parts of a wire-validated SETTINGS or WINDOW_UPDATE
    /// frame, preserving SETTINGS order. A failed frame changes neither window.
    /// Other settings still require handling by the connection driver.
    pub fn observe(
        &mut self,
        frame: &crate::selected_author_http2_frames::ResponseFrame,
    ) -> Result<(), FlowRefusal> {
        let mut next = Self {
            connection: self.connection,
            stream: self.stream,
            initial_stream: self.initial_stream,
        };
        match frame.kind {
            4 if frame.stream_identifier == 0
                && frame.payload.len() % 6 == 0
                && (frame.flags & 1 == 0 || frame.payload.is_empty()) =>
            {
                for setting in frame.payload.chunks_exact(6) {
                    if u16::from_be_bytes(setting[..2].try_into().unwrap()) == 4 {
                        next.initial_stream_size(u32::from_be_bytes(
                            setting[2..].try_into().unwrap(),
                        ))?;
                    }
                }
            }
            8 if frame.payload.len() == 4 => {
                next.credit(
                    frame.stream_identifier,
                    u32::from_be_bytes(frame.payload.as_slice().try_into().unwrap()) & 0x7fff_ffff,
                )?;
            }
            _ => return Err(FlowRefusal),
        }
        *self = next;
        Ok(())
    }

    /// Starts at HTTP/2's default windows before processing peer settings.
    pub fn new() -> Self {
        Self { connection: INITIAL_WINDOW, stream: INITIAL_WINDOW, initial_stream: INITIAL_WINDOW }
    }

    /// Applies SETTINGS_INITIAL_WINDOW_SIZE to the stream only. Checks occur
    /// before mutation, including overflow caused by previously received credit.
    pub fn initial_stream_size(&mut self, size: u32) -> Result<(), FlowRefusal> {
        if i64::from(size) > MAXIMUM_WINDOW {
            return Err(FlowRefusal);
        }
        let change = i64::from(size) - self.initial_stream;
        let stream = self
            .stream
            .checked_add(change)
            .filter(|value| *value <= MAXIMUM_WINDOW)
            .ok_or(FlowRefusal)?;
        self.stream = stream;
        self.initial_stream = i64::from(size);
        Ok(())
    }

    /// Applies an already decoded WINDOW_UPDATE; reserved bits must have been
    /// removed by its wire codec. Neither zero nor overflowing credit is legal.
    pub fn credit(&mut self, stream_identifier: u32, increment: u32) -> Result<(), FlowRefusal> {
        if increment == 0 || i64::from(increment) > MAXIMUM_WINDOW {
            return Err(FlowRefusal);
        }
        let window = match stream_identifier {
            0 => &mut self.connection,
            1 => &mut self.stream,
            _ => return Err(FlowRefusal),
        };
        let next = window
            .checked_add(i64::from(increment))
            .filter(|value| *value <= MAXIMUM_WINDOW)
            .ok_or(FlowRefusal)?;
        *window = next;
        Ok(())
    }

    /// Reserves the next DATA payload before writing it. Zero means wait for
    /// peer credit (or no remaining bytes), not permission to bypass a window.
    /// A default-size frame is valid even when the peer permits larger frames.
    pub fn reserve(&mut self, remaining: usize) -> usize {
        let allowance = self.connection.min(self.stream).max(0) as usize;
        let count = remaining.min(DATA_FRAME_BYTES).min(allowance);
        self.connection -= count as i64;
        self.stream -= count as i64;
        count
    }
}

/// Local receive windows with a fixed initial allowance. No credit is returned
/// until the consumer explicitly releases a received payload's permit.
#[derive(Debug)]
pub struct ReceiveWindows {
    connection: u32,
    stream: u32,
}

impl Default for ReceiveWindows {
    fn default() -> Self {
        Self::new()
    }
}

impl ReceiveWindows {
    /// Validates and charges a DATA frame before returning its unpadded content.
    /// The count used for flow control is always the original payload length.
    pub fn receive_frame<'window, 'frame>(
        &'window mut self,
        frame: &'frame crate::selected_author_http2_frames::ResponseFrame,
    ) -> Result<(ReceivedPayload<'window>, &'frame [u8]), FlowRefusal> {
        if frame.kind != 0 || frame.stream_identifier != 1 {
            return Err(FlowRefusal);
        }
        let content = if frame.flags & 8 != 0 {
            let padding = usize::from(*frame.payload.first().ok_or(FlowRefusal)?);
            let end = frame
                .payload
                .len()
                .checked_sub(padding)
                .filter(|end| *end >= 1)
                .ok_or(FlowRefusal)?;
            &frame.payload[1..end]
        } else {
            frame.payload.as_slice()
        };
        let permit = self.receive(u32::try_from(frame.payload.len()).map_err(|_| FlowRefusal)?)?;
        Ok((permit, content))
    }

    /// Matches the default windows advertised by the product connection.
    pub fn new() -> Self {
        Self { connection: INITIAL_WINDOW as u32, stream: INITIAL_WINDOW as u32 }
    }

    /// Charges the complete DATA frame payload, including the pad-length octet
    /// and padding, before exposing unpadded content to a consumer.
    pub fn receive(&mut self, payload_bytes: u32) -> Result<ReceivedPayload<'_>, FlowRefusal> {
        let connection = self.connection.checked_sub(payload_bytes).ok_or(FlowRefusal)?;
        let stream = self.stream.checked_sub(payload_bytes).ok_or(FlowRefusal)?;
        self.connection = connection;
        self.stream = stream;
        Ok(ReceivedPayload { windows: self, bytes: payload_bytes })
    }
}

/// Non-cloneable receive credit owned by the current consumer. Dropping this
/// permit returns no credit; cancellation cannot pretend content was consumed.
#[derive(Debug)]
pub struct ReceivedPayload<'window> {
    windows: &'window mut ReceiveWindows,
    bytes: u32,
}

impl ReceivedPayload<'_> {
    /// Consumes the permit after processing and returns the two WINDOW_UPDATE
    /// frames to write before admitting more data. A failed/cancelled write
    /// discards the connection. Empty DATA emits no invalid zero-increment frame.
    pub fn release(self) -> Option<[[u8; 13]; 2]> {
        if self.bytes == 0 {
            return None;
        }
        self.windows.connection += self.bytes;
        self.windows.stream += self.bytes;
        let increment = self.bytes.to_be_bytes();
        let frame = |stream| {
            [0, 0, 4, 8, 0, 0, 0, 0, stream, increment[0], increment[1], increment[2], increment[3]]
        };
        Some([frame(0), frame(1)])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn validated_wire_frames_drive_credit_and_padded_content_accounting() {
        use crate::selected_author_http2_frames::ResponseFrameReader;
        let mut frames = ResponseFrameReader::new();
        let settings = frames
            .read(&mut [0, 0, 6, 4, 0, 0, 0, 0, 0, 0, 4, 0, 0, 0, 0].as_slice())
            .await
            .unwrap();
        let mut send = SendWindows::new();
        send.observe(&settings).unwrap();
        assert_eq!(send.reserve(1), 0);
        let update =
            frames.read(&mut [0, 0, 4, 8, 0, 0, 0, 0, 1, 128, 0, 0, 2].as_slice()).await.unwrap();
        send.observe(&update).unwrap();
        assert_eq!(send.reserve(3), 2);
        frames.read(&mut [0, 0, 1, 1, 4, 0, 0, 0, 1, 0x88].as_slice()).await.unwrap();
        let data =
            frames.read(&mut [0, 0, 3, 0, 9, 0, 0, 0, 1, 1, b'x', 0].as_slice()).await.unwrap();
        let mut receive = ReceiveWindows::new();
        let (permit, content) = receive.receive_frame(&data).unwrap();
        assert_eq!(content, b"x");
        let updates = permit.release().unwrap();
        assert_eq!(&updates[0][9..], &[0, 0, 0, 3]);
    }

    #[test]
    fn a_multi_setting_frame_cannot_partially_change_send_windows() {
        use crate::selected_author_http2_frames::ResponseFrame;
        let mut send = SendWindows::new();
        send.credit(1, (MAXIMUM_WINDOW - INITIAL_WINDOW) as u32).unwrap();
        let frame = ResponseFrame {
            kind: 4,
            flags: 0,
            stream_identifier: 0,
            payload: vec![0, 4, 0, 0, 0, 0, 0, 4, 0, 1, 0, 0],
        };
        assert!(send.observe(&frame).is_err());
        assert_eq!(send.stream, MAXIMUM_WINDOW);
        assert_eq!(send.initial_stream, INITIAL_WINDOW);
        assert_eq!(send.connection, INITIAL_WINDOW);
    }

    #[test]
    fn sending_requires_both_windows_and_never_exceeds_a_frame() {
        let mut windows = SendWindows::new();
        for _ in 0..3 {
            assert_eq!(windows.reserve(usize::MAX), 16_384);
        }
        assert_eq!(windows.reserve(usize::MAX), 16_383);
        assert_eq!(windows.reserve(1), 0);
        windows.credit(0, 10).unwrap();
        assert_eq!(windows.reserve(1), 0);
        windows.credit(1, 5).unwrap();
        assert_eq!(windows.reserve(10), 5);
        assert_eq!(windows.reserve(1), 0);
        assert_eq!((windows.connection, windows.stream), (5, 0));
    }

    #[test]
    fn settings_reduction_can_make_a_stream_negative_without_changing_connection() {
        let mut windows = SendWindows::new();
        assert_eq!(windows.reserve(100), 100);
        windows.initial_stream_size(0).unwrap();
        assert_eq!((windows.connection, windows.stream), (65_435, -100));
        assert_eq!(windows.reserve(1), 0);
        windows.credit(1, 100).unwrap();
        assert_eq!(windows.reserve(1), 0);
        windows.initial_stream_size(1).unwrap();
        assert_eq!(windows.reserve(1), 1);
        assert_eq!(windows.connection, 65_434);
    }

    #[test]
    fn invalid_credits_and_overflowing_settings_leave_windows_unchanged() {
        let mut windows = SendWindows::new();
        for (stream, increment) in [(0, 0), (1, 0), (2, 1), (0, 0x8000_0000), (1, u32::MAX)] {
            assert!(windows.credit(stream, increment).is_err());
            assert_eq!((windows.connection, windows.stream), (INITIAL_WINDOW, INITIAL_WINDOW));
        }
        windows.credit(1, (MAXIMUM_WINDOW - INITIAL_WINDOW) as u32).unwrap();
        assert!(windows.credit(1, 1).is_err());
        assert!(windows.initial_stream_size(65_536).is_err());
        assert_eq!(windows.stream, MAXIMUM_WINDOW);
        assert_eq!(windows.initial_stream, INITIAL_WINDOW);
        assert!(windows.initial_stream_size(0x8000_0000).is_err());
        assert_eq!(windows.initial_stream, INITIAL_WINDOW);
    }

    #[test]
    fn receives_count_padding_and_only_release_consumed_bytes_once() {
        let mut windows = ReceiveWindows::new();
        // Three payload octets represent pad length + one content octet + pad.
        let updates = windows.receive(3).unwrap().release().unwrap();
        assert_eq!(updates[0], [0, 0, 4, 8, 0, 0, 0, 0, 0, 0, 0, 0, 3]);
        assert_eq!(updates[1][8], 1);
        assert_eq!(&updates[1][9..], &[0, 0, 0, 3]);
        assert_eq!((windows.connection, windows.stream), (65_535, 65_535));
        assert!(windows.receive(0).unwrap().release().is_none());
        drop(windows.receive(65_535).unwrap());
        assert!(windows.receive(1).is_err());
        assert_eq!((windows.connection, windows.stream), (0, 0));
    }

    #[test]
    fn failed_receive_is_atomic_and_dropped_credit_is_not_reconstructed() {
        let mut windows = ReceiveWindows::new();
        assert!(windows.receive(65_536).is_err());
        assert_eq!((windows.connection, windows.stream), (65_535, 65_535));
        drop(windows.receive(10).unwrap());
        windows.receive(5).unwrap().release().unwrap();
        assert_eq!((windows.connection, windows.stream), (65_525, 65_525));
        windows.stream = 1;
        assert!(windows.receive(2).is_err());
        assert_eq!((windows.connection, windows.stream), (65_525, 1));
    }
}
