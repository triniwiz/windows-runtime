use criterion::{black_box, criterion_group, criterion_main, BatchSize, Criterion};
use runtime::Runtime;

fn bench_script_eval(c: &mut Criterion) {
    let mut group = c.benchmark_group("runtime_sync");

    group.bench_function("run_script_tiny", |b| {
        let mut runtime = Runtime::new(".");
        b.iter(|| runtime.run_script(black_box("1 + 1")));
    });

    group.bench_function("namespace_lookup_hot", |b| {
        let mut runtime = Runtime::new(".");

        // Warm up namespace objects so steady-state hits the getter cache.
        runtime.run_script("Windows.Foundation");

        b.iter_batched(
            || {
                "for (let i = 0; i < 2000; i++) { void Windows.Foundation.Uri; }"
            },
            |script| runtime.run_script(script),
            BatchSize::SmallInput,
        );
    });

    group.bench_function("method_wrapper_lookup_hot", |b| {
        let mut runtime = Runtime::new(".");

        // Uri.AbsoluteUri accesses class and method/property wrappers repeatedly.
        let setup = r#"
            const uri = new Windows.Foundation.Uri('http://example.com/');
            void uri.AbsoluteUri;
        "#;
        runtime.run_script(setup);

        b.iter_batched(
            || {
                r#"
                    (function() {
                        const uri = new Windows.Foundation.Uri('http://example.com/');
                        for (let i = 0; i < 2000; i++) {
                            void uri.AbsoluteUri;
                        }
                    })();
                "#
            },
            |script| runtime.run_script(script),
            BatchSize::SmallInput,
        );
    });

    group.finish();
}

/// Port of the NativeScript iOS marshalling benchmarks from
/// https://blog.nativescript.org/perf-metrics-universal-javascript-part1/
/// to the Windows WinRT runtime.
///
/// Primitives  – 1 000 000 iterations of a small numeric WinRT call.
/// Strings     – 100 000 iterations of HSTRING in/HSTRING out.
/// Big data    – 200 iterations of passing a 65 536-byte Uint8Array into WinRT.
fn bench_marshalling(c: &mut Criterion) {
    let mut group = c.benchmark_group("marshalling");
    // Give Criterion more samples so the timings are stable.
    group.sample_size(10);

    // ── Primitives ─────────────────────────────────────────────────────────
    // Windows primitive marshalling benchmark: repeatedly read an Int32-valued
    // WinRT property from a pre-created Uri instance.
    group.bench_function("primitives_1M", |b| {
        let mut runtime = Runtime::new(".");
        // warm-up
        runtime.run_script("globalThis.__benchUri = new Windows.Foundation.Uri('http://example.com:8080/'); void globalThis.__benchUri.Port;");
        b.iter_batched(
            || r#"
                (function() {
                    for (var i = 0; i < 1000000; i++) {
                        void globalThis.__benchUri.Port;
                    }
                })();
            "#,
            |script| runtime.run_script(script),
            BatchSize::SmallInput,
        );
    });

    // ── Strings ────────────────────────────────────────────────────────────
    // Windows equivalent of NSString passthrough: HSTRING in, HSTRING out.
    // JsonValue.CreateStringValue(str).GetString() is a pure string round-trip
    // with minimal extra work beyond the boundary crossing.
    group.bench_function("strings_100K", |b| {
        let mut runtime = Runtime::new(".");
        // warm-up
        runtime.run_script("Windows.Data.Json.JsonValue.CreateStringValue('warm').GetString()");
        b.iter_batched(
            || r#"
                (function() {
                    var strings = [];
                    for (var i = 0; i < 100; i++) {
                        strings.push("abcdefghijklmnopqrstuvwxyz" + i);
                    }
                    for (var i = 0; i < 100000; i++) {
                        Windows.Data.Json.JsonValue
                            .CreateStringValue(strings[i % strings.length])
                            .GetString();
                    }
                })();
            "#,
            |script| runtime.run_script(script),
            BatchSize::SmallInput,
        );
    });

    // ── Big data ───────────────────────────────────────────────────────────
    // Windows equivalent of the NSArray<byte> -> UIImage path: push a large
    // JS TypedArray across the boundary and materialise an IBuffer from it.
    // This focuses on the copy/marshal cost of 64 KiB of byte data.
    group.bench_function("bigdata_200x64K", |b| {
        let mut runtime = Runtime::new(".");
        // Build the TypedArray once outside the timed loop.
        runtime.run_script(r#"
            var __bigBuf = new Uint8Array(65536);
            for (var i = 0; i < 65536; i++) { __bigBuf[i] = i & 0xFF; }
        "#);
        b.iter_batched(
            || r#"
                (function() {
                    for (var i = 0; i < 200; i++) {
                        Windows.Security.Cryptography.CryptographicBuffer
                            .CreateFromByteArray(__bigBuf);
                    }
                })();
            "#,
            |script| runtime.run_script(script),
            BatchSize::SmallInput,
        );
    });

    group.finish();
}

criterion_group!(benches, bench_script_eval, bench_marshalling);
criterion_main!(benches);
