//! Runs a list of labelled JS stages against an engine-supplied `eval` closure and prints a
//! uniform report. Engine-agnostic: each package supplies how to evaluate a script.

use std::io::Write;

/// Run each `(label, code)` stage through `eval`, printing `[engine] label => result`.
/// Stops at the first failure. Returns `true` if every stage passed.
pub fn run_stages<F>(engine: &str, stages: &[(&str, &str)], mut eval: F) -> bool
where
    F: FnMut(&str) -> Result<String, String>,
{
    let mut failed = false;
    for (label, code) in stages {
        print!("[{engine}] {label:24} => ");
        std::io::stdout().flush().ok(); // flush before the call so a crash still shows the label
        match eval(code) {
            Ok(out) => println!("{out}"),
            Err(e) => {
                println!("THREW: {e}");
                failed = true;
                break;
            }
        }
        std::io::stdout().flush().ok();
    }
    if failed {
        println!("[{engine}] a stage threw (see above)");
    } else {
        println!("[{engine}] OK — WinRT running on standalone {engine}");
    }
    !failed
}

/// Self-checking stages for the napi-backend feature set beyond the per-engine basics: IMap
/// keyed sugar, PropertySet boxed-primitive round-trips, native subclassing (`class Sub
/// extends WinRTClass`) on both object models, composable class trees (Windows.UI.Composition),
/// and the composable-ctor path (env-aware: XAML activation is wrong-thread headless — the
/// stage passes when that surfaces as the clean, specific JS error, or when it succeeds in a
/// XAML-capable host). Each stage throws on mismatch so `run_stages` fails loudly.
pub const FEATURE_STAGES: &[(&str, &str)] = &[
    ("IMap keyed sugar", r#"(function () {
        var m = new Windows.Foundation.Collections.StringMap();
        m['a'] = '1';
        if (m['a'] !== '1' || !m.HasKey('a') || !('a' in m) || ('zz' in m) || m['zz'] !== undefined) throw new Error('IMap sugar mismatch');
        var v = m.GetView();
        if (v['a'] !== '1' || v.Size !== 1 || !v.HasKey('a')) throw new Error('IMapView mismatch');
        return 'ok';
    })()"#),
    ("PropertySet unboxing", r#"(function () {
        var ps = new Windows.Foundation.Collections.PropertySet();
        ps['n'] = 42; ps['s'] = 'hi'; ps['b'] = true;
        if (ps['n'] !== 42 || ps['s'] !== 'hi' || ps['b'] !== true || ps.Size !== 3) throw new Error('PropertySet round-trip mismatch');
        return 'ok';
    })()"#),
    ("subclass (host object)", r#"(function () {
        var JO = Windows.Data.Json.JsonObject;
        class Sub extends JO { constructor() { super(); this.tag = 7; } custom() { return 'c:' + this.Stringify(); } }
        var s = new Sub();
        if (!(s instanceof Sub) || !(s instanceof JO) || s.tag !== 7 || s.custom() !== 'c:{}') throw new Error('host subclass mismatch');
        var plain = new JO();
        if (plain instanceof Sub) throw new Error('plain instance polluted');
        return 'ok';
    })()"#),
    ("subclass (proxy path)", r#"(function () {
        var SM = Windows.Foundation.Collections.StringMap;
        class MSub extends SM { describe() { return 'n=' + this.Size; } }
        var t = new MSub();
        t['k'] = 'v';
        if (t.describe() !== 'n=1' || t['k'] !== 'v' || !(t instanceof MSub) || !(t instanceof SM)) throw new Error('proxy subclass mismatch');
        return 'ok';
    })()"#),
    ("composition class tree", r#"(function () {
        var c = new Windows.UI.Composition.Compositor();
        var v = c.CreateSpriteVisual();
        v.Opacity = 0.5;
        if (v.Opacity !== 0.5) throw new Error('Visual.Opacity mismatch');
        v.Brush = c.CreateColorBrush();
        var ch = c.CreateSpriteVisual();
        v.Children.InsertAtTop(ch);
        if (v.Children.Count !== 1) throw new Error('Children mismatch');
        return 'ok';
    })()"#),
    ("composable ctor (env-aware)", r#"(function () {
        try {
            var ff = new Windows.UI.Xaml.Media.FontFamily('Segoe UI');
            if (ff.Source !== 'Segoe UI') throw new Error('FontFamily constructed but Source wrong: ' + ff.Source);
            return 'constructed';
        } catch (e) {
            var msg = String(e && e.message ? e.message : e);
            if (/different thread|0x8001010E/i.test(msg)) return 'headless-ok (clean wrong-thread error)';
            throw e;
        }
    })()"#),
];

