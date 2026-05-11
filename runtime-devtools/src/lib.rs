use anyhow::Result;
use parking_lot::Mutex;
use std::io::Write;
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::sync::Arc;
use std::thread;
use tungstenite::Message;
use v8::inspector::{
    Channel, ChannelImpl, StringBuffer, StringView, V8Inspector, V8InspectorClient,
    V8InspectorClientImpl, V8InspectorClientTrustLevel, V8InspectorSession,
};
use v8::UniquePtr;

// ─── Public types ─────────────────────────────────────────────────────────────

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DevtoolsServerConfig {
    pub host: String,
    pub port: u16,
}

impl Default for DevtoolsServerConfig {
    fn default() -> Self {
        // Port 42000: Windows runtime inspector.
        // NativeScript iOS runtime uses 40000, Android uses 41000 — Windows takes
        // the next slot so the CLI can auto-discover all three on the same host.
        Self { host: "127.0.0.1".to_string(), port: 42000 }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DevtoolsEndpoint {
    pub websocket_url: String,
    pub frontend_url: String,
}

// ─── Channel impl (V8 → WS outbound) ─────────────────────────────────────────

struct DevtoolsChannel {
    tx: SyncSender<String>,
}

fn string_buffer_to_string(buf: &StringBuffer) -> String {
    match buf.string() {
        StringView::U8(chars) => String::from_utf8_lossy(&chars).into_owned(),
        StringView::U16(chars) => String::from_utf16_lossy(&chars),
    }
}

impl ChannelImpl for DevtoolsChannel {
    fn send_response(&self, _call_id: i32, message: UniquePtr<StringBuffer>) {
        if let Some(buf) = message.as_ref() {
            let _ = self.tx.try_send(string_buffer_to_string(buf));
        }
    }
    fn send_notification(&self, message: UniquePtr<StringBuffer>) {
        if let Some(buf) = message.as_ref() {
            let _ = self.tx.try_send(string_buffer_to_string(buf));
        }
    }
    fn flush_protocol_notifications(&self) {}
}

// ─── Client impl (breakpoint pause loop) ─────────────────────────────────────

struct DevtoolsClient {
    paused: Arc<AtomicBool>,
    inbound_rx: Arc<Mutex<Receiver<String>>>,
    session_ptr: Arc<AtomicUsize>,
}

impl V8InspectorClientImpl for DevtoolsClient {
    fn run_message_loop_on_pause(&self, _ctx_group_id: i32) {
        self.paused.store(true, Ordering::Release);
        while self.paused.load(Ordering::Acquire) {
            if let Ok(msg) = self.inbound_rx.lock().try_recv() {
                let ptr = self.session_ptr.load(Ordering::Acquire);
                if ptr != 0 {
                    // SAFETY: session_ptr is published via Release from the V8 thread
                    // and zeroed in DevtoolsServer::drop before the Box is released.
                    // This callback also runs on the V8 thread, so there is no
                    // concurrent access to the session.
                    unsafe {
                        let view = StringView::from(msg.as_bytes());
                        (*(ptr as *const V8InspectorSession)).dispatch_protocol_message(view);
                    }
                }
            } else {
                thread::sleep(std::time::Duration::from_millis(5));
            }
        }
    }

    fn quit_message_loop_on_pause(&self) {
        self.paused.store(false, Ordering::Release);
    }
}

// ─── DevtoolsServer ───────────────────────────────────────────────────────────

pub struct DevtoolsServer {
    endpoint: DevtoolsEndpoint,
    inbound_rx: Arc<Mutex<Receiver<String>>>,
    session_ptr: Arc<AtomicUsize>,
    // Declare inspector before session so session is dropped first (LIFO field drops).
    _inspector: V8Inspector,
    _session: Box<V8InspectorSession>,
}

impl DevtoolsServer {
    /// Bind a TCP listener, start the background WS server thread, and wire the
    /// V8 inspector to `isolate`/`global_context`.  Must be called from the V8
    /// thread.  Takes a `Global<Context>` so that no active scope is required at
    /// the call site — a short-lived scope is created internally for the
    /// `context_created` registration.
    pub fn attach(
        config: &DevtoolsServerConfig,
        isolate: &mut v8::Isolate,
        global_context: &v8::Global<v8::Context>,
    ) -> Result<Self> {
        let listener = TcpListener::bind(format!("{}:{}", config.host, config.port))?;
        let addr = listener.local_addr()?;

        let websocket_url = format!("ws://{}/devtools/page/runtime", addr);
        let frontend_url = format!(
            "devtools://devtools/bundled/inspector.html?ws={}/devtools/page/runtime",
            addr
        );

        let (outbound_tx, outbound_rx) = mpsc::sync_channel::<String>(512);
        let (inbound_tx, inbound_rx) = mpsc::channel::<String>();
        let inbound_rx = Arc::new(Mutex::new(inbound_rx));

        let paused = Arc::new(AtomicBool::new(false));
        let session_ptr = Arc::new(AtomicUsize::new(0));

        let client = V8InspectorClient::new(Box::new(DevtoolsClient {
            paused,
            inbound_rx: Arc::clone(&inbound_rx),
            session_ptr: Arc::clone(&session_ptr),
        }));

        // create() only needs &mut Isolate — no active scope required.
        let inspector = V8Inspector::create(isolate, client);

        // Short-lived scope to localise the Global context for registration.
        {
            let name = b"NativeScript Runtime";
            v8::scope!(scope, isolate);
            let context = v8::Local::new(scope, global_context);
            inspector.context_created(context, 1, StringView::from(&name[..]), StringView::empty());
        }

        let session = Box::new(inspector.connect(
            1,
            Channel::new(Box::new(DevtoolsChannel { tx: outbound_tx })),
            StringView::empty(),
            V8InspectorClientTrustLevel::FullyTrusted,
        ));

        // The session is heap-allocated (Box), so its address is stable after this.
        // Release so any subsequent Acquire load on another thread sees the fully
        // initialized session.
        session_ptr.store(
            session.as_ref() as *const V8InspectorSession as usize,
            Ordering::Release,
        );

        let ws_url = websocket_url.clone();
        thread::Builder::new()
            .name("ns-devtools-server".to_string())
            .spawn(move || run_server(listener, inbound_tx, outbound_rx, ws_url))?;

        Ok(Self {
            endpoint: DevtoolsEndpoint { websocket_url, frontend_url },
            inbound_rx,
            session_ptr,
            _inspector: inspector,
            _session: session,
        })
    }

    pub fn endpoint(&self) -> &DevtoolsEndpoint {
        &self.endpoint
    }

    /// Dispatch any pending CDP messages that arrived from a connected DevTools
    /// client to V8.  Call this periodically from the V8 thread.
    pub fn pump_messages(&mut self) {
        let ptr = self.session_ptr.load(Ordering::Acquire);
        if ptr == 0 {
            return;
        }
        while let Ok(msg) = self.inbound_rx.lock().try_recv() {
            // SAFETY: same guarantee as in run_message_loop_on_pause.
            unsafe {
                let view = StringView::from(msg.as_bytes());
                (*(ptr as *const V8InspectorSession)).dispatch_protocol_message(view);
            }
        }
    }
}

impl Drop for DevtoolsServer {
    fn drop(&mut self) {
        // Zero before field destructors free the session (LIFO field drop order).
        self.session_ptr.store(0, Ordering::Release);
    }
}

// ─── Background TCP/WS server ─────────────────────────────────────────────────

fn run_server(
    listener: TcpListener,
    inbound_tx: mpsc::Sender<String>,
    outbound_rx: Receiver<String>,
    ws_url: String,
) {
    loop {
        let (stream, _addr) = match listener.accept() {
            Ok(s) => s,
            Err(_) => continue,
        };
        handle_connection(stream, &inbound_tx, &outbound_rx, &ws_url);
    }
}

fn handle_connection(
    stream: TcpStream,
    inbound_tx: &mpsc::Sender<String>,
    outbound_rx: &Receiver<String>,
    ws_url: &str,
) {
    // Peek at the request line to route without consuming any bytes.
    // TcpStream::peek() leaves data in the kernel receive buffer so that
    // tungstenite can re-read the full HTTP upgrade request.
    let mut peek_buf = [0u8; 256];
    let n = match stream.peek(&mut peek_buf) {
        Ok(n) => n,
        Err(_) => return,
    };
    let peek_str = String::from_utf8_lossy(&peek_buf[..n]);
    let path = peek_str.split_whitespace().nth(1).unwrap_or("/").to_string();

    if path.starts_with("/json") {
        serve_json(stream, &path, ws_url);
    } else {
        stream.set_nonblocking(false).ok();
        if let Ok(ws) = tungstenite::accept(stream) {
            handle_ws(ws, inbound_tx, outbound_rx);
        }
    }
}

fn serve_json(mut stream: TcpStream, path: &str, ws_url: &str) {
    let body = if path == "/json/version" {
        build_version_json()
    } else {
        build_list_json(ws_url)
    };
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json; charset=utf-8\r\n\
         Content-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    );
    let _ = stream.write_all(response.as_bytes());
}

fn handle_ws(
    mut ws: tungstenite::WebSocket<TcpStream>,
    inbound_tx: &mpsc::Sender<String>,
    outbound_rx: &Receiver<String>,
) {
    ws.get_mut().set_nonblocking(true).ok();

    loop {
        // Receive from the DevTools client.
        match ws.read() {
            Ok(Message::Text(text)) => {
                let _ = inbound_tx.send(text.to_string());
            }
            Ok(Message::Close(_)) => break,
            Ok(_) => {}
            Err(tungstenite::Error::Io(ref e))
                if e.kind() == std::io::ErrorKind::WouldBlock => {}
            Err(_) => break,
        }

        // Flush outbound messages (V8 → DevTools).
        while let Ok(msg) = outbound_rx.try_recv() {
            if ws.send(Message::Text(msg)).is_err() {
                return;
            }
        }

        thread::sleep(std::time::Duration::from_millis(1));
    }
}

// ─── CDP discovery JSON ───────────────────────────────────────────────────────

fn build_version_json() -> String {
    serde_json::json!({
        "Browser": "NativeScript/1.0",
        "Protocol-Version": "1.3",
        "V8-Version": "14.7",
        "webSocketDebuggerUrl": ""
    })
    .to_string()
}

fn build_list_json(ws_url: &str) -> String {
    let http_url = ws_url.replacen("ws://", "http://", 1);
    serde_json::json!([{
        "description": "NativeScript Runtime",
        "devtoolsFrontendUrl": format!(
            "devtools://devtools/bundled/inspector.html?ws={}",
            ws_url.trim_start_matches("ws://")
        ),
        "id": "nativescript-runtime",
        "title": "NativeScript Runtime",
        "type": "node",
        "url": http_url,
        "webSocketDebuggerUrl": ws_url
    }])
    .to_string()
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config() {
        let cfg = DevtoolsServerConfig::default();
        assert_eq!(cfg.host, "127.0.0.1");
        assert_eq!(cfg.port, 42000);
    }

    #[test]
    fn version_json_is_valid() {
        let v: serde_json::Value = serde_json::from_str(&build_version_json()).unwrap();
        assert!(v.get("Browser").is_some());
        assert!(v.get("Protocol-Version").is_some());
    }

    #[test]
    fn list_json_contains_ws_url() {
        let ws = "ws://127.0.0.1:42000/devtools/page/runtime";
        let v: serde_json::Value = serde_json::from_str(&build_list_json(ws)).unwrap();
        let arr = v.as_array().unwrap();
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0]["webSocketDebuggerUrl"], ws);
    }
}
