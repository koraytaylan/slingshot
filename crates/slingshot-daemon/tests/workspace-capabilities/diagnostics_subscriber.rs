//! Probe for the diagnostics-subscriber capability.
//!
//! Requires a subscriber that writes to a supplied sink rather than standard
//! output, renders one JavaScript Object Notation record per event, and honors
//! a level filter, because ordinary command output must stay result-only.

use std::io;
use std::sync::{Arc, Mutex};

use tracing_subscriber::EnvFilter;
use tracing_subscriber::fmt::MakeWriter;

/// A sink that collects everything the subscriber writes.
#[derive(Clone, Default)]
struct CollectedSink(Arc<Mutex<Vec<u8>>>);

impl io::Write for CollectedSink {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        self.0.lock().expect("the sink is not poisoned").extend_from_slice(buffer);
        Ok(buffer.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl<'writer> MakeWriter<'writer> for CollectedSink {
    type Writer = Self;

    fn make_writer(&'writer self) -> Self::Writer {
        self.clone()
    }
}

#[test]
fn events_reach_the_supplied_sink_as_filtered_records() {
    let sink = CollectedSink::default();
    let subscriber = tracing_subscriber::fmt()
        .json()
        .with_writer(sink.clone())
        .with_env_filter(EnvFilter::new("info"))
        .finish();
    tracing::subscriber::with_default(subscriber, || {
        tracing::info!(target: "probe", operation = "ping", "the request was served");
        tracing::debug!(target: "probe", "this event is below the filter");
    });
    let collected = sink.0.lock().expect("the sink is not poisoned").clone();
    let rendered = String::from_utf8(collected).expect("the records are text");
    let records: Vec<&str> = rendered.lines().collect();
    assert_eq!(records.len(), 1, "{rendered}");
    let record: serde_json::Value =
        serde_json::from_str(records[0]).expect("the record is a document");
    assert_eq!(record["fields"]["message"], "the request was served");
    assert_eq!(record["fields"]["operation"], "ping");
    assert_eq!(record["level"], "INFO");
}
