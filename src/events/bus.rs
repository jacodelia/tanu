//! In-memory event bus with typed channels.
//!
//! The `EventRouter` is the central dispatcher. Components register
//! their sender half as listeners; the router reads from its input
//! channel and broadcasts to all listeners.

use tokio::sync::mpsc;

use crate::events::Event;

/// A sender endpoint for publishing events to the bus.
pub type EventSender = mpsc::UnboundedSender<Event>;

/// A receiver endpoint for consuming events from the bus.
pub type EventReceiver = mpsc::UnboundedReceiver<Event>;

/// Creates a new event channel pair.
pub fn event_channel() -> (EventSender, EventReceiver) {
    mpsc::unbounded_channel()
}

/// The central event dispatcher.
///
/// Reads events from its input channel and broadcasts to all
/// registered listeners. Components receive events via their
/// own listener channels and send events back via the input sender.
pub struct EventRouter {
    input_tx: EventSender,
    input_rx: EventReceiver,
    listeners: Vec<EventSender>,
}

impl Default for EventRouter {
    fn default() -> Self {
        Self::new()
    }
}

impl EventRouter {
    /// Creates a new router with its own internal channel pair.
    pub fn new() -> Self {
        let (tx, rx) = event_channel();
        Self {
            input_tx: tx,
            input_rx: rx,
            listeners: Vec::new(),
        }
    }

    /// Returns a sender that feeds events INTO the router.
    /// Clone this to give to components that need to publish events.
    pub fn sender(&self) -> EventSender {
        self.input_tx.clone()
    }

    /// Registers a component's sender as a listener.
    /// The router will broadcast events TO this sender.
    pub fn register_listener(&mut self, sender: EventSender) {
        self.listeners.push(sender);
    }

    /// Runs the router loop: reads from input, broadcasts to all listeners.
    /// Spawn as a tokio task.
    pub async fn run(&mut self) {
        tracing::info!("EventRouter started");
        while let Some(event) = self.input_rx.recv().await {
            tracing::trace!(event_type = %event.event_type_name(), "Routing event");
            for listener in &self.listeners {
                if let Err(e) = listener.send(event.clone()) {
                    tracing::warn!(
                        event_type = %event.event_type_name(),
                        error = %e,
                        "Failed to send event to listener"
                    );
                }
            }
        }
        tracing::info!("EventRouter stopped");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_event_channel_send_receive() {
        let (tx, mut rx) = event_channel();
        tx.send(Event::Play).unwrap();
        let event = rx.recv().await.unwrap();
        assert_eq!(event, Event::Play);
    }

    #[tokio::test]
    async fn test_router_broadcasts_to_all_listeners() {
        let mut router = EventRouter::new();

        let (l1_tx, mut l1_rx) = event_channel();
        let (l2_tx, mut l2_rx) = event_channel();
        router.register_listener(l1_tx);
        router.register_listener(l2_tx);

        let app_tx = router.sender();

        let router_handle = tokio::spawn(async move {
            router.run().await;
        });

        app_tx.send(Event::Play).unwrap();

        let e1 = l1_rx.recv().await.unwrap();
        let e2 = l2_rx.recv().await.unwrap();
        assert_eq!(e1, Event::Play);
        assert_eq!(e2, Event::Play);

        drop(app_tx);
        router_handle.abort();
    }
}
