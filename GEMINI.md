# Matchbox Project Overview

Matchbox provides painless peer-to-peer WebRTC networking for Rust's native and WASM applications, primarily aimed at low-latency multiplayer games.

## Project Structure

This is a Rust workspace containing several crates:

- **`matchbox_socket`**: The core client-side socket abstraction for WASM and Native.
- **`matchbox_signaling`**: A library for building custom signaling servers.
- **`matchbox_server`**: A ready-to-use full-mesh signaling server binary.
- **`bevy_matchbox`**: Integration for the Bevy game engine.
- **`matchbox_protocol`**: Common protocol definitions shared between clients and servers.
- **`examples/`**: Various usage examples, including `simple`, `bevy_ggrs`, and `async_example`.

## Key Technologies

- **Rust**: Primary programming language.
- **WebRTC**: Used for p2p data channels (via `webrtc` crate on native and `web-sys` on WASM).
- **Tokio / Future-based**: Heavily utilizes async/await for message loops.
- **WASM**: First-class support for browser-based games.

## Building and Running

### Common Commands

- **Test (Native)**: `cargo test --features signaling --all-targets`
- **Doc Tests**: `cargo test --doc`
- **Run Signaling Server**: `cargo run -p matchbox_server`
- **Run Simple Example**: `cargo run -p simple_example`
- **Lint (Clippy)**: `cargo clippy --features signaling --all-targets -- -D warnings`
- **Format Check**: `cargo fmt --all --check`

### WASM Development

For WASM-specific builds or checks, use the following flags:

```bash
RUSTFLAGS='--cfg=web_sys_unstable_apis --cfg=getrandom_backend="wasm_js"' cargo check \
    --all-targets \
    --target wasm32-unknown-unknown \
    -p matchbox_socket \
    -p bevy_matchbox \
    -p bevy_ggrs_example \
    -p simple_example
```

## Development Conventions

- **Formatting**: Strict adherence to `rustfmt.toml` (e.g., `imports_granularity = "Crate"`, `max_width = 100`).
- **Licensing**: All contributions must be dual-licensed under MIT and Apache-2.0.
- **Async**: Use `n0-future` or standard `futures` traits for compatibility across native and WASM.
- **Testing**: New features should include unit tests and, if applicable, be verified in the relevant examples.
- **Documentation**: All public APIs should be documented (use `cargo test --doc` to verify).
