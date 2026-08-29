//! Fake Adobe Identity Management Services endpoint.
//!
//! The exchange this stands in for carries a client secret and a signed
//! assertion, so the questions worth asking about it are questions about
//! request bytes: how many requests were sent, where they went, and what
//! exactly they contained. A fake that answers with scripted bytes and records
//! every request makes those questions answerable, and makes a trap listener -
//! one that must receive nothing at all - just as easy to set up.
//!
//! It speaks the protocol over a plain connection rather than a protected one.
//! What it is for is framing and request accounting; proving that a hostile
//! certificate cannot authenticate the real endpoint needs the real client and
//! belongs to the composed transcript.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

/// Separator between a request head and its body.
const HEAD_SEPARATOR: &[u8] = b"\r\n\r\n";

/// Field a request declares its body length with.
const LENGTH_FIELD: &str = "content-length:";

/// A listener that answers with scripted bytes and records what it received.
#[derive(Debug)]
pub struct ScriptedIdentityManagementServer {
    /// Address callers connect to.
    address: SocketAddr,
    /// Complete bytes of every request that arrived, in order.
    received: Arc<Mutex<Vec<Vec<u8>>>>,
    /// Whether the accept loop should stop.
    stopping: Arc<AtomicBool>,
    /// The accept loop.
    listening: Option<JoinHandle<()>>,
}

impl ScriptedIdentityManagementServer {
    /// Returns a listener answering every request with `response`.
    ///
    /// # Panics
    ///
    /// Panics when a loopback port cannot be bound, which a test environment
    /// that cannot bind one has already failed for another reason.
    #[must_use]
    pub fn answering(response: &[u8]) -> Self {
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("a loopback port binds");
        let address = listener.local_addr().expect("the bound port has an address");
        let received = Arc::new(Mutex::new(Vec::new()));
        let stopping = Arc::new(AtomicBool::new(false));
        let answer = response.to_vec();
        let recorded = Arc::clone(&received);
        let halting = Arc::clone(&stopping);
        let listening = std::thread::spawn(move || {
            for connection in listener.incoming() {
                if halting.load(Ordering::SeqCst) {
                    return;
                }
                let Ok(connection) = connection else {
                    return;
                };
                serve(connection, &answer, &recorded);
            }
        });
        Self { address, received, stopping, listening: Some(listening) }
    }

    /// Returns a listener that answers nothing, for a trap.
    ///
    /// A trap exists to be proved empty, so it must accept a connection rather
    /// than refuse one: a refused connection would look the same as a caller
    /// that never tried.
    #[must_use]
    pub fn trap() -> Self {
        Self::answering(b"")
    }

    /// Returns the address callers connect to.
    #[must_use]
    pub fn address(&self) -> SocketAddr {
        self.address
    }

    /// Returns the complete bytes of every request that arrived.
    ///
    /// # Panics
    ///
    /// Panics when a caller thread panicked while recording, which leaves the
    /// record unusable rather than merely empty.
    #[must_use]
    pub fn received(&self) -> Vec<Vec<u8>> {
        self.received.lock().expect("the record is not poisoned").clone()
    }
}

impl Drop for ScriptedIdentityManagementServer {
    fn drop(&mut self) {
        self.stopping.store(true, Ordering::SeqCst);
        let _unblocked = TcpStream::connect(self.address);
        if let Some(listening) = self.listening.take() {
            let _joined = listening.join();
        }
    }
}

/// Reads one complete request, records it, and writes the scripted answer.
fn serve(mut connection: TcpStream, answer: &[u8], received: &Arc<Mutex<Vec<Vec<u8>>>>) {
    let Some(request) = read_request(&mut connection) else {
        return;
    };
    if request.is_empty() {
        return;
    }
    received.lock().expect("the record is not poisoned").push(request);
    let _written = connection.write_all(answer);
    let _flushed = connection.flush();
}

/// Reads one complete request, head and declared body.
///
/// The head is read a line at a time and the body is read to its declared
/// length, so the record holds exactly what the caller sent rather than
/// whatever happened to arrive in one read.
fn read_request(connection: &mut TcpStream) -> Option<Vec<u8>> {
    let mut reader = BufReader::new(connection);
    let mut request = Vec::new();
    loop {
        let mut line = Vec::new();
        let read = reader.read_until(b'\n', &mut line).ok()?;
        if read == 0 {
            return Some(request);
        }
        request.extend_from_slice(&line);
        if line == b"\r\n" {
            break;
        }
    }
    let head_end = find(&request, HEAD_SEPARATOR)?;
    let declared = declared_length(&request[..head_end])?;
    let mut body = vec![0; declared];
    reader.read_exact(&mut body).ok()?;
    request.extend_from_slice(&body);
    Some(request)
}

/// Returns the body length one request head declares.
fn declared_length(head: &[u8]) -> Option<usize> {
    let head = core::str::from_utf8(head).ok()?;
    for line in head.split("\r\n") {
        let lowered = line.to_ascii_lowercase();
        let Some(value) = lowered.strip_prefix(LENGTH_FIELD) else {
            continue;
        };
        return value.trim().parse().ok();
    }
    Some(0)
}

/// Returns where `needle` starts inside `haystack`.
fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|window| window == needle)
}
