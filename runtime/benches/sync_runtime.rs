use criterion::{black_box, criterion_group, criterion_main, BatchSize, Criterion};
use runtime::Runtime;

fn bench_script_eval(c: &mut Criterion) {
    let mut group = c.benchmark_group("runtime_sync");

    group.bench_function("run_script_tiny", |b| {
        let mut runtime = Runtime::new(".");
        b.iter(|| runtime.run_script(black_box("1 + 1"), "<bench>"));
    });

    group.bench_function("namespace_lookup_hot", |b| {
        let mut runtime = Runtime::new(".");
        runtime.run_script("Windows.Foundation", "<warmup>");

        b.iter_batched(
            || "for (let i = 0; i < 2000; i++) { void Windows.Foundation.Uri; }",
            |script| runtime.run_script(script, "<bench>"),
            BatchSize::SmallInput,
        );
    });

    group.bench_function("method_wrapper_lookup_hot", |b| {
        let mut runtime = Runtime::new(".");
        runtime.run_script(
            r#"
            const uri = new Windows.Foundation.Uri('http://example.com/');
            void uri.AbsoluteUri;
        "#,
            "<warmup>",
        );

        b.iter_batched(
            || {
                r#"
                (function() {
                    const uri = new Windows.Foundation.Uri('http://example.com/');
                    for (let i = 0; i < 2000; i++) { void uri.AbsoluteUri; }
                })();
            "#
            },
            |script| runtime.run_script(script, "<bench>"),
            BatchSize::SmallInput,
        );
    });

    group.finish();
}

