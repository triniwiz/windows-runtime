use crate::Runtime;
use serde_json::Value;
use std::thread;
use std::time::Duration;
use std::time::{SystemTime, UNIX_EPOCH};

fn unique_result_file(name: &str) -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let mut path = std::env::temp_dir();
    path.push(format!("windows_runtime_{}_{}.json", name, nanos));
    path.to_string_lossy().to_string()
}

// ── Timer integration tests ──────────────────────────────────────────────────

#[test]
fn timers_set_timeout_fires() {
    run_js_assert(
        "timers_set_timeout_fires",
        r#"
            return new Promise(function(resolve, reject) {
                __ns__setTimeout(function() { resolve(); }, 20);
            });
        "#,
    );
}

#[test]
fn timers_clear_timeout_prevents_fire() {
    run_js_assert(
        "timers_clear_timeout_prevents_fire",
        r#"
            return new Promise(function(resolve, reject) {
                let fired = false;
                let id = __ns__setTimeout(function() { fired = true; reject('timer fired'); }, 30);
                __ns__clearTimeout(id);
                __ns__setTimeout(function() {
                    if (!fired) resolve(); else reject('fired unexpectedly');
                }, 80);
            });
        "#,
    );
}

#[test]
fn timers_set_interval_repeats_and_clears() {
    run_js_assert(
        "timers_set_interval_repeats_and_clears",
        r#"
            return new Promise(function(resolve, reject) {
                let count = 0;
                let id = __ns__setInterval(function() {
                    count++;
                    if (count === 2) {
                        __ns__clearInterval(id);
                        __ns__setTimeout(function() {
                            if (count === 2) resolve(); else reject('unexpected tick count: ' + count);
                        }, 60);
                    } else if (count > 2) {
                        reject('too many ticks: ' + count);
                    }
                }, 15);
            });
        "#,
    );
}

fn run_js_assert(name: &str, body: &str) {
    let mut runtime = Box::new(Runtime::new("."));
    // Register the isolate pointer for Delegate/timer callbacks so they can
    // enter V8 from other threads. Box the runtime so its address is stable.
    runtime.register_delegate_isolate_ptr();
    let result_file = unique_result_file(name);
    let result_file_json = serde_json::to_string(&result_file).unwrap();
    let temp_dir_json = serde_json::to_string(&std::env::temp_dir().to_string_lossy().to_string()).unwrap();

    let script = format!(
        r#"
            (function() {{
                const __resultFile = {result_file};
                const __tempDir = {temp_dir};

                function __writeResult(ok, message) {{
                    if (typeof __nsProxyWriteTextFile !== "function") {{
                        throw new Error("__nsProxyWriteTextFile is not available in runtime");
                    }}

                    __nsProxyWriteTextFile(__resultFile, JSON.stringify({{
                        ok: !!ok,
                        message: String(message || "")
                    }}));
                }}

                function __errorMessage(e) {{
                    return (e && (e.stack || e.message))
                        ? String(e.stack || e.message)
                        : String(e);
                }}

                try {{
                    const __maybePromise = (function() {{
                        {body}
                    }})();

                    if (__maybePromise && typeof __maybePromise.then === "function") {{
                        __maybePromise.then(function () {{
                            __writeResult(true, "ok");
                        }}).catch(function (e) {{
                            __writeResult(false, __errorMessage(e));
                        }});
                    }} else {{
                        __writeResult(true, "ok");
                    }}
                }} catch (e) {{
                    __writeResult(false, __errorMessage(e));
                }}
            }})();
        "#,
        result_file = result_file_json,
        temp_dir = temp_dir_json,
        body = body,
    );

    runtime.run_script(&script, &format!("{}.js", name));

    let mut found = false;
    for _ in 0..200 {
        if std::path::Path::new(&result_file).exists() {
            found = true;
            break;
        }
        // Pump native timers while waiting so timer callbacks can run on the main thread.
        crate::timers::pump();
        thread::sleep(Duration::from_millis(10));
    }

    if !found {
        panic!("missing test result file for {name}: timed out waiting for JS assertion result");
    }

    let raw = std::fs::read_to_string(&result_file)
        .unwrap_or_else(|e| panic!("missing test result file for {name}: {e}"));
    let parsed: Value = serde_json::from_str(&raw)
        .unwrap_or_else(|e| panic!("invalid test result JSON for {name}: {e}, raw={raw}"));

    let ok = parsed.get("ok").and_then(Value::as_bool).unwrap_or(false);
    if !ok {
        let msg = parsed
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("unknown JS failure");
        panic!("interop test '{name}' failed: {msg}");
    }

    let _ = std::fs::remove_file(&result_file);
}