/// Kickoff for the async/event-loop demo stage: exercises timers (setTimeout, setInterval +
/// clearInterval), setImmediate (+ clearImmediate cancellation, immediate-after-microtask
/// ordering), queueMicrotask ordering, and a real WinRT async operation awaited through
/// `NSWinRT.toPromise` (ThreadPool.RunAsync — the Completed delegate arrives via the message
/// pump). Runs entirely under the standalone event loop; the verdict lands in
/// `globalThis.__nsAsyncDemo`.
pub const ASYNC_DEMO_KICKOFF: &str = r#"
globalThis.__nsAsyncDemo = 'pending';
(function () {
  'use strict';
  var order = [];
  queueMicrotask(function () { order.push('micro'); });
  var immediateArg = '';
  setImmediate(function (a, b) { order.push('immediate'); immediateArg = a + b; }, 'o', 'k');
  var cancelled = setImmediate(function () { order.push('cancelled'); });
  clearImmediate(cancelled);
  var ticks = 0;
  var iv = setInterval(function () { ticks++; if (ticks >= 3) { clearInterval(iv); } }, 5);
  new Promise(function (resolve) {
    setTimeout(function () { order.push('timeout'); resolve(); }, 20);
  }).then(function () {
    return NSWinRT.toPromise(Windows.System.Threading.ThreadPool.RunAsync(function () {}));
  }).then(function () {
    return new Promise(function (resolve) {
      (function waitTicks() { if (ticks >= 3) { resolve(); } else { setTimeout(waitTicks, 5); } })();
    });
  }).then(function () {
    globalThis.__nsAsyncDemo = 'micro=' + (order[0] === 'micro')
      + ',immediate=' + (order.indexOf('immediate') > 0 && order.indexOf('immediate') < order.indexOf('timeout') && immediateArg === 'ok')
      + ',cancelled=' + (order.indexOf('cancelled') < 0)
      + ',timeout=' + (order.indexOf('timeout') >= 0)
      + ',ticks=' + ticks + ',winrt=ok';
  }).catch(function (e) {
    globalThis.__nsAsyncDemo = 'ERROR: ' + (e && e.message ? e.message : e);
  });
})();
'started'
"#;

pub const ASYNC_DEMO_EXPECTED: &str =
    "micro=true,immediate=true,cancelled=true,timeout=true,ticks=3,winrt=ok";

/// Run the async/event-loop demo: evaluate the kickoff, drive the host's event loop until idle
/// (`run_loop`, expected to return `false` on a deadline), then check the recorded verdict.
pub fn run_async_demo<E, L>(engine: &str, mut eval: E, run_loop: L) -> bool
where
    E: FnMut(&str) -> Result<String, String>,
    L: FnOnce() -> bool,
{
    print!("[{engine}] {:24} => ", "async event loop");
    std::io::stdout().flush().ok();
    if let Err(e) = eval(ASYNC_DEMO_KICKOFF) {
        println!("KICKOFF THREW: {e}");
        return false;
    }
    if !run_loop() {
        println!("LOOP DEADLINE (work still pending)");
        return false;
    }
    match eval("globalThis.__nsAsyncDemo") {
        Ok(v) if v == ASYNC_DEMO_EXPECTED => {
            println!("{v}");
            true
        }
        Ok(v) => {
            println!("MISMATCH: {v} (expected {ASYNC_DEMO_EXPECTED})");
            false
        }
        Err(e) => {
            println!("VERDICT THREW: {e}");
            false
        }
    }
}
