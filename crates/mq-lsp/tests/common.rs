//! Minimal stdio JSON-RPC client for driving the `mq-lsp` binary.

use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{Receiver, channel};
use std::time::Duration;

use serde_json::{Value, json};

const TIMEOUT: Duration = Duration::from_secs(20);

/// A running language server plus the messages it has sent.
pub struct LspClient {
    child: Child,
    stdin: ChildStdin,
    rx: Receiver<Value>,
    next_id: i64,
    notifications: Vec<Value>,
    /// Result of the `initialize` request.
    pub init_result: Value,
}

impl LspClient {
    /// Spawns the server and completes the `initialize` handshake.
    pub fn start() -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_mq-lsp"))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("failed to spawn mq-lsp");
        let stdin = child.stdin.take().unwrap();
        let mut stdout = BufReader::new(child.stdout.take().unwrap());
        let (tx, rx) = channel();
        std::thread::spawn(move || {
            while let Some(message) = read_message(&mut stdout) {
                if tx.send(message).is_err() {
                    break;
                }
            }
        });

        let mut client = Self {
            child,
            stdin,
            rx,
            next_id: 0,
            notifications: Vec::new(),
            init_result: Value::Null,
        };
        client.init_result = client.request(
            "initialize",
            json!({ "processId": null, "rootUri": null, "capabilities": {} }),
        );
        client.notify("initialized", json!({}));
        client
    }

    fn send(&mut self, message: &Value) {
        let body = message.to_string();
        write!(self.stdin, "Content-Length: {}\r\n\r\n{}", body.len(), body).unwrap();
        self.stdin.flush().unwrap();
    }

    /// Sends a notification.
    pub fn notify(&mut self, method: &str, params: Value) {
        self.send(&json!({ "jsonrpc": "2.0", "method": method, "params": params }));
    }

    /// Sends a request and returns its `result` (panics on an error response).
    pub fn request(&mut self, method: &str, params: Value) -> Value {
        self.next_id += 1;
        let id = self.next_id;
        self.send(&json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }));
        loop {
            let message = self.rx.recv_timeout(TIMEOUT).expect("timed out waiting for response");
            if message.get("method").is_some() {
                if let Some(request_id) = message.get("id") {
                    // Server-to-client request: acknowledge with a null result.
                    self.send(&json!({ "jsonrpc": "2.0", "id": request_id, "result": null }));
                } else {
                    self.notifications.push(message);
                }
            } else if message["id"] == json!(id) {
                assert!(message.get("error").is_none(), "{method} failed: {message}");
                return message["result"].clone();
            }
        }
    }

    /// Opens a document with the given URI and text.
    pub fn open(&mut self, uri: &str, text: &str) {
        self.notify(
            "textDocument/didOpen",
            json!({ "textDocument": { "uri": uri, "languageId": "mq", "version": 1, "text": text } }),
        );
    }

    /// Request helper for position-based methods.
    pub fn at(&mut self, method: &str, uri: &str, line: u32, character: u32, extra: Value) -> Value {
        let mut params = json!({
            "textDocument": { "uri": uri },
            "position": { "line": line, "character": character },
        });
        if let (Some(p), Some(e)) = (params.as_object_mut(), extra.as_object()) {
            p.extend(e.clone());
        }
        self.request(method, params)
    }

    /// Waits for a `publishDiagnostics` notification for `uri` that satisfies `pred`.
    pub fn diagnostics_for(&mut self, uri: &str, pred: impl Fn(&Value) -> bool) -> Value {
        let matches = |m: &Value| {
            m["method"] == "textDocument/publishDiagnostics" && m["params"]["uri"] == uri && pred(&m["params"])
        };
        if let Some(pos) = self.notifications.iter().position(&matches) {
            return self.notifications.remove(pos)["params"].clone();
        }
        loop {
            let message = self
                .rx
                .recv_timeout(TIMEOUT)
                .expect("timed out waiting for diagnostics");
            if message.get("method").is_some() && message.get("id").is_some() {
                self.send(&json!({ "jsonrpc": "2.0", "id": message["id"], "result": null }));
            } else if matches(&message) {
                return message["params"].clone();
            } else if message.get("method").is_some() {
                self.notifications.push(message);
            }
        }
    }

    /// Performs the `shutdown`/`exit` sequence.
    pub fn shutdown(mut self) {
        self.request("shutdown", Value::Null);
        self.notify("exit", Value::Null);
        let _ = self.child.wait();
    }
}

impl Drop for LspClient {
    fn drop(&mut self) {
        let _ = self.child.kill();
    }
}

fn read_message(reader: &mut impl BufRead) -> Option<Value> {
    let mut length = None;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).ok()? == 0 {
            return None;
        }
        let line = line.trim();
        if line.is_empty() {
            break;
        }
        if let Some(value) = line.strip_prefix("Content-Length:") {
            length = value.trim().parse::<usize>().ok();
        }
    }
    let mut body = vec![0; length?];
    reader.read_exact(&mut body).ok()?;
    serde_json::from_slice(&body).ok()
}
