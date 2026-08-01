//! The shared WinRT-interop microbenchmark workload (see `../bench.js`), embedded so the napi
//! standalone hosts can run the exact same script the classic runtime runs via `playground`.
//! A host evals [`WORKLOAD`] (when `NSWIN_BENCH` is set) after bring-up; it prints `BENCH\t…` lines.

pub const WORKLOAD: &str = include_str!("../bench.js");
