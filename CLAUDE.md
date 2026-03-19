# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Build Commands

```bash
# Desktop (Windows/macOS/Linux) - debug
cargo run -p fractal-app

# Desktop - release (optimized, uses LTO + codegen-units=1)
cargo run -p fractal-app --release

# Check all crates without building
cargo check --workspace

# Run tests
cargo test --workspace

# Run tests for a single crate
cargo test -p fractal-core

# Web (WebGPU/WASM) - requires trunk
cargo install trunk
cd crates/fractal-app
trunk serve --release --port 8080
# Open http://localhost:8080

# Android - requires cargo-ndk and Android NDK
cargo install cargo-ndk
cargo ndk -t arm64-v8a -o android/app/src/main/jniLibs build -p fractal-app --release
# Then build APK: cd android && ./gradlew assembleDebug
```

CI enforces `-D warnings` (RUSTFLAGS), so the build fails on any warnings.

## Architecture

The project is a 4-crate Rust workspace. Data flows: `fractal-core` (math/types) → `fractal-renderer` (GPU) + `fractal-ui` (egui panels) → `fractal-app` (application orchestration).

### Crates

- **`fractal-core`** — Platform-agnostic math: camera, SDF primitives, and all 6 fractal definitions (`FractalType` enum + `FractalParams` struct). No GPU dependencies.
- **`fractal-renderer`** — wgpu context (`context.rs`), render pipeline (`pipeline.rs`), GPU uniform buffers (`uniforms.rs`), and WGSL shaders (`shaders/`).
- **`fractal-ui`** — egui immediate-mode UI panels for fractal params, camera, and color settings. `state.rs` manages `UiState`.
- **`fractal-app`** — Ties everything together. `app.rs` is the main application loop. `main.rs` is the desktop/WASM entry; `lib.rs` exposes the Android entry point (`android_main`).

### Platform Differences

`fractal-app` uses `#[cfg(...)]` extensively:

| Concern | Native | WASM | Android |
|---|---|---|---|
| Entry point | `main()` in `main.rs` | `wasm_bindgen(start)` in `main.rs` | `android_main` in `lib.rs` |
| Async runtime | `pollster::block_on` | `wasm_bindgen_futures::spawn_local` | `pollster::block_on` |
| Time | `std::time::Instant` | `web_time::Instant` | `std::time::Instant` |
| Logging | `env_logger` | `console_log` | `android_logger` |
| Shared state (WASM) | N/A | `Rc<RefCell<>>` | N/A |

### Rendering Pipeline

All rendering happens via a single fullscreen ray marching pass in WGSL (`crates/fractal-renderer/shaders/raymarcher.wgsl`, ~17 KB). Per pixel: generate ray → march ray (evaluate SDF in loop) → on hit: compute normal via finite differences → apply lighting/coloring → output RGB.

wgpu uses Naga to transpile WGSL to SPIR-V (Vulkan), MSL (Metal), HLSL (DirectX 12), or native WebGPU at runtime.

### Shader Uniforms

`FractalParams` in Rust maps 1:1 to a uniform buffer in WGSL. When adding new fractal parameters, update both `fractal-core/src/fractals/mod.rs` (the struct) and `raymarcher.wgsl` (the uniform binding and SDF implementation).

### Adding a New Fractal Type

1. Add variant to `FractalType` enum (`fractal-core/src/fractals/mod.rs`)
2. Implement SDF function in WGSL (`raymarcher.wgsl`)
3. Add UI controls in `fractal-ui/src/panels/fractal_params.rs`

## Key Technical Details

- **Deep zoom**: Uses double-single arithmetic (`hi: f32 + lo: f32`) in WGSL to emulate f64 precision (~14 decimal digits), enabling zoom to ~10^12. Implemented via Knuth's TwoSum and Veltkamp multiplication.
- **Android**: NativeActivity (no Java), loads `libfractal_app.so` (cdylib). Requires Vulkan level 1 hardware feature.
- **Web entry**: `crates/fractal-app/index.html` is the Trunk-managed HTML entry. Assets are copied via `data-trunk` directives.
- **Windows icon**: Embedded at build time via `crates/fractal-app/build.rs` using `winresource`.