#[test]
fn typed_array_to_windows_buffer_roundtrip_readbyte() {
    run_js_assert(
        "typed_array_to_windows_buffer_roundtrip_readbyte",
        r#"
            const cb = Windows.Security.Cryptography.CryptographicBuffer;
            const input = new Uint8Array([1, 2, 3, 254]);
            const winBuf = cb.CreateFromByteArray(input);

            if (winBuf.Length !== 4) {
                throw new Error(`Expected Length=4, got ${winBuf.Length}`);
            }

            const reader = Windows.Storage.Streams.DataReader.FromBuffer(winBuf);
            const got = [reader.ReadByte(), reader.ReadByte(), reader.ReadByte(), reader.ReadByte()];
            const expected = [1, 2, 3, 254];

            for (let i = 0; i < expected.length; i++) {
                if (got[i] !== expected[i]) {
                    throw new Error(`Mismatch at ${i}: got ${got[i]}, expected ${expected[i]}`);
                }
            }
        "#,
    );
}


#[test]
fn typed_array_to_windows_buffer_exposes_length() {
    run_js_assert(
        "typed_array_to_windows_buffer_exposes_length",
        r#"
            const cb = Windows.Security.Cryptography.CryptographicBuffer;
            const input = new Uint8Array([1, 2, 3, 254]);
            const winBuf = cb.CreateFromByteArray(input);

            if (winBuf.Length !== 4) {
                throw new Error(`Expected Length=4, got ${winBuf.Length}`);
            }
        "#,
    );
}

#[test]
fn arraybuffer_to_windows_buffer_roundtrip_readbyte() {

    run_js_assert(
        "arraybuffer_to_windows_buffer_roundtrip_readbyte",
        r#"
            const cb = Windows.Security.Cryptography.CryptographicBuffer;
            const buffer = new ArrayBuffer(4);
            const view = new Uint8Array(buffer);
            view.set([9, 8, 7, 6]);

            const winBuf = cb.CreateFromByteArray(buffer);
            const reader = Windows.Storage.Streams.DataReader.FromBuffer(winBuf);

            const got = [reader.ReadByte(), reader.ReadByte(), reader.ReadByte(), reader.ReadByte()];
            const expected = [9, 8, 7, 6];
            for (let i = 0; i < expected.length; i++) {
                if (got[i] !== expected[i]) {
                    throw new Error(`Mismatch at ${i}: got ${got[i]}, expected ${expected[i]}`);
                }
            }
        "#,
    );
}


#[test]
fn typed_array_subarray_respects_byte_offset() {
    run_js_assert(
        "typed_array_subarray_respects_byte_offset",
        r#"
            const cb = Windows.Security.Cryptography.CryptographicBuffer;
            const source = new Uint8Array([99, 11, 22, 33, 77]);
            const slice = source.subarray(1, 4);
            const winBuf = cb.CreateFromByteArray(slice);
            const reader = Windows.Storage.Streams.DataReader.FromBuffer(winBuf);

            const got = [reader.ReadByte(), reader.ReadByte(), reader.ReadByte()];
            const expected = [11, 22, 33];
            for (let i = 0; i < expected.length; i++) {
                if (got[i] !== expected[i]) {
                    throw new Error(`Offset mismatch at ${i}: got ${got[i]}, expected ${expected[i]}`);
                }
            }
        "#,
    );
}

#[test]
fn windows_buffer_to_typed_array_via_datareader_readbytes() {
    run_js_assert(
        "windows_buffer_to_typed_array_via_datareader_readbytes",
        r#"
            const cb = Windows.Security.Cryptography.CryptographicBuffer;
            const winBuf = cb.CreateFromByteArray(new Uint8Array([42, 100, 7, 255]));

            const reader = Windows.Storage.Streams.DataReader.FromBuffer(winBuf);
            const out = new Uint8Array(4);
            reader.ReadBytes(out);

            const expected = [42, 100, 7, 255];
            for (let i = 0; i < expected.length; i++) {
                if (out[i] !== expected[i]) {
                    throw new Error(`ReadBytes mismatch at ${i}: got ${out[i]}, expected ${expected[i]}`);
                }
            }
        "#,
    );
}

#[test]
fn windows_buffer_to_arraybuffer_via_typedarray_view() {
    run_js_assert(
        "windows_buffer_to_arraybuffer_via_typedarray_view",
        r#"
            const cb = Windows.Security.Cryptography.CryptographicBuffer;
            const winBuf = cb.CreateFromByteArray(new Uint8Array([5, 4, 3, 2, 1]));

            const reader = Windows.Storage.Streams.DataReader.FromBuffer(winBuf);
            const outBuffer = new ArrayBuffer(5);
            const outView = new Uint8Array(outBuffer);
            reader.ReadBytes(outView);

            const expected = [5, 4, 3, 2, 1];
            for (let i = 0; i < expected.length; i++) {
                if (outView[i] !== expected[i]) {
                    throw new Error(`ArrayBuffer mismatch at ${i}: got ${outView[i]}, expected ${expected[i]}`);
                }
            }
        "#,
    );
}

