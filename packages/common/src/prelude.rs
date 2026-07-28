//! JS runtime prelude for the standalone hosts, evaluated after `install_globals` and the URL
//! polyfill. Provides the pieces a bare engine lacks that are pure JS over the napi natives:
//!   - `queueMicrotask` (over the engine's own promise queue) when the engine doesn't expose it,
//!   - `NSWinRT.toPromise(op)` — converts a WinRT IAsyncOperation/IAsyncAction to a Promise
//!     (same idea as nswinrt.js's `toPromise` for the Node package). It holds the event loop
//!     open via the `__nsLoopRetain`/`__nsLoopRelease` natives while an operation is
//!     outstanding, so `await NSWinRT.toPromise(op)` "just works" under `run_event_loop`.
//!
//! Kept engine-conservative (function expressions, no async/await syntax) so one source runs
//! unchanged on QuickJS, Hermes, V8, and JSC.

pub const PRELUDE: &str = r#"
(function (g) {
  'use strict';

  // Node-style `global` alias for `globalThis` — @nativescript/core and webpack `target: 'node'`
  // output both reference bare `global` (e.g. `global.foo = ...`). Classic rusty_v8 sets this via
  // `init_global` (a read-only own property); defined the same way here so it runs on every napi
  // engine.
  if (typeof g.global === 'undefined') {
    Object.defineProperty(g, 'global', { value: g, writable: false, configurable: true });
  }

  if (typeof g.queueMicrotask !== 'function') {
    g.queueMicrotask = function (cb) {
      if (typeof cb !== 'function') { throw new TypeError('queueMicrotask expects a function'); }
      Promise.resolve().then(cb);
    };
  }

  var retain = typeof g.__nsLoopRetain === 'function' ? g.__nsLoopRetain : function () {};
  var release = typeof g.__nsLoopRelease === 'function' ? g.__nsLoopRelease : function () {};

  function statusEnum() {
    try { return g.Windows.Foundation.AsyncStatus; }
    catch (e) { return { Started: 0, Completed: 1, Canceled: 2, Error: 3 }; }
  }

  function normalizeStatus(status) {
    if (status == null) { return NaN; }
    if (typeof status === 'number') { return status; }
    var n = Number(status);
    return isNaN(n) ? NaN : n;
  }

  // Convert a WinRT IAsyncOperation/IAsyncAction proxy to a JS Promise via its Completed event,
  // Status property, and GetResults() method. Mirrors nswinrt.js (Node package); the pump there
  // is a ref-counted Node timer, here it is the standalone event loop's keep-alive counter.
  function toPromise(op) {
    if (op == null || (typeof op !== 'object' && typeof op !== 'function')) {
      return Promise.resolve(op);
    }
    if (typeof op.then === 'function' && !('Completed' in op)) { return op; }

    var S = statusEnum();
    retain();
    return new Promise(function (resolve, reject) {
      var settled = false;
      function done(fn, arg) { settled = true; release(); fn(arg); }
      function settle(override) {
        if (settled) { return; }
        try {
          var status = normalizeStatus(override !== undefined ? override : op.Status);
          if (status === S.Completed || status === 1) {
            done(resolve, typeof op.GetResults === 'function' ? op.GetResults() : undefined);
          } else if (status === S.Canceled || status === 2) {
            done(reject, new Error('WinRT async operation was canceled'));
          } else if (status === S.Error || status === 3) {
            done(reject, op.ErrorCode || new Error('WinRT async operation failed'));
          }
        } catch (err) { done(reject, err); }
      }

      var initial = normalizeStatus(op.Status);
      if (!isNaN(initial) && initial !== 0) { settle(initial); return; }
      op.Completed = function (asyncInfo, asyncStatus) { settle(asyncStatus); };
      // Race guard: it may have completed between the status read and handler assignment.
      var race = normalizeStatus(op.Status);
      if (!isNaN(race) && race !== 0) { settle(race); }
    });
  }

  g.NSWinRT = g.NSWinRT || {};
  g.NSWinRT.toPromise = toPromise;
})(globalThis);

