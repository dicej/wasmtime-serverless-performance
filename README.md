# wasmtime-serverless-performance

This is a small suite of benchmarks designed to test Wasmtime peformance for
serverless function workloads in various configurations.

In all cases, we measure the time it takes to:

* build and pass an HTTP request object to request handler function
* (in the request handler function) assert that the URI, headers, and body match expected values
* (on return from the function) assert that the returned HTTP response object contains the expected values

We benchmark the following variations:

* A native function call
    * This represents an idealized performance scenario for cases where no per-tenant or per-request isolation are necessary
* A native function call in a forked process
    * This represents a minimum viable per-request isolation scenario using OS-brokered sandboxing (e.g. containers)
* A minimal, fully sandboxed [Spin](https://github.com/fermyon/spin) app, written in Rust, with no precompilation, preinstantiation, or allocation pooling
    * This represents a sandboxing using Wasmtime without any type of optimization
* As above, but with precompilation
    * This represents Wasmtime-based sandboxing with precompilation to native code
* As above, but with preinstantiation
    * Preinstantiation provides additional performance benefits beyond precompilation, leaving as little as possible to do prior to actual instantiation
* As above, but with allocation pooling
    * Allocation pooling further reduces the work needed to instantiate a Wasm component by enabling safe reuse of memory allocations across instances

We also include a couple of extra cases for comparing alternative guest execution models:

* A [WAGI](https://deislabs.io/posts/introducing-wagi-easiest-way-to-build-webassembly-microservices/) app, written in Rust
    * This uses [WASI](https://wasi.dev/) standard I/O streams to serialize request and response data, and is also based on core Wasm modules instead of components
* A Spin app based on the official Spin Rust SDK
    * This is useful for measuring the overhead of the SDK vs. directly using [wit-bindgen](https://github.com/bytecodealliance/wit-bindgen)-generated bindings
* A Spin app written in Python instead of Rust
    * This is based on the experimental [Spin Python SDK](https://github.com/fermyon/spin-python-sdk), which uses [Wizer](https://github.com/bytecodealliance/wizer) to pre-initialize the Python interpreter and thereby minimize latency

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
