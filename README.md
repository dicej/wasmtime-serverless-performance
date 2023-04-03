# wasmtime-serverless-performance

This is a small suite of benchmarks designed to test Wasmtime peformance for
serverless function workloads in various configurations.

## Building and running

First, make sure you have [Rust](https://rustup.rs/) nightly 1.67 or later installed,
including the `wasm32-wasi` and `wasm32-unknown-unknown` targets:

```shell
rustup install nightly
rustup target add --toolchain=nightly wasm32-wasi wasm32-unknown-unknown
```

Then, run the benchmarks:

```shell
cargo +nightly bench
```
