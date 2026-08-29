//! Probe for the diagnostics capability.
//!
//! Requires structured events with typed fields, a span that scopes them, and a
//! level that a subscriber can filter, so a transport can explain itself
//! without writing to the result stream.

use std::sync::{Arc, Mutex};

use tracing::field::{Field, Visit};
use tracing::{Event, Level, Subscriber, span};

/// A subscriber that records the level and the rendered fields of each event.
#[derive(Clone, Default)]
struct RecordingSubscriber(Arc<Mutex<Vec<String>>>);

/// Collects one event's fields into a rendered line.
#[derive(Default)]
struct FieldRenderer(Vec<String>);

impl Visit for FieldRenderer {
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        self.0.push(format!("{}={value:?}", field.name()));
    }
}

impl Subscriber for RecordingSubscriber {
    fn enabled(&self, metadata: &tracing::Metadata<'_>) -> bool {
        *metadata.level() <= Level::INFO
    }

    fn new_span(&self, attributes: &span::Attributes<'_>) -> span::Id {
        self.0
            .lock()
            .expect("the record is not poisoned")
            .push(format!("span:{}", attributes.metadata().name()));
        span::Id::from_u64(1)
    }

    fn record(&self, _span: &span::Id, _values: &span::Record<'_>) {}

    fn record_follows_from(&self, _span: &span::Id, _follows: &span::Id) {}

    fn event(&self, event: &Event<'_>) {
        let mut renderer = FieldRenderer::default();
        event.record(&mut renderer);
        let level = *event.metadata().level();
        self.0
            .lock()
            .expect("the record is not poisoned")
            .push(format!("{level}:{}", renderer.0.join(",")));
    }

    fn enter(&self, _span: &span::Id) {}

    fn exit(&self, _span: &span::Id) {}
}

#[test]
fn structured_events_carry_typed_fields_inside_a_span() {
    let subscriber = RecordingSubscriber::default();
    tracing::subscriber::with_default(subscriber.clone(), || {
        let request = tracing::info_span!("author_request", route = "author");
        let _entered = request.enter();
        tracing::info!(status = 200_u16, retries = 0_u32, "the request completed");
        tracing::trace!("this event is below the enabled level");
    });
    let recorded = subscriber.0.lock().expect("the record is not poisoned").clone();
    assert_eq!(recorded.len(), 2, "{recorded:?}");
    assert_eq!(recorded[0], "span:author_request");
    assert!(recorded[1].starts_with("INFO:"), "{recorded:?}");
    assert!(recorded[1].contains("status=200"), "{recorded:?}");
    assert!(recorded[1].contains("retries=0"), "{recorded:?}");
}
