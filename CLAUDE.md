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

# Cross-check Android compilation (requires: rustup target add aarch64-linux-android)
cargo check -p fractal-app --target aarch64-linux-android

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
CI uses `cargo nextest` with the `ci` profile (`.config/nextest.toml`) on all PC platforms
(Windows, macOS, Linux x64, Linux ARM64) and checks Android compilation via
`.github/workflows/test.yml`. The release workflow (`release.yml`) also runs the full test
suite before creating a release. Snapshot tests are excluded from CI (they require a GPU).
When running local checks, do NOT set RUSTFLAGS manually — just use `cargo check --workspace`.

Before pushing, verify cross-compilation for at least Windows (default) and Android:
```bash
cargo check --workspace
cargo check --workspace --features hot-reload
cargo check -p fractal-app --target aarch64-linux-android
```

### Hot-Reload (Shader Development)

```bash
# Run with shader hot-reload enabled (dev only)
cargo run -p fractal-app --features hot-reload
```

When enabled, the app polls `crates/fractal-renderer/shaders/raymarcher.wgsl` for changes every 500ms. Edit the shader, save, and see results live. Compile errors are logged and the old shader continues rendering. Also hot-reloads `settings.toml` config changes.

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
| Session storage | `dirs::data_dir()/ModernFractalViewer/saves/` | `localStorage` | `dirs::data_dir()` (may be `None`) |

### Rendering Pipeline

All rendering happens via a single fullscreen ray marching pass in WGSL (`crates/fractal-renderer/shaders/raymarcher.wgsl`, ~17 KB). Per pixel: generate ray → march ray (evaluate SDF in loop) → on hit: compute normal via finite differences → apply lighting/coloring → output RGB.

wgpu uses Naga to transpile WGSL to SPIR-V (Vulkan), MSL (Metal), HLSL (DirectX 12), or native WebGPU at runtime.

### Shader Uniforms

`FractalParams` in Rust maps 1:1 to a uniform buffer in WGSL. When adding new fractal parameters, update both `fractal-core/src/fractals/mod.rs` (the struct) and `raymarcher.wgsl` (the uniform binding and SDF implementation).

**No `vec3` in uniform structs**: WGSL requires 16-byte alignment for `vec4` and 8-byte for `vec2`, but `vec3` also occupies 16 bytes. The Rust `Uniforms` struct in `uniforms.rs` uses individual `f32` fields with explicit `_padN` fields instead of `[f32; 3]` to match WGSL layout. Byte offsets are documented in comments there.

### Adding a New Fractal Type

1. Add variant to `FractalType` enum (`fractal-core/src/fractals/mod.rs`)
2. Implement SDF function in WGSL (`raymarcher.wgsl`)
3. Add UI controls in `fractal-ui/src/panels/fractal_params.rs`

## Testing Guidelines

- Do NOT write tests that only assert default values from `Default::default()` or constructors. Default values change frequently and such tests add no behavioral coverage. Tests should verify logic, constraints, invariants, and round-trips instead.
- Snapshot/golden-image tests are feature-gated behind `snapshot-tests` and require a GPU: `cargo test -p fractal-renderer --features snapshot-tests`
- To regenerate golden images: `GENERATE_GOLDEN=1 cargo test -p fractal-renderer --features snapshot-tests`
- Every new feature or bug fix **must** include new tests or updates to existing tests that verify the change. If the change affects behavior, there should be a test that would fail without the fix/feature. This applies to all crates — skip only when the change is purely visual/GPU (covered by snapshot tests) or platform-specific (e.g., window management) where automated testing is impractical.

## Conventions

- **Serde backward compatibility**: All serializable structs use `#[serde(default)]` so that old saved sessions still deserialize when new fields are added. Use `#[serde(alias = "old_name")]` when renaming fields. Follow this pattern for any new serializable fields.
- **GPU adapter limits**: The renderer requests `adapter.limits()` instead of `Limits::default()` to support low-power GPUs (e.g., Raspberry Pi VideoCore VI). Do not hardcode limit assumptions.
- **Android page size**: `.cargo/config.toml` sets `-z max-page-size=16384` for all Android targets (16 KB page alignment required by Android 15+). Do not remove these flags.
- **Do not use `with_visible(false)` on winit windows**: On Windows, creating a window with `with_visible(false)` prevents the event loop from delivering `RedrawRequested` events. If `set_visible(true)` is called from inside the render loop, it creates a deadlock — render never runs, so the window never becomes visible. The workaround is to keep the window visible from creation and use in-app splash/loading UI instead.
- **wgpu swapchain textures are uninitialized**: Each swapchain buffer may contain arbitrary data (often white) when first acquired. Always issue a `LoadOp::Clear` on every frame until you are sure all swapchain buffers have been written at least once. The number of buffers depends on `desired_maximum_frame_latency` and the platform.
- **Splash/loading screen duration should be time-based**: Frame-count-based splash dismissal (e.g., "show for 2 frames") results in sub-millisecond visibility at high FPS. Use `Instant::elapsed()` with a minimum duration (e.g., 1 second) instead.
- **Early black clear eliminates OS white flash**: On Windows, the OS paints a white background on new windows before any GPU rendering. To eliminate this, render a `LoadOp::Clear(BLACK)` frame immediately after `RenderContext::new()` inside `App::new()`, before any other initialization (pipeline, egui, etc.). Also start the window at splash size (not maximized) and only maximize after the splash phase ends.

## Key Technical Details

- **Deep zoom**: Uses double-single arithmetic (`hi: f32 + lo: f32`) in WGSL to emulate f64 precision (~14 decimal digits), enabling zoom to ~10^12. Implemented via Knuth's TwoSum and Veltkamp multiplication.
- **Android**: NativeActivity (no Java), loads `libfractal_app.so` (cdylib). Requires Vulkan level 1 hardware feature.
- **Web entry**: `crates/fractal-app/index.html` is the Trunk-managed HTML entry. Assets are copied via `data-trunk` directives.
- **Windows icon**: Embedded at build time via `crates/fractal-app/build.rs` using `winresource`.
- **Last session auto-save**: The app auto-saves the current state to a reserved `__last_session` slot on exit and restores it on next launch. This slot is hidden from the session UI list (IDs starting with `__` are filtered out in `refresh_session_slots()`).
- **Control settings (data-driven UI)**: All slider/drag value ranges are defined in `AppSettings` (TOML config) instead of hardcoded. Config file is at `{data_dir}/settings.toml`. Edit via Debug → Control Settings panel or by hand.

## Reliable Resources on SDFs, Raymarching, Lighting, Rendering

Consult these when working on SDF implementations, ray marching, lighting, or rendering code in `raymarcher.wgsl`:

https://iquilezles.org/articles/distfunctions
https://iquilezles.org/articles/distgradfunctions3d
https://iquilezles.org/articles/bboxes3d
https://iquilezles.org/articles/intersectors
https://iquilezles.org/articles/smoothsteps
https://iquilezles.org/articles/sigmoids
https://iquilezles.org/articles/raymarchingdf
https://iquilezles.org/articles/rmshadows
https://iquilezles.org/articles/normalsSDF
https://iquilezles.org/articles/fbmsdf
https://iquilezles.org/articles/binarysearchsdf
https://iquilezles.org/articles/fog
https://iquilezles.org/articles/outdoorslighting

More links can be found in: https://iquilezles.org/articles