#[test]
fn zero_length_typed_array_to_windows_buffer() {
    run_js_assert(
        "zero_length_typed_array_to_windows_buffer",
        r#"
            const cb = Windows.Security.Cryptography.CryptographicBuffer;
            const empty = new Uint8Array(0);
            const winBuf = cb.CreateFromByteArray(empty);
            if (winBuf.Length !== 0) {
                throw new Error(`Expected zero-length WinRT buffer, got ${winBuf.Length}`);
            }
        "#,
    );
}

#[test]
fn interop_as_buffer_source_accepts_arraybuffer_and_view() {
    run_js_assert(
        "interop_as_buffer_source_accepts_arraybuffer_and_view",
        r#"
            const interop = NSWinRT.interop;
            const ab = new ArrayBuffer(8);
            const ta = new Uint8Array(ab, 2, 4);

            const r1 = interop.asBufferSource(ab);
            const r2 = interop.asBufferSource(ta);

            if (!(r1 instanceof ArrayBuffer)) {
                throw new Error("Expected ArrayBuffer to be accepted as-is");
            }
            if (!ArrayBuffer.isView(r2)) {
                throw new Error("Expected typed array view to be accepted as-is");
            }
        "#,
    );
}

#[test]
fn interop_as_buffer_source_rejects_non_buffer_values() {
    run_js_assert(
        "interop_as_buffer_source_rejects_non_buffer_values",
        r#"
            const interop = NSWinRT.interop;
            let threw = false;
            try {
                interop.asBufferSource(123);
            } catch (_) {
                threw = true;
            }

            if (!threw) {
                throw new Error("Expected asBufferSource to reject non-buffer input");
            }
        "#,
    );
}

#[test]
fn interop_pointer_from_buffer_works_for_arraybuffer_and_view() {
    run_js_assert(
        "interop_pointer_from_buffer_works_for_arraybuffer_and_view",
        r#"
            const interop = NSWinRT.interop;
            const base = new Uint8Array([10, 20, 30, 40]);
            const ab = base.buffer;
            const view = base.subarray(1, 3);

            const p1 = interop.pointerFromBuffer(ab);
            const p2 = interop.pointerFromBuffer(view);

            if (p1 == null || p2 == null) {
                throw new Error("Expected non-null pointers for ArrayBuffer and view");
            }

            const k1 = interop.pointerKey(p1);
            const k2 = interop.pointerKey(p2);
            if (k1 == null || k2 == null) {
                throw new Error("Expected pointer keys to be available");
            }

            if (k1 === k2) {
                throw new Error("Expected different pointer keys for full buffer and offset view");
            }
        "#,
    );
}

#[test]
fn interop_track_and_resolve_buffer_source_roundtrip() {
    run_js_assert(
        "interop_track_and_resolve_buffer_source_roundtrip",
        r#"
            const interop = NSWinRT.interop;
            const bytes = new Uint8Array([3, 1, 4, 1, 5]);

            const pointer = interop.trackBufferSource(bytes);
            const resolved = interop.resolveTrackedBuffer(pointer);

            if (!resolved) {
                throw new Error("Expected tracked buffer to resolve by pointer");
            }

            const out = interop.asUint8View(resolved);
            if (out.length !== 5) {
                throw new Error(`Resolved length mismatch: ${out.length}`);
            }

            const expected = [3, 1, 4, 1, 5];
            for (let i = 0; i < expected.length; i++) {
                if (out[i] !== expected[i]) {
                    throw new Error(`Resolved bytes mismatch at ${i}: got ${out[i]}, expected ${expected[i]}`);
                }
            }
        "#,
    );
}

#[test]
fn interop_byte_helpers_read_and_write_roundtrip() {
    run_js_assert(
        "interop_byte_helpers_read_and_write_roundtrip",
        r#"
            const interop = NSWinRT.interop;
            const bytes = new Uint8Array(16);

            interop.writeU8(bytes, 0, 0xAA);
            interop.writeI32(bytes, 4, -123456789, true);
            interop.writeF32(bytes, 8, 3.25, true);
            interop.writeF64(bytes, 8, Math.PI, true);

            const gotU8 = interop.readU8(bytes, 0);
            if (gotU8 !== 0xAA) {
                throw new Error(`readU8 mismatch: ${gotU8}`);
            }

            const i32 = interop.readI32(bytes, 4, true);
            if (i32 !== -123456789) {
                throw new Error(`readI32 mismatch: ${i32}`);
            }

            const f64 = interop.readF64(bytes, 8, true);
            if (Math.abs(f64 - Math.PI) > 1e-12) {
                throw new Error(`readF64 mismatch: ${f64}`);
            }
        "#,
    );
}

