//! Pre-request HTTP/2 negotiation. No request headers or body are accepted here.
//! The enclosing driver owns the selected connection and discards it on error
//! or cancellation; negotiation is never retried against a different endpoint.

use tokio::io::{AsyncRead, AsyncWrite, AsyncWriteExt};
use tokio::time::{Duration, timeout};

use crate::selected_author_http::FiniteHttpFailure;
use crate::selected_author_http2_flow::SendWindows;
use crate::selected_author_http2_frames::ResponseFrameReader;

const PREFACE: &[u8] = b"PRI * HTTP/2.0\r\n\r\nSM\r\n\r\n";
// SETTINGS_ENABLE_PUSH = 0. All other receiving limits retain their RFC defaults.
const SETTINGS: &[u8] = &[0, 0, 6, 4, 0, 0, 0, 0, 0, 0, 2, 0, 0, 0, 0];
const ACK: &[u8] = &[0, 0, 0, 4, 1, 0, 0, 0, 0];

/// One selected-author connection after TLS/prior-knowledge and SETTINGS.
/// This is protocol readiness, not authorization to submit an operation.
#[derive(Debug)]
pub struct PreparedHttp2 {
    /// The same selected socket used for negotiation; never reconnect it.
    pub stream: crate::selected_author_transport::SelectedAuthorStream,
    /// Accounting already established on that socket.
    pub negotiated: Negotiated,
}

impl crate::selected_author_transport::SelectedAuthorTransport {
    /// Opens and negotiates exactly the frozen author without sending a request.
    /// Failed or cancelled negotiation drops the connection and has no fallback.
    pub async fn prepare_http2(&self) -> Result<PreparedHttp2, FiniteHttpFailure> {
        let mut stream = self.connect_http2().await.map_err(|_| FiniteHttpFailure::Connect)?;
        let deadline =
            crate::author_hypertext_transfer_protocol_policy::ExchangeDeadlines::embedded()
                .response_header_milliseconds;
        let negotiated = negotiate(&mut stream, Duration::from_millis(deadline)).await?;
        Ok(PreparedHttp2 { stream, negotiated })
    }
}

/// State which must be retained for this connection, not reset before a request.
#[derive(Debug)]
pub struct Negotiated {
    /// Reader which already consumed the server preface and settings.
    pub frames: ResponseFrameReader,
    /// Send credit after all pre-request peer settings and window updates.
    pub send_windows: SendWindows,
}

/// Exchanges protocol prefaces and SETTINGS under one absolute deadline.
/// Requires a selected h2-ALPN connection or explicitly permitted prior-knowledge
/// cleartext transport. This function sends only connection-level protocol bytes.
/// A refusal therefore maps to Connect, not an uncertain application write.
pub async fn negotiate(
    stream: &mut (impl AsyncRead + AsyncWrite + Unpin),
    deadline: Duration,
) -> Result<Negotiated, FiniteHttpFailure> {
    timeout(deadline, negotiate_frames(stream, ResponseFrameReader::new())).await.map_err(|_| FiniteHttpFailure::Connect)?
}