// CommonJS shim: webpack `target: 'node'` bundles (what NativeScript apps are built as) expect
// `require`/`module`/`exports`/`__dirname`/`__filename` as globals — normally supplied by Node's
// own module wrapper. This runtime is not Node, so we supply them here the same way the classic
// rusty_v8 runtime does (`global_fns::HELPER_SOURCE`), backed by the `__nsResolveModulePath` /
// `__nsReadTextFile` / `__nsAppRoot` natives `host_abi::initialize_runtime` installs. Without this, every
// chunk (runtime.js/vendor.js/the app bundle) throws `ReferenceError: require is not defined` on
// evaluation.
(function (g) {
  'use strict';

  if (typeof g.require === 'function' && typeof g.module !== 'undefined') {
    return;
  }
  if (typeof g.__nsResolveModulePath !== 'function' || typeof g.__nsReadTextFile !== 'function') {
    return;
  }

  var cjsCache = new Map();

  function resolveSpecifier(specifier, callerFile) {
    if (typeof specifier !== 'string' || specifier.length === 0) {
      throw new Error('Cannot find module: ' + String(specifier));
    }
    var appRoot = (g.__nsAppRoot || '').replace(/[\\\/]+$/, '');

    // NativeScript tilde alias: ~/foo -> {appRoot}/app/foo
    if (specifier.charAt(0) === '~' && specifier.charAt(1) === '/') {
      var abs = appRoot + '\\app\\' + specifier.substring(2).replace(/\//g, '\\');
      return g.__nsResolveModulePath(abs, '', appRoot) || abs;
    }

    // Relative (./foo, ../foo) or bare name: use native resolver with caller context.
    // Fall back to app/bundle.js as parent so top-level require('./chunk.js') works.
    var parent = callerFile || (appRoot + '\\app\\bundle.js');
    return g.__nsResolveModulePath(specifier, parent, appRoot);
  }

  function makeRequire(callerFile) {
    return function require(specifier) {
      var resolved = resolveSpecifier(specifier, callerFile);
      if (!resolved) { throw new Error('Cannot find module: ' + specifier); }

      var key = resolved.replace(/\\/g, '/').toLowerCase();
      if (cjsCache.has(key)) { return cjsCache.get(key).exports; }

      var mod = { id: resolved, filename: resolved, exports: {} };
      cjsCache.set(key, mod); // set before eval to break circular deps

      var isJson = key.slice(-5) === '.json';
      var content = g.__nsReadTextFile(resolved);

      if (isJson) {
        try { mod.exports = JSON.parse(content || '{}'); } catch (_e) { mod.exports = {}; }
        return mod.exports;
      }

      var dirName = resolved.replace(/\//g, '\\').replace(/\\[^\\]*$/, '');
      var childRequire = makeRequire(resolved);

      try {
        var factory = new Function('module', 'exports', 'require', '__filename', '__dirname', content);
        factory(mod, mod.exports, childRequire, resolved, dirName);
      } catch (e) {
        cjsCache.delete(key);
        throw e;
      }
      cjsCache.set(key, mod);
      return mod.exports;
    };
  }

  g.require = makeRequire(null);

  // Top-level CJS globals for scripts executed outside a factory wrapper (e.g. when the host
  // calls runtime_runscript directly with a CJS file — runtime.js/vendor.js/the app bundle).
  if (typeof g.module === 'undefined') {
    var _topMod = { id: '<main>', exports: {} };
    Object.defineProperty(g, 'module',  { value: _topMod, writable: true, configurable: true });
    Object.defineProperty(g, 'exports', { value: _topMod.exports, writable: true, configurable: true });
  }

  // Provide __dirname / __filename globals for webpack target:'node' bundles. webpack leaves
  // these undefined when building for node (expects Node.js to provide them via its module
  // wrapper); this runtime supplies the app directory as a reasonable fallback value.
  if (typeof g.__dirname === 'undefined') {
    var _appRoot2 = (g.__nsAppRoot || '').replace(/[\\\/]+$/, '');
    var _appDir = _appRoot2 + '\\app';
    Object.defineProperty(g, '__dirname',  { value: _appDir, writable: true, configurable: true });
    Object.defineProperty(g, '__filename', { value: _appDir + '\\bundle.js', writable: true, configurable: true });
  }
})(globalThis);
'prelude-ok'
"#;