#[test]
fn dynamic_import_loads_module_from_file_path() {
    run_js_assert(
        "dynamic_import_loads_module_from_file_path",
        r#"
            const modulePath = __tempDir + "\\runtime_dynamic_import_test.js";
            __nsProxyWriteTextFile(modulePath, "export const answer = 42; export default 7;");

            return import(modulePath).then((mod) => {
                if (mod.answer !== 42) {
                    throw new Error(`Expected named export answer=42, got ${mod.answer}`);
                }
                if (mod.default !== 7) {
                    throw new Error(`Expected default export=7, got ${mod.default}`);
                }
            });
        "#,
    );
}

#[test]
fn message_channel_delivers_messages_across_ports() {
    run_js_assert(
        "message_channel_delivers_messages_across_ports",
        r#"
            return new Promise((resolve, reject) => {
                const channel = new MessageChannel();

                channel.port2.onmessage = (event) => {
                    if (event.data !== 123) {
                        reject(new Error(`Expected event.data=123, got ${event.data}`));
                        return;
                    }
                    resolve();
                };

                channel.port1.postMessage(123);
            });
        "#,
    );
}

// ─── MessagePort ────────────────────────────────────────────────────────────

#[test]
fn message_port_close_prevents_delivery() {
    run_js_assert(
        "message_port_close_prevents_delivery",
        r#"
            return new Promise((resolve, reject) => {
                const { port1, port2 } = new MessageChannel();
                port2.close();

                port2.onmessage = () => {
                    reject(new Error("Message delivered to a closed port"));
                };

                port1.postMessage("should be dropped");

                // queueMicrotask runs after any already-queued microtasks, so
                // delivery would have fired before this if the port were open.
                queueMicrotask(() => resolve());
            });
        "#,
    );
}

#[test]
fn message_port_bidirectional_communication() {
    run_js_assert(
        "message_port_bidirectional_communication",
        r#"
            return new Promise((resolve, reject) => {
                const { port1, port2 } = new MessageChannel();

                port2.onmessage = (event) => {
                    if (event.data !== "ping") {
                        reject(new Error(`port2: expected 'ping', got ${event.data}`));
                        return;
                    }
                    port2.postMessage("pong");
                };

                port1.onmessage = (event) => {
                    if (event.data !== "pong") {
                        reject(new Error(`port1: expected 'pong', got ${event.data}`));
                        return;
                    }
                    resolve();
                };

                port1.postMessage("ping");
            });
        "#,
    );
}

#[test]
fn message_port_multiple_listeners_all_receive() {
    run_js_assert(
        "message_port_multiple_listeners_all_receive",
        r#"
            return new Promise((resolve, reject) => {
                const { port1, port2 } = new MessageChannel();
                let hits = 0;

                function checkDone() {
                    hits++;
                    if (hits === 2) resolve();
                }

                port2.addEventListener("message", (event) => {
                    if (event.data !== "hello") {
                        reject(new Error(`listener1: unexpected data ${event.data}`));
                    } else {
                        checkDone();
                    }
                });

                port2.addEventListener("message", (event) => {
                    if (event.data !== "hello") {
                        reject(new Error(`listener2: unexpected data ${event.data}`));
                    } else {
                        checkDone();
                    }
                });

                port1.postMessage("hello");
            });
        "#,
    );
}

#[test]
fn message_port_remove_event_listener_stops_delivery() {
    run_js_assert(
        "message_port_remove_event_listener_stops_delivery",
        r#"
            return new Promise((resolve, reject) => {
                const { port1, port2 } = new MessageChannel();
                let removedFired = false;
                let keptFired    = false;

                const removedListener = () => { removedFired = true; };
                const keptListener    = (event) => {
                    keptFired = true;
                    if (removedFired) {
                        reject(new Error("Removed listener still fired"));
                    } else {
                        resolve();
                    }
                };

                port2.addEventListener("message", removedListener);
                port2.addEventListener("message", keptListener);
                port2.removeEventListener("message", removedListener);

                port1.postMessage("test");
            });
        "#,
    );
}

// ─── Worker ─────────────────────────────────────────────────────────────────

#[test]
fn worker_supports_eval_and_file_path_sources() {
    run_js_assert(
        "worker_supports_eval_and_file_path_sources",
        r#"
            function runWorker(worker) {
                return new Promise((resolve, reject) => {
                    worker.onmessage = (event) => {
                        if (event.data !== 42) {
                            reject(new Error(`Expected worker response 42, got ${event.data}`));
                            return;
                        }
                        worker.terminate();
                        resolve();
                    };
                    worker.postMessage(41);
                });
            }

            const evalWorker = new Worker("self.onmessage = function (event) { self.postMessage(event.data + 1); }", { eval: true });

            const workerPath = __tempDir + "\\runtime_worker_file_test.js";
            __nsProxyWriteTextFile(workerPath, "self.onmessage = function (event) { self.postMessage(event.data + 1); }");
            const fileWorker = new Worker(workerPath);

            return runWorker(evalWorker).then(() => runWorker(fileWorker));
        "#,
    );
}

