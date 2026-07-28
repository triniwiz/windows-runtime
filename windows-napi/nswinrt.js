// NSWinRT JS helper layer (ships with the package alongside the native addon).
// Pure JS that operates on the native WinRT proxies — async→Promise, event helpers.
// Mirrors the runtime's HELPER_SOURCE `toPromise`, adapted for the napi backend where the
// host owns the loop (the consumer pumps via `pumpMessages`).
'use strict';
const native = require('./index.js');

const Windows = native.getNamespace('Windows');

function statusEnum() {
  try { return Windows.Foundation.AsyncStatus; }
  catch { return { Started: 0, Completed: 1, Canceled: 2, Error: 3 }; }
}

function normalizeStatus(status) {
  if (status == null) return NaN;
  if (typeof status === 'number') return status;
  const n = Number(status);
  return Number.isNaN(n) ? NaN : n;
}

// Pump lifecycle: while any WinRT async op is outstanding, the pump timer is ref'd so Node
// stays alive to deliver Completed; when the last settles, it is unref'd so the process can
// exit. (An unref'd-only timer would let Node exit before completion fires.)
let pending = 0;
function retainPump() {
  pending += 1;
  if (autoPumpTimer && typeof autoPumpTimer.ref === 'function') autoPumpTimer.ref();
}
function releasePump() {
  pending = Math.max(0, pending - 1);
  if (pending === 0 && autoPumpTimer && typeof autoPumpTimer.unref === 'function') {
    autoPumpTimer.unref();
  }
}

// Convert a WinRT IAsyncOperation/IAsyncAction proxy to a JS Promise via its Completed event,
// Status property, and GetResults() method — all provided by the native proxy layer.
function toPromise(op) {
  if (op == null || (typeof op !== 'object' && typeof op !== 'function')) {
    return Promise.resolve(op);
  }
  if (typeof op.then === 'function' && !('Completed' in op)) return op;

  const S = statusEnum();
  retainPump();
  return new Promise((resolve, reject) => {
    let settled = false;
    const done = (fn, arg) => { settled = true; releasePump(); fn(arg); };
    const settle = (override) => {
      if (settled) return;
      try {
        const status = normalizeStatus(override !== undefined ? override : op.Status);
        if (status === S.Completed || status === 1) {
          done(resolve, typeof op.GetResults === 'function' ? op.GetResults() : undefined);
        } else if (status === S.Canceled || status === 2) {
          done(reject, new Error('WinRT async operation was canceled'));
        } else if (status === S.Error || status === 3) {
          done(reject, op.ErrorCode || new Error('WinRT async operation failed'));
        }
      } catch (err) { done(reject, err); }
    };

    const initial = normalizeStatus(op.Status);
    if (!Number.isNaN(initial) && initial !== 0) { settle(initial); return; }
    op.Completed = (asyncInfo, asyncStatus) => settle(asyncStatus);
    // Race guard: it may have completed between the status read and handler assignment.
    const race = normalizeStatus(op.Status);
    if (!Number.isNaN(race) && race !== 0) settle(race);
  });
}

// Drive the Windows message loop until `promise` settles or `timeoutMs` elapses. WinRT STA
// async completions are delivered as messages, so the loop must be pumped for them to fire.
async function awaitWithPump(promise, timeoutMs = 5000) {
  let done = false, result, error, ok;
  promise.then((r) => { done = true; ok = true; result = r; },
              (e) => { done = true; ok = false; error = e; });
  const start = Date.now();
  while (!done && Date.now() - start < timeoutMs) {
    native.pumpMessages();
    await new Promise((r) => setImmediate(r));
  }
  if (!done) throw new Error('awaitWithPump timed out');
  if (!ok) throw error;
  return result;
}

// Pump the WinRT message loop automatically on a Node timer so awaiting WinRT Promises is
// transparent (no manual `awaitWithPump`). The timer is unref'd, so it never keeps the process
// alive on its own. Returns a stop function. Safe to call once at startup.
let autoPumpTimer = null;
function enableAutoPump(intervalMs = 4) {
  if (autoPumpTimer) return () => disableAutoPump();
  autoPumpTimer = setInterval(() => { native.pumpMessages(); }, intervalMs);
  if (typeof autoPumpTimer.unref === 'function') autoPumpTimer.unref();
  return () => disableAutoPump();
}
function disableAutoPump() {
  if (autoPumpTimer) { clearInterval(autoPumpTimer); autoPumpTimer = null; }
}

// interop.* — the full NSWinRT.interop surface (Pointer/OutParam, reference/typed-value
// boxing, buffer + DateTime utilities, uuid/winmd registration, zero-copy
// arrayBufferFromBuffer). Installed by the shared native layer so the surface is identical
// across Node/Bun/Deno and the standalone engine hosts.
native.installInterop();
const interop = globalThis.NSWinRT.interop;

// dotnet.* — the .NET/BCL bridge (NSWinRT.dotnet: invoke/get/fromHandle/registerNamespace,
// taskToPromise/asDelegate, plus NSWinRT.runOnUIThread). A no-op at the JS layer until a
// dotnet-bridge/publish/DotNetBridge.dll exists next to the app.
native.installDotnet();
const dotnet = globalThis.NSWinRT.dotnet;

module.exports = {
  native,
  Windows,
  toPromise,
  awaitWithPump,
  enableAutoPump,
  disableAutoPump,
  interop,
  dotnet,
};
