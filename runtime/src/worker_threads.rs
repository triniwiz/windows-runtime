use crate::Runtime;
use parking_lot::{Mutex, RwLock};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::sync::{Arc, OnceLock};
use std::thread;
use std::time::Duration;

#[derive(Debug)]
enum WorkerCommand {
    PostMessage(Vec<u8>),
    Terminate,
}

#[derive(Debug)]
enum WorkerEvent {
    Message(Vec<u8>),
    Error(String),
    Exited,
}

#[derive(Debug)]
struct WorkerHandle {
    tx: Sender<WorkerCommand>,
    /// Wrapped in Arc<Mutex> so the WORKERS registry lock can be released before
    /// a blocking recv — otherwise poll_events_blocking would hold the global
    /// registry lock for the entire timeout duration, starving create/terminate.
    rx: Arc<Mutex<Receiver<WorkerEvent>>>,
    join: thread::JoinHandle<()>,
}

#[derive(Debug)]
pub enum PolledWorkerEvent {
    Message(Vec<u8>),
    Error(String),
    Exited,
}

static NEXT_WORKER_ID: AtomicU64 = AtomicU64::new(1);
static WORKERS: OnceLock<RwLock<HashMap<u64, WorkerHandle>>> = OnceLock::new();

fn workers() -> &'static RwLock<HashMap<u64, WorkerHandle>> {
    WORKERS.get_or_init(|| RwLock::new(HashMap::new()))
}

fn worker_bootstrap_script(source: &str, filename: &str) -> Result<String, String> {
    let source_json = serde_json::to_string(source)
        .map_err(|e| format!("Failed to serialize worker source: {e}"))?;
    let filename_json = serde_json::to_string(filename)
        .map_err(|e| format!("Failed to serialize worker filename: {e}"))?;

    Ok(format!(
        r#"
            (function () {{
                const __workerSource = {source};
                const __workerFilename = {filename};
                const __listeners = [];

                globalThis.__nsWorkerOutbox = [];
                globalThis.self = globalThis;
                globalThis.postMessage = function (data) {{
                    globalThis.__nsWorkerOutbox.push(data);
                }};

                globalThis.addEventListener = function (type, listener) {{
                    if (type !== 'message' || typeof listener !== 'function') {{
                        return;
                    }}
                    if (__listeners.indexOf(listener) < 0) {{
                        __listeners.push(listener);
                    }}
                }};

                globalThis.removeEventListener = function (type, listener) {{
                    if (type !== 'message' || typeof listener !== 'function') {{
                        return;
                    }}
                    const index = __listeners.indexOf(listener);
                    if (index >= 0) {{
                        __listeners.splice(index, 1);
                    }}
                }};

                globalThis.__nsDispatchToWorker = function (data) {{
                    const event = {{
                        type: 'message',
                        data: data,
                        target: globalThis,
                        currentTarget: globalThis,
                        ports: []
                    }};

                    if (typeof globalThis.onmessage === 'function') {{
                        globalThis.onmessage(event);
                    }}

                    __listeners.slice().forEach(function (listener) {{
                        listener.call(globalThis, event);
                    }});
                }};

                if (typeof globalThis.__nsEvalAsModule === 'function') {{
                    globalThis.__nsEvalAsModule(__workerSource, __workerFilename || '[worker]');
                }} else {{
                    const exec = new Function('__filename', __workerSource);
                    exec(__workerFilename || '[worker]');
                }}
            }})();
        "#,
        source = source_json,
        filename = filename_json
    ))
}