#[test]
fn worker_multiple_sequential_roundtrips() {
    run_js_assert(
        "worker_multiple_sequential_roundtrips",
        r#"
            return new Promise((resolve, reject) => {
                const worker = new Worker(
                    "self.onmessage = function (e) { self.postMessage(e.data * 2); }",
                    { eval: true }
                );

                const replies = [];

                worker.onmessage = (event) => { replies.push(event.data); };

                worker.postMessage(1);
                worker.postMessage(2);
                worker.postMessage(3);

                // All three dispatches were queued before this microtask runs.
                Promise.resolve().then(() => {
                    worker.terminate();
                    const expected = [2, 4, 6];
                    for (let i = 0; i < expected.length; i++) {
                        if (replies[i] !== expected[i]) {
                            reject(new Error(`Round ${i}: expected ${expected[i]}, got ${replies[i]}`));
                            return;
                        }
                    }
                    resolve();
                });
            });
        "#,
    );
}

#[test]
fn worker_complex_object_roundtrip() {
    run_js_assert(
        "worker_complex_object_roundtrip",
        r#"
            return new Promise((resolve, reject) => {
                const worker = new Worker(
                    "self.onmessage = function (e) { self.postMessage(e.data); }",
                    { eval: true }
                );

                const payload = { x: 42, arr: [1, "two", true], nested: { ok: true } };

                worker.onmessage = (event) => {
                    worker.terminate();
                    const got = event.data;
                    if (
                        got.x !== 42 ||
                        got.arr[0] !== 1 ||
                        got.arr[1] !== "two" ||
                        got.arr[2] !== true ||
                        !got.nested.ok
                    ) {
                        reject(new Error(`Payload mismatch: ${JSON.stringify(got)}`));
                    } else {
                        resolve();
                    }
                };

                worker.postMessage(payload);
            });
        "#,
    );
}

#[test]
fn worker_terminate_prevents_message_delivery() {
    run_js_assert(
        "worker_terminate_prevents_message_delivery",
        r#"
            // postMessage after terminate() must be a silent no-op.
            const worker = new Worker(
                "self.onmessage = function (e) { self.postMessage('reply'); }",
                { eval: true }
            );

            let received = false;
            worker.onmessage = () => { received = true; };

            worker.terminate();
            worker.postMessage("should be ignored");

            if (received) {
                throw new Error("Worker delivered a message after terminate()");
            }
        "#,
    );
}

#[test]
fn worker_add_event_listener_receives_messages() {
    run_js_assert(
        "worker_add_event_listener_receives_messages",
        r#"
            return new Promise((resolve, reject) => {
                const worker = new Worker(
                    "self.onmessage = function (e) { self.postMessage(e.data + 10); }",
                    { eval: true }
                );

                worker.addEventListener("message", (event) => {
                    worker.terminate();
                    if (event.data !== 15) {
                        reject(new Error(`Expected 15, got ${event.data}`));
                    } else {
                        resolve();
                    }
                });

                worker.postMessage(5);
            });
        "#,
    );
}

// ─── Structured Clone ────────────────────────────────────────────────────────

#[test]
fn worker_structured_clone_date_roundtrip() {
    run_js_assert(
        "worker_structured_clone_date_roundtrip",
        r#"
            return new Promise((resolve, reject) => {
                const worker = new Worker(
                    "self.onmessage = function (e) { self.postMessage(e.data); }",
                    { eval: true }
                );

                const sent = new Date(2025, 0, 15, 12, 30, 0);

                worker.onmessage = (event) => {
                    worker.terminate();
                    const got = event.data;
                    if (!(got instanceof Date)) {
                        reject(new Error(`Expected Date, got ${typeof got}`));
                    } else if (got.getTime() !== sent.getTime()) {
                        reject(new Error(`Date mismatch: ${got.toISOString()} vs ${sent.toISOString()}`));
                    } else {
                        resolve();
                    }
                };

                worker.postMessage(sent);
            });
        "#,
    );
}

#[test]
fn worker_structured_clone_arraybuffer_roundtrip() {
    run_js_assert(
        "worker_structured_clone_arraybuffer_roundtrip",
        r#"
            return new Promise((resolve, reject) => {
                const worker = new Worker(
                    "self.onmessage = function (e) { self.postMessage(e.data); }",
                    { eval: true }
                );

                const original = new Uint8Array([10, 20, 30, 40, 50]);
                const sentBuffer = original.buffer;

                worker.onmessage = (event) => {
                    worker.terminate();
                    const got = event.data;
                    if (!(got instanceof ArrayBuffer)) {
                        reject(new Error(`Expected ArrayBuffer, got ${Object.prototype.toString.call(got)}`));
                        return;
                    }
                    const view = new Uint8Array(got);
                    const expected = [10, 20, 30, 40, 50];
                    for (let i = 0; i < expected.length; i++) {
                        if (view[i] !== expected[i]) {
                            reject(new Error(`Byte mismatch at ${i}: ${view[i]} !== ${expected[i]}`));
                            return;
                        }
                    }
                    resolve();
                };

                worker.postMessage(sentBuffer);
            });
        "#,
    );
}

