use crossterm::event::{self, Event};
use miette::miette;
use std::{sync::mpsc, thread, time::Duration};

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
