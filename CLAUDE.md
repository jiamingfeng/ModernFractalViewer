# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

For full build commands, architecture, features, conventions, and technical details, see [docs/DEVELOPMENT_GUIDE.md](docs/DEVELOPMENT_GUIDE.md).

## Quick Reference

```bash
# Build & run (desktop)
cargo run -p fractal-app --release

# Check all crates
cargo check --workspace

# Run all tests
cargo test --workspace

# Check Android cross-compilation
cargo check -p fractal-app --target aarch64-linux-android

# Pre-push verification
cargo check --workspace
cargo check --workspace --features hot-reload
cargo check -p fractal-app --target aarch64-linux-android
```

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
- **Feature documentation**: Every new feature must be documented in the `## Features` section of [docs/DEVELOPMENT_GUIDE.md](docs/DEVELOPMENT_GUIDE.md). Include: brief description, key files, data flow, and platform notes. This ensures new contributors can understand and modify any feature without reading every source file.

## Adding a New Fractal Type

1. Add variant to `FractalType` enum (`fractal-core/src/fractals/mod.rs`)
2. Implement SDF function in WGSL (`raymarcher.wgsl`)
3. Add UI controls in `fractal-ui/src/panels/fractal_params.rs`
4. Add range definitions in `fractal-ui/src/app_settings.rs` and `default_app_settings.toml`
5. Document the new type in the Features section of [docs/DEVELOPMENT_GUIDE.md](docs/DEVELOPMENT_GUIDE.md)