#[test]
fn worker_structured_clone_typed_array_roundtrip() {
    run_js_assert(
        "worker_structured_clone_typed_array_roundtrip",
        r#"
            return new Promise((resolve, reject) => {
                const worker = new Worker(
                    "self.onmessage = function (e) { self.postMessage(e.data); }",
                    { eval: true }
                );

                const sent = new Float32Array([1.5, 2.5, 3.5]);

                worker.onmessage = (event) => {
                    worker.terminate();
                    const got = event.data;
                    if (!(got instanceof Float32Array)) {
                        reject(new Error(`Expected Float32Array, got ${Object.prototype.toString.call(got)}`));
                        return;
                    }
                    if (got.length !== 3 || got[0] !== 1.5 || got[1] !== 2.5 || got[2] !== 3.5) {
                        reject(new Error(`Float32Array mismatch: ${Array.from(got)}`));
                    } else {
                        resolve();
                    }
                };

                worker.postMessage(sent);
            });
        "#,
    );
}

#[test]
fn worker_structured_clone_map_and_set_roundtrip() {
    run_js_assert(
        "worker_structured_clone_map_and_set_roundtrip",
        r#"
            return new Promise((resolve, reject) => {
                const worker = new Worker(
                    "self.onmessage = function (e) { self.postMessage(e.data); }",
                    { eval: true }
                );

                const sentMap = new Map([["a", 1], ["b", 2]]);
                const sentSet = new Set([10, 20, 30]);
                const payload = { m: sentMap, s: sentSet };

                worker.onmessage = (event) => {
                    worker.terminate();
                    const { m, s } = event.data;

                    if (!(m instanceof Map)) {
                        reject(new Error(`Expected Map, got ${Object.prototype.toString.call(m)}`)); return;
                    }
                    if (m.get("a") !== 1 || m.get("b") !== 2) {
                        reject(new Error(`Map contents wrong: ${JSON.stringify([...m])}`)); return;
                    }
                    if (!(s instanceof Set)) {
                        reject(new Error(`Expected Set, got ${Object.prototype.toString.call(s)}`)); return;
                    }
                    if (!s.has(10) || !s.has(20) || !s.has(30)) {
                        reject(new Error(`Set contents wrong: ${JSON.stringify([...s])}`)); return;
                    }
                    resolve();
                };

                worker.postMessage(payload);
            });
        "#,
    );
}

#[test]
fn worker_structured_clone_non_cloneable_throws() {
    run_js_assert(
        "worker_structured_clone_non_cloneable_throws",
        r#"
            const worker = new Worker(
                "self.onmessage = function (e) { self.postMessage('should not reach'); }",
                { eval: true }
            );

            // Functions are not cloneable by the structured clone algorithm.
            let threw = false;
            try {
                worker.postMessage(function() {});
            } catch (e) {
                threw = true;
            }

            worker.terminate();

            if (!threw) {
                throw new Error("Expected DataCloneError for function, but no exception was thrown");
            }
        "#,
    );
}

#[test]
fn worker_structured_clone_circular_ref_supported() {
    run_js_assert(
        "worker_structured_clone_circular_ref_supported",
        r#"
            return new Promise(function(resolve, reject) {
                const worker = new Worker(
                    "self.onmessage = function (e) { self.postMessage(typeof e.data.self); };",
                    { eval: true }
                );

                const obj = {};
                obj.self = obj; // circular reference — supported by structured clone

                worker.onmessage = function(e) {
                    worker.terminate();
                    if (e.data !== 'object') {
                        reject(new Error('Expected circular self-ref to survive roundtrip, got: ' + e.data));
                    } else {
                        resolve();
                    }
                };

                worker.postMessage(obj);
            });
        "#,
    );
}

// ─── Delegate ────────────────────────────────────────────────────────────────

#[test]
fn as_delegate_accepts_plain_function() {
    run_js_assert(
        "as_delegate_accepts_plain_function",
        r#"
            let called = false;
            const fn = NSWinRT.asDelegate(function () { called = true; });
            if (typeof fn !== 'function') {
                throw new Error(`Expected function, got ${typeof fn}`);
            }
            fn();
            if (!called) {
                throw new Error("asDelegate-wrapped function was not called");
            }
        "#,
    );
}

