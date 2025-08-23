use crossterm::event::{self, Event};
use miette::miette;
use std::time::Duration;
use std::{sync::mpsc, thread};

pub struct EventHandler {
    receiver: mpsc::Receiver<Event>,
}

impl EventHandler {
    pub fn new(tick_rate: Duration) -> Self {
        let (sender, receiver) = mpsc::channel();

        thread::spawn(move || {
            let mut last_tick = std::time::Instant::now();

            loop {
                let timeout = tick_rate
                    .checked_sub(last_tick.elapsed())
                    .unwrap_or(Duration::from_secs(0));

                if event::poll(timeout).unwrap() {
                    if let Ok(event) = event::read() {
                        if sender.send(event).is_err() {
                            break;
                        }
                    }
                }

                if last_tick.elapsed() >= tick_rate {
                    last_tick = std::time::Instant::now();
                }
            }
        });

        Self { receiver }
    }
}

pub trait EventHandlerExt {
    fn next(&self) -> miette::Result<Option<Event>>;
}

impl EventHandlerExt for EventHandler {
    fn next(&self) -> miette::Result<Option<Event>> {
        match self.receiver.try_recv() {
            Ok(event) => Ok(Some(event)),
            Err(mpsc::TryRecvError::Empty) => Ok(None),
            Err(mpsc::TryRecvError::Disconnected) => Err(miette!("Event channel disconnected")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_handler_creation() {
        let handler = EventHandler::new(Duration::from_millis(100));
        // Verify that the handler can be created without panicking
        assert!(handler.receiver.try_recv().is_err()); // Should be empty initially
    }

    #[test]
    fn test_next_returns_none_when_no_events() {
        let handler = EventHandler::new(Duration::from_millis(100));
        let result = handler.next().unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_next_handles_disconnected_channel() {
        let (_, receiver) = mpsc::channel();
        let handler = EventHandler { receiver };

        // Drop the sender to simulate disconnection
        drop(handler.receiver);

        // Create a new handler with a disconnected receiver
        let (sender, receiver) = mpsc::channel();
        drop(sender); // Disconnect immediately

        let handler = EventHandler { receiver };
        let result = handler.next();

        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Event channel disconnected")
        );
    }

    #[test]
    fn test_event_handler_with_different_tick_rates() {
        let fast_handler = EventHandler::new(Duration::from_millis(10));
        let slow_handler = EventHandler::new(Duration::from_millis(1000));

        // Both should be created successfully
        assert!(fast_handler.next().unwrap().is_none());
        assert!(slow_handler.next().unwrap().is_none());
    }

    #[test]
    fn test_multiple_next_calls() {
        let handler = EventHandler::new(Duration::from_millis(50));

        // Multiple calls should not panic
        for _ in 0..5 {
            let _ = handler.next();
        }
    }
}
