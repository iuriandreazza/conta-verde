use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};

use conta_verde_application::{DomainEvent, EventPublisher};

/// An event publisher that keeps what it was given, for tests of what a write raises.
#[derive(Default, Clone)]
pub struct RecordingEvents {
    events: Arc<Mutex<Vec<DomainEvent>>>,
}

impl RecordingEvents {
    pub fn events(&self) -> Vec<DomainEvent> {
        self.events.lock().expect("events lock poisoned").clone()
    }
}

impl EventPublisher for RecordingEvents {
    fn publish(&self, event: DomainEvent) -> Pin<Box<dyn Future<Output = ()> + Send + '_>> {
        self.events
            .lock()
            .expect("events lock poisoned")
            .push(event);
        Box::pin(async {})
    }
}