#[test]
fn as_delegate_accepts_invoke_object() {
    run_js_assert(
        "as_delegate_accepts_invoke_object",
        r#"
            let received = null;
            const obj = {
                tag: 'myObj',
                invoke: function (x) { received = this.tag + ':' + x; },
            };
            const fn = NSWinRT.asDelegate(obj);
            if (typeof fn !== 'function') {
                throw new Error(`Expected function, got ${typeof fn}`);
            }
            fn(42);
            if (received !== 'myObj:42') {
                throw new Error(`Expected 'myObj:42', got '${received}'`);
            }
        "#,
    );
}

#[test]
fn as_delegate_rejects_non_callable() {
    run_js_assert(
        "as_delegate_rejects_non_callable",
        r#"
            const badInputs = [42, "string", true, {}];
            for (const input of badInputs) {
                let threw = false;
                try {
                    NSWinRT.asDelegate(input);
                } catch (_) {
                    threw = true;
                }
                if (!threw) {
                    throw new Error(`Expected asDelegate(${JSON.stringify(input)}) to throw`);
                }
            }
        "#,
    );
}

#[test]
fn as_delegate_with_invoke_object_preserves_this_binding() {
    run_js_assert(
        "as_delegate_with_invoke_object_preserves_this_binding",
        r#"
            const obj = {
                value: 'bound',
                invoke: function () { return this.value; },
            };
            const fn = NSWinRT.asDelegate(obj);
            const result = fn();
            if (result !== 'bound') {
                throw new Error(`Expected 'bound', got '${result}'`);
            }
        "#,
    );
}

#[test]
fn event_emitter_add_and_emit() {
    run_js_assert(
        "event_emitter_add_and_emit",
        r#"
            const emitter = NSWinRT.createEventEmitter();
            let callCount = 0;
            emitter.add(function () { callCount++; });

            emitter.emit();
            emitter.emit();

            if (callCount !== 2) {
                throw new Error(`Expected listener called 2 times, got ${callCount}`);
            }
        "#,
    );
}

#[test]
fn event_emitter_emit_passes_arguments_to_listeners() {
    run_js_assert(
        "event_emitter_emit_passes_arguments_to_listeners",
        r#"
            const emitter = NSWinRT.createEventEmitter();
            let gotA = null;
            let gotB = null;
            emitter.add(function (a, b) { gotA = a; gotB = b; });

            emitter.emit('hello', 99);

            if (gotA !== 'hello' || gotB !== 99) {
                throw new Error(`Expected ('hello', 99), got (${gotA}, ${gotB})`);
            }
        "#,
    );
}

#[test]
fn event_emitter_dispose_stops_listener() {
    run_js_assert(
        "event_emitter_dispose_stops_listener",
        r#"
            const emitter = NSWinRT.createEventEmitter();
            let callCount = 0;
            const subscription = emitter.add(function () { callCount++; });

            emitter.emit();   // callCount → 1
            subscription.dispose();
            emitter.emit();   // disposed: should not increment

            if (callCount !== 1) {
                throw new Error(`Expected listener called once after dispose, got ${callCount}`);
            }
        "#,
    );
}

#[test]
fn event_emitter_count_tracks_listeners() {
    run_js_assert(
        "event_emitter_count_tracks_listeners",
        r#"
            const emitter = NSWinRT.createEventEmitter();
            if (emitter.count() !== 0) {
                throw new Error(`Expected initial count 0, got ${emitter.count()}`);
            }

            const s1 = emitter.add(function () {});
            if (emitter.count() !== 1) {
                throw new Error(`Expected count 1 after add, got ${emitter.count()}`);
            }

            const s2 = emitter.add(function () {});
            if (emitter.count() !== 2) {
                throw new Error(`Expected count 2 after second add, got ${emitter.count()}`);
            }

            s1.dispose();
            if (emitter.count() !== 1) {
                throw new Error(`Expected count 1 after first dispose, got ${emitter.count()}`);
            }

            s2.dispose();
            if (emitter.count() !== 0) {
                throw new Error(`Expected count 0 after all disposed, got ${emitter.count()}`);
            }
        "#,
    );
}

#[test]
fn event_emitter_multiple_listeners_all_called() {
    run_js_assert(
        "event_emitter_multiple_listeners_all_called",
        r#"
            const emitter = NSWinRT.createEventEmitter();
            const results = [];
            emitter.add(function (x) { results.push('a:' + x); });
            emitter.add(function (x) { results.push('b:' + x); });
            emitter.add(function (x) { results.push('c:' + x); });

            emitter.emit(7);

            const expected = ['a:7', 'b:7', 'c:7'];
            if (results.length !== expected.length) {
                throw new Error(`Expected ${expected.length} calls, got ${results.length}: ${JSON.stringify(results)}`);
            }
            for (let i = 0; i < expected.length; i++) {
                if (results[i] !== expected[i]) {
                    throw new Error(`At index ${i}: expected '${expected[i]}', got '${results[i]}'`);
                }
            }
        "#,
    );
}

