//! Probe for the protocol-body capability.
//!
//! Requires a decoded-size bound that fails before the body is collected, a
//! collection that keeps the trailer section, and a body whose frames are
//! reported one at a time rather than only as a finished buffer.

use std::convert::Infallible;
use std::pin::Pin;
use std::task::{Context, Poll};

use bytes::Bytes;
use http::HeaderMap;
use http_body_util::{BodyExt, Full, Limited};
use hyper::body::{Body, Frame, SizeHint};

/// Largest decoded body a probe accepts before refusing.
const DECODED_BODY_LIMIT: usize = 16;

/// A body that yields two data frames and then a trailer section.
struct ScriptedBody {
    remaining: Vec<Frame<Bytes>>,
}

impl ScriptedBody {
    /// Builds the scripted body in the order its frames are produced.
    fn new() -> Self {
        let mut trailers = HeaderMap::new();
        trailers.insert("x-digest", "abc".parse().expect("the trailer value is valid"));
        let mut remaining = vec![
            Frame::trailers(trailers),
            Frame::data(Bytes::from_static(b"two")),
            Frame::data(Bytes::from_static(b"one")),
        ];
        remaining.reverse();
        remaining.reverse();
        Self { remaining }
    }
}

impl Body for ScriptedBody {
    type Data = Bytes;
    type Error = Infallible;

    fn poll_frame(
        mut self: Pin<&mut Self>,
        _context: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Self::Data>, Self::Error>>> {
        Poll::Ready(self.remaining.pop().map(Ok))
    }

    fn size_hint(&self) -> SizeHint {
        SizeHint::default()
    }
}

#[test]
fn a_decoded_bound_fails_before_the_body_is_collected() {
    let runtime =
        tokio::runtime::Builder::new_current_thread().build().expect("the runtime builds");
    runtime.block_on(async {
        let within = Limited::new(Full::new(Bytes::from_static(b"short")), DECODED_BODY_LIMIT);
        let collected = within.collect().await.expect("a body within the bound collects");
        assert_eq!(collected.to_bytes(), Bytes::from_static(b"short"));

        let oversized = Bytes::from(vec![b'x'; DECODED_BODY_LIMIT + 1]);
        let refused = Limited::new(Full::new(oversized), DECODED_BODY_LIMIT).collect().await;
        assert!(refused.is_err(), "a body beyond the bound must be refused");

        let mut scripted = ScriptedBody::new();
        let first =
            scripted.frame().await.expect("a frame arrives").expect("the frame is readable");
        assert_eq!(
            first.data_ref().expect("the first frame carries data"),
            &Bytes::from_static(b"one")
        );

        let collected = ScriptedBody::new().collect().await.expect("the scripted body collects");
        let trailers = collected.trailers().cloned().expect("the body carries a trailer section");
        assert_eq!(trailers.get("x-digest").and_then(|value| value.to_str().ok()), Some("abc"));
        assert_eq!(collected.to_bytes(), Bytes::from_static(b"onetwo"));
    });
}
