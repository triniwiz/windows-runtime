use criterion::{criterion_group, criterion_main, BatchSize, Criterion};
use runtime::Runtime;

fn bench_async_paths(c: &mut Criterion) {
    let mut group = c.benchmark_group("runtime_async");

    group.bench_function("promise_microtasks", |b| {
        let mut runtime = Runtime::new(".");

        b.iter_batched(
            || {
                r#"
                    let done = 0;
                    for (let i = 0; i < 1500; i++) {
                        Promise.resolve(i).then(() => { done++; });
                    }
                "#
            },
            |script| runtime.run_script(script, "<bench>"),
            BatchSize::SmallInput,
        );
    });

    group.bench_function("promise_chain_depth", |b| {
        let mut runtime = Runtime::new(".");

        b.iter_batched(
            || {
                r#"
                    (function() {
                        let p = Promise.resolve(0);
                        for (let i = 0; i < 1000; i++) {
                            p.then(v => v + 1);
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

criterion_group!(benches, bench_async_paths);
criterion_main!(benches);
