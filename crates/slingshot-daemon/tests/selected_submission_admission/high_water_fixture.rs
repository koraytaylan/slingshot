//! Wire checks for the CSRF-protected high-water capture used by reset tests.

use slingshot_agent_connection::authentication::environment_provider::RequestAuthentication;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};

const PREFACE_AND_SETTINGS_BYTES: usize = 39;
const FRAME_HEADER_BYTES: usize = 9;
const MAXIMUM_FRAME_BYTES: usize = 16384;
const HIGH_LENGTH_SHIFT: u32 = 16;
const MIDDLE_LENGTH_SHIFT: u32 = 8;
const HEADERS: u8 = 1;
const END_HEADERS_AND_STREAM: u8 = 5;
const DATA: u8 = 0;
const END_STREAM: u8 = 1;
const END_HEADERS: u8 = 4;
const STATUS_OK: u8 = 0x88;
const SETTINGS: [u8; FRAME_HEADER_BYTES] = [0, 0, 0, 4, 0, 0, 0, 0, 0];
const SETTINGS_ACK: [u8; FRAME_HEADER_BYTES] = [0, 0, 0, 4, 1, 0, 0, 0, 0];
const TOKEN_BODY: &[u8] = br#"{"token":"high-water-token"}"#;

fn length(header: &[u8; FRAME_HEADER_BYTES]) -> usize {
    usize::from(header[0]) << HIGH_LENGTH_SHIFT
        | usize::from(header[1]) << MIDDLE_LENGTH_SHIFT
        | usize::from(header[2])
}

pub(super) async fn token(
    listener: &TcpListener,
    http2: bool,
    authentication: &RequestAuthentication,
) {
    let (mut socket, _) = listener.accept().await.unwrap();
    let mut request = Vec::new();
    if http2 {
        let mut preface = [0; PREFACE_AND_SETTINGS_BYTES];
        socket.read_exact(&mut preface).await.unwrap();
        assert!(preface.starts_with(b"PRI * HTTP/2.0\r\n\r\nSM\r\n\r\n"));
        socket.write_all(&SETTINGS).await.unwrap();
        let mut header = [0; FRAME_HEADER_BYTES];
        socket.read_exact(&mut header).await.unwrap();
        assert_eq!(header, SETTINGS_ACK);
        socket.write_all(&SETTINGS_ACK).await.unwrap();
        socket.read_exact(&mut header).await.unwrap();
        assert_eq!((header[3], header[4]), (HEADERS, END_HEADERS_AND_STREAM));
        assert!(length(&header) <= MAXIMUM_FRAME_BYTES);
        request.resize(length(&header), 0);
        socket.read_exact(&mut request).await.unwrap();
    } else {
        while !request.ends_with(b"\r\n\r\n") {
            request.push(socket.read_u8().await.unwrap());
            assert!(request.len() <= MAXIMUM_FRAME_BYTES);
        }
        assert!(request.starts_with(b"GET /aem/libs/granite/csrf/token.json HTTP/1.1\r\n"));
    }
    let route = b"/aem/libs/granite/csrf/token.json";
    assert!(request.windows(route.len()).any(|part| part == route));
    authentication
        .lend_value_bytes(|value| assert!(request.windows(value.len()).any(|part| part == value)));
    if http2 {
        let mut block = vec![STATUS_OK];
        for (name, value) in [
            ("content-type", "application/json".to_owned()),
            ("content-length", TOKEN_BODY.len().to_string()),
        ] {
            block.extend_from_slice(&[0, u8::try_from(name.len()).unwrap()]);
            block.extend_from_slice(name.as_bytes());
            block.push(u8::try_from(value.len()).unwrap());
            block.extend_from_slice(value.as_bytes());
        }
        for (kind, flags, bytes) in
            [(HEADERS, END_HEADERS, block.as_slice()), (DATA, END_STREAM, TOKEN_BODY)]
        {
            let count = u32::try_from(bytes.len()).unwrap().to_be_bytes();
            socket
                .write_all(&[count[1], count[2], count[3], kind, flags, 0, 0, 0, 1])
                .await
                .unwrap();
            socket.write_all(bytes).await.unwrap();
        }
        let mut discarded = Vec::new();
        socket.read_to_end(&mut discarded).await.unwrap();
    } else {
        socket.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n", TOKEN_BODY.len()).as_bytes()).await.unwrap();
        socket.write_all(TOKEN_BODY).await.unwrap();
    }
    socket.shutdown().await.unwrap();
}

pub(super) async fn body(socket: &mut TcpStream, request: &[u8], http2: bool) {
    let token = b"high-water-token";
    assert!(request.windows(token.len()).any(|part| part == token));
    let mut body = Vec::new();
    if http2 {
        loop {
            let mut header = [0; FRAME_HEADER_BYTES];
            socket.read_exact(&mut header).await.unwrap();
            assert_eq!(header[3], DATA);
            assert!(length(&header) <= MAXIMUM_FRAME_BYTES);
            let start = body.len();
            body.resize(start + length(&header), 0);
            socket.read_exact(&mut body[start..]).await.unwrap();
            if header[4] & END_STREAM != 0 {
                break;
            }
        }
    } else {
        let head = std::str::from_utf8(request).unwrap();
        let count = head
            .lines()
            .find_map(|line| line.strip_prefix("Content-Length: "))
            .unwrap()
            .parse::<usize>()
            .unwrap();
        assert!(count <= MAXIMUM_FRAME_BYTES);
        body.resize(count, 0);
        socket.read_exact(&mut body).await.unwrap();
    }
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&body).unwrap(),
        serde_json::json!({
            "agent_event_store_generation": 7, "daemon_subscription_identifier": "subscription-one"
        })
    );
}
