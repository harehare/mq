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
        if let Some(ref sender) = self.sender
            && sender.send(message.to_string()).is_err()
        {
            // Channel is closed, but we still need to return success
            // to avoid breaking the logging infrastructure
            eprintln!(
                "Warning: Log channel is closed, message dropped: {}",
                message
            );
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_debug_console_writer_send_and_receive() {
        let (mut writer, rx) = DebugConsoleWriter::new();
        let msg = "Hello, mq!";
        let bytes_written = writer.write(msg.as_bytes()).unwrap();
        assert_eq!(bytes_written, msg.len());
        let received = rx.try_recv().unwrap();
        assert_eq!(received, msg);
    }

    #[test]
    fn test_debug_console_writer_flush() {
        let (mut writer, _rx) = DebugConsoleWriter::new();
        assert!(writer.flush().is_ok());
    }

    #[test]
    fn test_debug_console_writer_channel_closed() {
        let (mut writer, rx) = DebugConsoleWriter::new();
        drop(rx); // Close the receiver
        let msg = "Should not panic";
        // Should not panic or return error even if channel is closed
        let bytes_written = writer.write(msg.as_bytes()).unwrap();
        assert_eq!(bytes_written, msg.len());
    }

    #[test]
    fn test_make_writer_returns_clone() {
        let (writer, _rx) = DebugConsoleWriter::new();
        let clone = writer.make_writer();
        // Ensure the clone is equal (sender is Some)
        assert!(clone.sender.is_some());
    }
}