pub(crate) async fn negotiate_frames(
    stream: &mut (impl AsyncRead + AsyncWrite + Unpin),
    mut frames: ResponseFrameReader,
) -> Result<Negotiated, FiniteHttpFailure> {
    let failure = FiniteHttpFailure::Connect;
    stream.write_all(PREFACE).await.map_err(|_| failure)?;
    stream.write_all(SETTINGS).await.map_err(|_| failure)?;
    stream.flush().await.map_err(|_| failure)?;
    let mut send_windows = SendWindows::new();
    let mut acknowledged = false;
    let mut maximum_concurrent = u32::MAX;
    loop {
        let frame = frames.read(stream).await.map_err(|_| failure)?;
        match frame.kind {
            4 if frame.flags & 1 != 0 => {
                if acknowledged {
                    return Err(failure);
                }
                acknowledged = true;
            }
            4 => {
                send_windows.observe(&frame).map_err(|_| failure)?;
                for setting in frame.payload.chunks_exact(6) {
                    if u16::from_be_bytes(setting[..2].try_into().unwrap()) == 3 {
                        maximum_concurrent = u32::from_be_bytes(setting[2..].try_into().unwrap());
                    }
                }
                stream.write_all(ACK).await.map_err(|_| failure)?;
                stream.flush().await.map_err(|_| failure)?;
            }
            6 if frame.flags & 1 == 0 => {
                let mut ack = [0; 17];
                ack[..9].copy_from_slice(&[0, 0, 8, 6, 1, 0, 0, 0, 0]);
                ack[9..].copy_from_slice(&frame.payload);
                stream.write_all(&ack).await.map_err(|_| failure)?;
                stream.flush().await.map_err(|_| failure)?;
            }
            // No local PING was sent; unsolicited acknowledgements prove nothing.
            6 => {}
            8 if frame.stream_identifier == 0 => {
                send_windows.observe(&frame).map_err(|_| failure)?;
            }
            // No request stream exists yet. GOAWAY cannot authorize opening one.
            0 | 1 | 3 | 5 | 7 | 8 | 9 => return Err(failure),
            // PRIORITY may refer to idle streams; unknown extension frames are ignored.
            _ => {}
        }
        if acknowledged && maximum_concurrent != 0 {
            return Ok(Negotiated { frames, send_windows });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::AsyncReadExt;

    fn frame(kind: u8, flags: u8, stream: u32, payload: &[u8]) -> Vec<u8> {
        let length = (payload.len() as u32).to_be_bytes();
        let mut bytes = vec![length[1], length[2], length[3], kind, flags];
        bytes.extend_from_slice(&stream.to_be_bytes());
        bytes.extend_from_slice(payload);
        bytes
    }

    async fn peer_preface(peer: &mut tokio::io::DuplexStream) {
        let mut bytes = vec![0; PREFACE.len() + SETTINGS.len()];
        peer.read_exact(&mut bytes).await.unwrap();
        assert_eq!(&bytes[..PREFACE.len()], PREFACE);
        assert_eq!(&bytes[PREFACE.len()..], SETTINGS);
    }

    #[tokio::test]
    async fn handshake_waits_for_ack_and_stream_permission_and_retains_credit() {
        let (mut client, mut peer) = tokio::io::duplex(128);
        let server = async {
            peer_preface(&mut peer).await;
            peer.write_all(&frame(4, 0, 0, &[0, 3, 0, 0, 0, 0, 0, 4, 0, 0, 0, 7])).await.unwrap();
            let mut ack = [0; 9];
            peer.read_exact(&mut ack).await.unwrap();
            assert_eq!(ack, ACK);
            peer.write_all(ACK).await.unwrap();
            peer.write_all(&frame(6, 0, 0, b"12345678")).await.unwrap();
            let mut ping_ack = [0; 17];
            peer.read_exact(&mut ping_ack).await.unwrap();
            assert_eq!(ping_ack.to_vec(), frame(6, 1, 0, b"12345678"));
            peer.write_all(&frame(4, 0, 0, &[0, 3, 0, 0, 0, 1])).await.unwrap();
            peer.read_exact(&mut ack).await.unwrap();
            assert_eq!(ack, ACK);
        };
        let (result, ()) = tokio::join!(negotiate(&mut client, Duration::from_secs(1)), server);
        let mut negotiated = result.unwrap();
        assert_eq!(negotiated.send_windows.reserve(100), 7);
        assert_eq!(negotiated.send_windows.reserve(100), 0);
        // The reader must not expect a second server preface.
        assert_eq!(
            negotiated.frames.read(&mut frame(1, 5, 1, &[0x88]).as_slice()).await.unwrap().kind,
            1
        );
    }

    #[tokio::test]
    async fn refusals_never_send_application_bytes() {
        for invalid in [
            b"HTTP/1.1 200 OK\r\n\r\n".to_vec(),
            frame(4, 1, 0, &[]),
            [frame(4, 0, 0, &[]), frame(1, 5, 1, &[0x88])].concat(),
            [frame(4, 0, 0, &[]), frame(7, 0, 0, &[0; 8])].concat(),
            [frame(4, 0, 0, &[]), frame(8, 0, 1, &[0, 0, 0, 1])].concat(),
        ] {
            let expects_ack = invalid.starts_with(&frame(4, 0, 0, &[]));
            let (mut client, mut peer) = tokio::io::duplex(128);
            let request = async {
                let failure = negotiate(&mut client, Duration::from_secs(1)).await.unwrap_err();
                assert!(!failure.request_may_have_reached_author());
                drop(client);
            };
            let server = async {
                peer_preface(&mut peer).await;
                peer.write_all(&invalid).await.unwrap();
                let mut remainder = Vec::new();
                peer.read_to_end(&mut remainder).await.unwrap();
                assert_eq!(remainder, if expects_ack { ACK } else { &[] });
            };
            tokio::join!(request, server);
        }
    }

    #[tokio::test]
    async fn silent_peer_is_bounded_without_sending_a_request() {
        let (mut client, mut peer) = tokio::io::duplex(128);
        let request = async {
            assert_eq!(
                negotiate(&mut client, Duration::from_millis(10)).await.unwrap_err(),
                FiniteHttpFailure::Connect
            );
            drop(client);
        };
        let server = async {
            peer_preface(&mut peer).await;
            let mut extra = Vec::new();
            peer.read_to_end(&mut extra).await.unwrap();
            assert!(extra.is_empty());
        };
        tokio::join!(request, server);
    }
}
