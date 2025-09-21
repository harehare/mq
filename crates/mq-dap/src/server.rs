use dap::prelude::*;
use std::io::{self, BufReader, BufWriter};
use tracing::{debug, error, info};

use crate::adapter::MqAdapter;
use crate::error::MqAdapterError;
use crate::log::DebugConsoleWriter;

type DynResult<T> = miette::Result<T, Box<dyn std::error::Error>>;

pub fn start() -> DynResult<()> {
    let (debug_writer, log_rx) = DebugConsoleWriter::new();

    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::TRACE)
        .with_writer(debug_writer)
        .init();

    info!("Starting mq-dap debug adapter");

    let mut adapter = MqAdapter::default();
    let reader = BufReader::new(io::stdin());
    let writer = BufWriter::new(io::stdout());
    let mut server = Server::new(reader, writer);

    // First, the client sends an initialize request
    let req = match server.poll_request()? {
        Some(req) => req,
        None => {
            return Err(Box::new(MqAdapterError::ProtocolError(
                "Missing initialize request".to_string(),
            )));
        }
    };

    if let Command::Initialize(_) = &req.command {
        let capabilities = types::Capabilities {
            supports_set_variable: Some(true),
            supports_exception_options: Some(false),
            supports_exception_filter_options: Some(false),
            ..Default::default()
        };
        let rsp = req.success(ResponseBody::Initialize(capabilities));
        server.respond(rsp)?;
        server.send_event(Event::Initialized)?;
    } else {
        return Err(Box::new(MqAdapterError::ProtocolError(
            "Expected initialize request".to_string(),
        )));
    }

    loop {
        debug!("Checking for log messages");
        // Process all available log messages
        while let Ok(log_message) = log_rx.clone().try_recv() {
            if let Err(e) = adapter.send_log_output(&log_message, &mut server) {
                eprintln!("Failed to send log output: {}", e);
            }
        }

        debug!("Waiting for next request or debugger message");
        if let Some(rx) = adapter.debugger_message_rx() {
            if let Ok(message) = rx.try_recv() {
                if let Err(e) = adapter.handle_debugger_message(message, &mut server) {
                    error!(error = %e, "Failed to handle debugger message");
                }
            }
        }

        match server.poll_request()? {
            Some(req) => {
                if let Err(e) = adapter.handle_request(req, &mut server) {
                    error!(error = %e, "Failed to handle DAP request");
                    if let Some(MqAdapterError::ProtocolError(msg)) =
                        e.downcast_ref::<MqAdapterError>()
                    {
                        if msg == "Shutdown" {
                            break;
                        }
                    }
                }
            }
            None => {
                info!("Client disconnected or stream ended");
                break;
            }
        }
    }

    Ok(())
}