#[test]
fn event_emitter_emit_snapshot_prevents_late_add_being_called() {
    run_js_assert(
        "event_emitter_emit_snapshot_prevents_late_add_being_called",
        r#"
            // A listener added during emit should not be called in the same emit
            const emitter = NSWinRT.createEventEmitter();
            let lateCallCount = 0;

            emitter.add(function () {
                emitter.add(function () { lateCallCount++; });
            });

            emitter.emit();

            if (lateCallCount !== 0) {
                throw new Error(`Listener added during emit was called ${lateCallCount} time(s); expected 0`);
            }
        "#,
    );
}

// ── WinRT delegate creation tests ────────────────────────────────────────────
//
// These tests exercise the JsDelegate COM bridge: when a plain JS function is
// passed to a WinRT delegate constructor, the runtime should return a
// `{ handle: External }` object whose handle is a valid COM pointer.
//
// Windows.Foundation metadata is required for these tests to fully assert;
// when the type is not resolvable the tests skip gracefully.

#[test]
fn delegate_constructor_with_fn_returns_handle_object() {
    run_js_assert(
        "delegate_constructor_with_fn_returns_handle_object",
        r#"
            function resolvePath(path) {
                return path.split('.').reduce(function(o, k) {
                    return (o && o[k] !== undefined) ? o[k] : null;
                }, globalThis);
            }

            const candidates = [
                'Windows.Foundation.EventHandler_1_Object',
                'Windows.Foundation.TypedEventHandler_2_Object_Object',
            ];

            for (const name of candidates) {
                const DelegateType = resolvePath(name);
                if (!DelegateType) continue;

                const handler = new DelegateType(function(sender, args) {});
                if (!handler) {
                    throw new Error(name + ' constructor returned falsy');
                }
                const hasHandle = handler.handle !== undefined && handler.handle !== null;
                const hasImpl   = typeof handler['__implementation__'] !== 'undefined';
                if (!hasHandle && !hasImpl) {
                    throw new Error(
                        name + ' constructor returned object with neither handle nor __implementation__'
                    );
                }
                break;
            }
        "#,
    );
}

#[test]
fn delegate_constructor_with_invoke_object_returns_handle() {
    run_js_assert(
        "delegate_constructor_with_invoke_object_returns_handle",
        r#"
            function resolvePath(path) {
                return path.split('.').reduce(function(o, k) {
                    return (o && o[k] !== undefined) ? o[k] : null;
                }, globalThis);
            }

            const candidates = [
                'Windows.Foundation.EventHandler_1_Object',
                'Windows.Foundation.TypedEventHandler_2_Object_Object',
            ];

            for (const name of candidates) {
                const DelegateType = resolvePath(name);
                if (!DelegateType) continue;

                // Capital Invoke — WinRT canonical method name
                const h1 = new DelegateType({ Invoke: function(sender, args) {} });
                if (!h1 || h1.handle === undefined || h1.handle === null) {
                    throw new Error(name + ': { Invoke: fn } should produce a handle');
                }

                // Lowercase invoke — NativeScript Android-style alias
                const h2 = new DelegateType({ invoke: function(sender, args) {} });
                if (!h2 || h2.handle === undefined || h2.handle === null) {
                    throw new Error(name + ': { invoke: fn } should produce a handle');
                }

                // Shorthand method syntax
                const h3 = new DelegateType({ Invoke(sender, args) {} });
                if (!h3 || h3.handle === undefined || h3.handle === null) {
                    throw new Error(name + ': { Invoke(){} } shorthand should produce a handle');
                }
                break;
            }
        "#,
    );
}

#[test]
fn delegate_constructor_with_plain_object_falls_through() {
    run_js_assert(
        "delegate_constructor_with_plain_object_falls_through",
        r#"
            function resolvePath(path) {
                return path.split('.').reduce(function(o, k) {
                    return (o && o[k] !== undefined) ? o[k] : null;
                }, globalThis);
            }

            const candidates = [
                'Windows.Foundation.EventHandler_1_Object',
                'Windows.Foundation.TypedEventHandler_2_Object_Object',
            ];

            for (const name of candidates) {
                const DelegateType = resolvePath(name);
                if (!DelegateType) continue;

                // An object with no Invoke/invoke method falls through to __implementation__.
                const impl = { onEvent: function(sender, args) {} };
                const handler = new DelegateType(impl);
                if (!handler) {
                    throw new Error(name + ' constructor returned falsy for plain object arg');
                }
                if (handler.handle !== undefined && handler.handle !== null) {
                    throw new Error(name + ': plain object without Invoke should not have a handle');
                }
                break;
            }
        "#,
    );
}