pub fn create_worker(app_root: String, source: String, filename: String) -> Result<u64, String> {
    let worker_id = NEXT_WORKER_ID.fetch_add(1, Ordering::Relaxed);

    let (cmd_tx, cmd_rx) = mpsc::channel::<WorkerCommand>();
    let (evt_tx, evt_rx) = mpsc::channel::<WorkerEvent>();
    let evt_rx = Arc::new(Mutex::new(evt_rx));

    let join = thread::Builder::new()
        .name(format!("ns-worker-{worker_id}"))
        .spawn(move || {
            let mut runtime = Runtime::new(app_root.as_str());

            match worker_bootstrap_script(source.as_str(), filename.as_str()) {
                Ok(script) => runtime.run_script(script.as_str(), filename.as_str()),
                Err(err) => {
                    let _ = evt_tx.send(WorkerEvent::Error(err));
                    let _ = evt_tx.send(WorkerEvent::Exited);
                    return;
                }
            }

            for result in runtime.drain_outbox_bytes() {
                match result {
                    Ok(bytes) => {
                        let _ = evt_tx.send(WorkerEvent::Message(bytes));
                    }
                    Err(err) => {
                        let _ = evt_tx.send(WorkerEvent::Error(err));
                    }
                }
            }

            loop {
                match cmd_rx.recv() {
                    Ok(WorkerCommand::PostMessage(bytes)) => {
                        runtime.dispatch_to_worker(&bytes);
                        for result in runtime.drain_outbox_bytes() {
                            match result {
                                Ok(b) => {
                                    let _ = evt_tx.send(WorkerEvent::Message(b));
                                }
                                Err(e) => {
                                    let _ = evt_tx.send(WorkerEvent::Error(e));
                                }
                            }
                        }
                    }
                    Ok(WorkerCommand::Terminate) | Err(_) => {
                        break;
                    }
                }
            }

            let _ = evt_tx.send(WorkerEvent::Exited);
        })
        .map_err(|e| format!("Failed to spawn worker thread: {e}"))?;

    workers().write().insert(
        worker_id,
        WorkerHandle {
            tx: cmd_tx,
            rx: evt_rx,
            join,
        },
    );

    Ok(worker_id)
}

pub fn post_message(worker_id: u64, payload_bytes: Vec<u8>) -> Result<(), String> {
    let workers = workers().read();
    let Some(worker) = workers.get(&worker_id) else {
        return Err(format!("Unknown worker id: {worker_id}"));
    };

    worker
        .tx
        .send(WorkerCommand::PostMessage(payload_bytes))
        .map_err(|e| format!("Failed to send worker message: {e}"))
}

fn collect_events(rx: &Receiver<WorkerEvent>) -> Vec<PolledWorkerEvent> {
    let mut events = Vec::new();
    loop {
        match rx.try_recv() {
            Ok(WorkerEvent::Message(bytes)) => events.push(PolledWorkerEvent::Message(bytes)),
            Ok(WorkerEvent::Error(err)) => events.push(PolledWorkerEvent::Error(err)),
            Ok(WorkerEvent::Exited) => {
                events.push(PolledWorkerEvent::Exited);
                break;
            }
            Err(TryRecvError::Empty) => break,
            Err(TryRecvError::Disconnected) => {
                events.push(PolledWorkerEvent::Exited);
                break;
            }
        }
    }
    events
}

pub fn poll_events(worker_id: u64) -> Result<Vec<PolledWorkerEvent>, String> {
    // Clone the Arc so we can release the registry lock before draining.
    let rx = {
        let workers = workers().read();
        let Some(worker) = workers.get(&worker_id) else {
            return Err(format!("Unknown worker id: {worker_id}"));
        };
        Arc::clone(&worker.rx)
    };

    let guard = rx.lock();
    Ok(collect_events(&guard))
}

pub fn poll_events_blocking(
    worker_id: u64,
    timeout_ms: u64,
) -> Result<Vec<PolledWorkerEvent>, String> {
    // Clone the Arc and immediately release the registry lock so that
    // create_worker / terminate_worker are not starved for the full timeout.
    let rx = {
        let workers = workers().read();
        let Some(worker) = workers.get(&worker_id) else {
            return Err(format!("Unknown worker id: {worker_id}"));
        };
        Arc::clone(&worker.rx)
    };

    let rx = rx.lock();
    let mut events = Vec::new();

    match rx.recv_timeout(Duration::from_millis(timeout_ms)) {
        Ok(WorkerEvent::Message(bytes)) => events.push(PolledWorkerEvent::Message(bytes)),
        Ok(WorkerEvent::Error(err)) => events.push(PolledWorkerEvent::Error(err)),
        Ok(WorkerEvent::Exited) => events.push(PolledWorkerEvent::Exited),
        Err(_) => return Ok(events),
    }

    // Drain any additional events that arrived without blocking.
    events.extend(collect_events(&rx));

    Ok(events)
}

pub fn terminate_worker(worker_id: u64) -> Result<(), String> {
    let mut workers_guard = workers().write();
    let Some(worker) = workers_guard.remove(&worker_id) else {
        return Err(format!("Unknown worker id: {worker_id}"));
    };
    drop(workers_guard);

    let _ = worker.tx.send(WorkerCommand::Terminate);
    let _ = worker.join.join();

    Ok(())
}
