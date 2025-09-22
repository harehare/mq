use std::io::{self, Write};

use crossbeam_channel::{Receiver, Sender};
use tracing_subscriber::fmt::MakeWriter;

#[derive(Clone)]
pub struct DebugConsoleWriter {
    sender: Option<Sender<String>>,
}

impl DebugConsoleWriter {
    pub fn new() -> (Self, Receiver<String>) {
        let (tx, rx) = crossbeam_channel::unbounded();
        let writer = Self { sender: Some(tx) };
        (writer, rx)
    }
}

impl Write for DebugConsoleWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let message = String::from_utf8_lossy(buf);
        if let Some(ref sender) = self.sender {
            if sender.send(message.to_string()).is_err() {
                // Channel is closed, but we still need to return success
                // to avoid breaking the logging infrastructure
                eprintln!(
                    "Warning: Log channel is closed, message dropped: {}",
                    message
                );
            }
        }

        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        // For debugging purposes, we don't need to implement actual flushing
        // since messages are sent immediately via the channel
        Ok(())
    }
}

impl<'a> MakeWriter<'a> for DebugConsoleWriter {
    type Writer = Self;

    fn make_writer(&'a self) -> Self::Writer {
        self.clone()
    }
}
