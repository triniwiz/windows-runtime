# Runtime Performance Benchmarks

This crate includes Criterion benchmarks for runtime lookup and invocation hot paths.

## Bench suites

- sync_runtime
  - run_script_tiny
  - namespace_lookup_hot
  - method_wrapper_lookup_hot

- async_runtime
  - promise_microtasks
  - promise_chain_depth

## Run commands

From workspace root:

- cargo bench -p runtime --bench sync_runtime
- cargo bench -p runtime --bench async_runtime

If cargo is not on PATH in PowerShell:

- $env:PATH += ";$env:USERPROFILE\\.cargo\\bin"
- cargo bench -p runtime --bench sync_runtime
- cargo bench -p runtime --bench async_runtime

## Notes

- namespace_lookup_hot and method_wrapper_lookup_hot are designed to validate wrapper cache efficiency in the named property getter.
- promise benchmarks focus on microtask throughput and scheduling overhead.
- For WinRT async object benchmarks, add scenarios using concrete IAsyncOperation-returning APIs that are available in your test environment.