fn bench_marshalling(c: &mut Criterion) {
    let mut group = c.benchmark_group("marshalling");
    group.sample_size(10);

    group.bench_function("primitives_1M", |b| {
        let mut runtime = Runtime::new(".");
        runtime.run_script("globalThis.__benchUri = new Windows.Foundation.Uri('http://example.com:8080/'); void globalThis.__benchUri.Port;", "<warmup>");
        b.iter_batched(
            || r#"(function(){ for (var i = 0; i < 1000000; i++) { void globalThis.__benchUri.Port; } })();"#,
            |script| runtime.run_script(script, "<bench>"),
            BatchSize::SmallInput,
        );
    });

    group.bench_function("strings_100K", |b| {
        let mut runtime = Runtime::new(".");
        runtime.run_script("Windows.Data.Json.JsonValue.CreateStringValue('warm').GetString()", "<warmup>");
        b.iter_batched(
            || r#"
                (function() {
                    var strings = [];
                    for (var i = 0; i < 100; i++) { strings.push("abcdefghijklmnopqrstuvwxyz" + i); }
                    for (var i = 0; i < 100000; i++) {
                        Windows.Data.Json.JsonValue.CreateStringValue(strings[i % strings.length]).GetString();
                    }
                })();
            "#,
            |script| runtime.run_script(script, "<bench>"),
            BatchSize::SmallInput,
        );
    });

    group.bench_function("bigdata_200x64K", |b| {
        let mut runtime = Runtime::new(".");
        runtime.run_script(r#"
            var __bigBuf = new Uint8Array(65536);
            for (var i = 0; i < 65536; i++) { __bigBuf[i] = i & 0xFF; }
        "#, "<warmup>");
        b.iter_batched(
            || r#"
                (function() {
                    for (var i = 0; i < 200; i++) {
                        Windows.Security.Cryptography.CryptographicBuffer.CreateFromByteArray(__bigBuf);
                    }
                })();
            "#,
            |script| runtime.run_script(script, "<bench>"),
            BatchSize::SmallInput,
        );
    });

    group.finish();
}

/// Win32 FFI dispatch throughput via libffi.
fn bench_win32_ffi(c: &mut Criterion) {
    let mut group = c.benchmark_group("win32_ffi");
    group.sample_size(20);

    // GetTickCount64 — no args, u64 return.  Measures raw libffi + DLL dispatch.
    group.bench_function("get_tick_count64_100K", |b| {
        let mut runtime = Runtime::new(".");
        runtime.run_script(
            "NSWinRT.win32.call('kernel32.dll','GetTickCount64','u64');",
            "<warmup>",
        );
        b.iter_batched(
            || {
                r#"
                (function(){
                    for (var i = 0; i < 100000; i++)
                        NSWinRT.win32.call('kernel32.dll','GetTickCount64','u64');
                })();
            "#
            },
            |script| runtime.run_script(script, "<bench>"),
            BatchSize::SmallInput,
        );
    });

    // bind() Proxy overhead vs direct call.
    group.bench_function("bind_proxy_100K", |b| {
        let mut runtime = Runtime::new(".");
        runtime.run_script(
            "var _k32 = NSWinRT.win32.bind('kernel32.dll','u64'); _k32.GetTickCount64();",
            "<warmup>",
        );
        b.iter_batched(
            || {
                r#"
                (function(){
                    for (var i = 0; i < 100000; i++) _k32.GetTickCount64();
                })();
            "#
            },
            |script| runtime.run_script(script, "<bench>"),
            BatchSize::SmallInput,
        );
    });

    // import() makes DLL exports plain globals: GetTickCount64() with no prefix.
    group.bench_function("import_global_100K", |b| {
        let mut runtime = Runtime::new(".");
        runtime.run_script(
            "NSWinRT.win32.import('kernel32.dll','u64'); GetTickCount64();",
            "<warmup>",
        );
        b.iter_batched(
            || {
                r#"
                (function(){
                    for (var i = 0; i < 100000; i++) GetTickCount64();
                })();
            "#
            },
            |script| runtime.run_script(script, "<bench>"),
            BatchSize::SmallInput,
        );
    });

    group.finish();
}

/// URL parsing throughput (WHATWG url crate).
fn bench_url_parse(c: &mut Criterion) {
    let mut group = c.benchmark_group("url_parse");
    group.sample_size(20);

    group.bench_function("parse_100K", |b| {
        let mut runtime = Runtime::new(".");
        runtime.run_script("new URL('https://example.com:8080/path?q=1#h');", "<warmup>");
        b.iter_batched(
            || r#"
                (function(){
                    for (var i = 0; i < 100000; i++)
                        new URL('https://user:pw@example.com:8080/path/to/resource?key=value&other=123#anchor');
                })();
            "#,
            |script| runtime.run_script(script, "<bench>"),
            BatchSize::SmallInput,
        );
    });

    group.finish();
}

/// .NET BCL invoke throughput (skipped if DotNetBridge is not published).
fn bench_dotnet(c: &mut Criterion) {
    let bridge = std::path::PathBuf::from(".")
        .join("dotnet-bridge")
        .join("publish")
        .join("DotNetBridge.dll");
    if !bridge.exists() {
        eprintln!("[bench_dotnet] SKIP: dotnet-bridge not published");
        return;
    }

    let mut group = c.benchmark_group("dotnet");
    group.sample_size(10);

    // Static call: Stopwatch.GetTimestamp() — no args, returns i64.
    group.bench_function("stopwatch_get_timestamp_1K", |b| {
        let mut runtime = Runtime::new(".");
        runtime.run_script("System.Diagnostics.Stopwatch.GetTimestamp();", "<warmup>");
        b.iter_batched(
            || {
                r#"
                (function(){
                    for (var i = 0; i < 1000; i++)
                        System.Diagnostics.Stopwatch.GetTimestamp();
                })();
            "#
            },
            |script| runtime.run_script(script, "<bench>"),
            BatchSize::SmallInput,
        );
    });

    // Instance round-trip: create + stop + release using natural API.
    group.bench_function("stopwatch_roundtrip_100", |b| {
        let mut runtime = Runtime::new(".");
        runtime.run_script("System.Diagnostics.Stopwatch.StartNew();", "<warmup>");
        b.iter_batched(
            || {
                r#"
                (function(){
                    for (var i = 0; i < 100; i++) {
                        var sw = System.Diagnostics.Stopwatch.StartNew();
                        sw.Stop();
                        sw.release();
                    }
                })();
            "#
            },
            |script| runtime.run_script(script, "<bench>"),
            BatchSize::SmallInput,
        );
    });

    group.finish();
}

/// rAF throughput: how many frames can be pumped per second.
fn bench_raf(c: &mut Criterion) {
    let mut group = c.benchmark_group("raf");
    // On a live display __nsDwmFlush blocks one VSync (~8-16ms).
    // On headless it returns immediately.  Use a small iteration count.
    group.sample_size(10);

    group.bench_function("pump_10_frames", |b| {
        let mut runtime = Runtime::new(".");
        b.iter(|| {
            runtime.run_script(
                r#"
                var _frames = 0;
                function _frame(ts) {
                    _frames++;
                    if (_frames < 10) requestAnimationFrame(_frame);
                }
                requestAnimationFrame(_frame);
            "#,
                "<bench>",
            );
            // Drain microtasks (runs all 10 rAF iterations).
            runtime.run_script("", "<pump>");
        });
    });

    group.finish();
}

/// Class-member cache: cold (first lookup) vs hot (cached) resolution.
///
/// These benchmarks directly exercise the `CLASS_MEMBERS_CACHE` path touched
/// by every WinRT property/method access.  A significant gap between cold and
/// hot numbers indicates the cache is working correctly; the hot numbers set
/// the floor for per-access overhead.
fn bench_class_member_cache(c: &mut Criterion) {
    let mut group = c.benchmark_group("class_member_cache");
    group.sample_size(20);

    // Cold: new Runtime per iteration so the thread-local cache is empty.
    group.bench_function("cold_uri_members", |b| {
        b.iter(|| {
            let mut rt = Runtime::new(".");
            // First access after construction — populates CLASS_MEMBERS_CACHE.
            rt.run_script(
                "void Windows.Foundation.Uri.prototype.AbsoluteUri",
                "<bench>",
            );
        });
    });

    // Hot: Runtime created once; cache is warm from the second call onward.
    group.bench_function("hot_uri_property_2K", |b| {
        let mut rt = Runtime::new(".");
        rt.run_script(
            "globalThis.__u = new Windows.Foundation.Uri('http://example.com/');",
            "<warmup>",
        );
        b.iter_batched(
            || r#"(function(){ for (var i = 0; i < 2000; i++) void globalThis.__u.AbsoluteUri; })();"#,
            |script| rt.run_script(script, "<bench>"),
            criterion::BatchSize::SmallInput,
        );
    });

    // Hot: method dispatch (involves class-member cache + libffi round-trip).
    group.bench_function("hot_method_dispatch_1K", |b| {
        let mut rt = Runtime::new(".");
        rt.run_script(
            "Windows.Data.Json.JsonValue.CreateStringValue('warm').GetString();",
            "<warmup>",
        );
        b.iter_batched(
            || {
                r#"
                (function(){
                    for (var i = 0; i < 1000; i++)
                        Windows.Data.Json.JsonValue.CreateStringValue('x').GetString();
                })();
            "#
            },
            |script| rt.run_script(script, "<bench>"),
            criterion::BatchSize::SmallInput,
        );
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_script_eval,
    bench_marshalling,
    bench_win32_ffi,
    bench_url_parse,
    bench_dotnet,
    bench_raf,
    bench_class_member_cache,
);
criterion_main!(benches);
