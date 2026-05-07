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
                    const uri = new Windows.Foundation.Uri('http://example.com/');
                    for (let i = 0; i < 2000; i++) {
                        void uri.AbsoluteUri;
                    }
                "#
            },
            |script| runtime.run_script(script),
            BatchSize::SmallInput,
        );
    });

    group.finish();
}

criterion_group!(benches, bench_script_eval);
criterion_main!(benches);
