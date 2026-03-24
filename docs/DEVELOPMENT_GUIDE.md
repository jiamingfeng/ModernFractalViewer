# Development Guide

This document consolidates all developer-facing content for the Modern 3D Fractal Viewer: build commands, architecture, features, conventions, and technical details.

---

## Build Commands

### Desktop (Windows / macOS / Linux)

```bash
# Debug build
cargo run -p fractal-app

# Release build (optimized, uses LTO + codegen-units=1)
cargo run -p fractal-app --release

# Check all crates without building
cargo check --workspace

# Cross-check Android compilation (requires: rustup target add aarch64-linux-android)
cargo check -p fractal-app --target aarch64-linux-android

# Run tests
cargo test --workspace

# Run tests for a single crate
cargo test -p fractal-core
```

### Web (WebGPU / WASM)

```bash
# Requires trunk
cargo install trunk
cd crates/fractal-app
trunk serve --release --port 8080
# Open http://localhost:8080
```

### Android

```bash
# Requires cargo-ndk and Android NDK
cargo install cargo-ndk
cargo ndk -t arm64-v8a -o android/app/src/main/jniLibs build -p fractal-app --release
# Then build APK: cd android && ./gradlew assembleRelease
```

### Android APK Signing

```bash
# 1. Generate a keystore:
keytool -genkey -v -keystore android/release.jks -keyalg RSA -keysize 2048 -validity 10000 -alias release

# 2. Create android/key.properties:
#    storeFile=../release.jks
#    storePassword=your_store_password
#    keyAlias=release
#    keyPassword=your_key_password

# 3. Build signed APK:
cd android && ./gradlew assembleRelease
```

For CI signing, set these GitHub secrets:
- `ANDROID_KEYSTORE_BASE64` (base64-encoded .jks file)
- `ANDROID_KEYSTORE_PASSWORD`
- `ANDROID_KEY_ALIAS`
- `ANDROID_KEY_PASSWORD`

### Hot-Reload (Shader Development)

```bash
# Run with shader hot-reload enabled (dev only)
cargo run -p fractal-app --features hot-reload
```

When enabled, the app polls `crates/fractal-renderer/shaders/raymarcher.wgsl` for changes every 500ms. Edit the shader, save, and see results live. Compile errors are logged and the old shader continues rendering. Also hot-reloads `settings.toml` config changes.

### CI

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

---

## Architecture

The project is a 4-crate Rust workspace. Data flows: `fractal-core` (math/types) → `fractal-renderer` (GPU) + `fractal-ui` (egui panels) → `fractal-app` (application orchestration).

### Crates

- **`fractal-core`** — Platform-agnostic math: camera, SDF primitives, all 6 fractal definitions (`FractalType` enum + `FractalParams` struct), mesh data types and export (glTF). No GPU dependencies.
- **`fractal-renderer`** — wgpu context (`context.rs`), render pipeline (`pipeline.rs`), GPU uniform buffers (`uniforms.rs`), compute pipeline for SDF volume sampling (`compute.rs`), and WGSL shaders (`shaders/`). SDF functions are shared between `raymarcher.wgsl` and `sdf_volume.wgsl` via `sdf_common.wgsl`.
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
4. Add range definitions in `fractal-ui/src/app_settings.rs` and `default_app_settings.toml`
5. Document the new type in the Features section below

---

## Features

### Fractal Types & SDF Rendering

Six 3D fractal types rendered via GPU ray marching: Mandelbulb (power 1–16, default 8), Menger Sponge, Julia 3D (quaternion, configurable C vector), Mandelbox (box folding + spherical inversion), Sierpinski Tetrahedron, and Apollonian Gasket. Each has a dedicated SDF function in the WGSL shader with per-type configurable parameters.

- **Files**: `fractal-core/src/fractals/mod.rs` (FractalType enum, FractalParams struct), `fractal-renderer/shaders/raymarcher.wgsl` (SDF implementations, ~650 lines), `fractal-ui/src/panels/fractal_params.rs` (per-type UI controls)
- **Data flow**: UI → `FractalParams` → `Uniforms.update_fractal()` → GPU uniform buffer → WGSL `sdf()` function dispatches by `fractal_type` field
- **Adding a type**: Add enum variant → implement SDF in WGSL → add UI panel → add range config → document

### Deep Zoom (Double-Single Arithmetic)

Emulates f64 precision (~14 decimal digits) using pairs of f32 values (`hi + lo`), enabling zoom to ~10^12 magnification. Uses Knuth's TwoSum for addition and Veltkamp's method for multiplication.

- **Files**: `fractal-renderer/shaders/raymarcher.wgsl` (lines ~122–158, `DS` struct and arithmetic functions)
- **Note**: Only affects camera/ray generation precision; SDF evaluation uses standard f32

### Continuous Level of Detail (LOD)

Pixel-footprint-based LOD that filters sub-pixel geometry and SDF noise. Based on Inigo Quilez's technique. Three mechanisms work together: (1) adaptive epsilon grows as `base_epsilon + t * pixel_angular_size * lod_scale`, (2) SDF iteration count is reduced at distance via `var<private> effective_iterations` (fewer iterations = smoother SDF = less aliasing), (3) minimum step size prevents ray micro-stepping through noisy SDF regions near surfaces.

- **Files**: `fractal-core/src/sdf.rs` (lod_enabled, lod_scale in RayMarchConfig), `fractal-renderer/src/uniforms.rs` (GPU uniform fields at offset 436-440), `fractal-renderer/shaders/raymarcher.wgsl` (adaptive epsilon + iteration reduction in ray_march(), LOD-aware calc_normal(), effective_iterations set in render_sample()), `fractal-ui/src/panels/mod.rs` (checkbox + slider)
- **Data flow**: UI checkbox/slider → RayMarchConfig.lod_enabled/lod_scale → Uniforms.update_ray_march() → GPU uniform → WGSL ray_march() computes adaptive epsilon + iteration reduction per step, render_sample() sets effective_iterations for shading
- **Config**: lod_enabled (bool, default true), lod_scale (0.1–5.0, default 1.0). Higher lod_scale = more aggressive culling = faster but less distant detail.
- **Iteration reduction formula**: `reduce = log2(pixel_footprint * lod_scale / epsilon)`, minimum 3 iterations always preserved. All 6 SDF functions read `effective_iterations` instead of `u.iterations`.

### Camera System

Orbital camera with azimuth/elevation/distance. Supports mouse orbit (left drag), pan (right drag), scroll zoom (logarithmic), touch gestures (single-finger orbit, two-finger pinch-zoom and pan), keyboard shortcuts (R=reset, Space=auto-rotate, Esc=toggle panel), and preset views (Top, Front).

- **Files**: `fractal-core/src/camera.rs` (Camera struct, orbital math, view/projection matrices), `fractal-app/src/app.rs` (event handling: `handle_mouse_move`, `handle_scroll`, `handle_touch`), `fractal-app/src/input.rs` (InputState, TouchPoint, pinch distance math), `fractal-ui/src/panels/camera_controls.rs` (FOV/zoom sliders, preset buttons)
- **Data flow**: Input events → Camera.orbit()/zoom_by()/pan() → Camera.update_position() → pushed to UiState each frame → Uniforms.update_camera() → GPU
- **Serialization**: Camera uses `#[serde(alias = "zoom")]` on `distance` field for backward compatibility with old saves

### Color & Palette System

Five color modes (Solid, Orbit Trap, Iteration, Normal, Combined) with 8 built-in palette presets (Inferno, Ocean, Sunset, Magma, Viridis, Classic, Fire, Ice). Custom palettes support up to 8 color stops with live color picker editing. Catmull-Rom interpolation for smooth gradients. Triangular dithering eliminates 8-bit color banding.

- **Files**: `fractal-core/src/sdf.rs` (ColorConfig struct, PALETTE_PRESETS array), `fractal-ui/src/panels/color_settings.rs` (palette editor, color mode selector, dither slider), `fractal-renderer/shaders/raymarcher.wgsl` (palette sampling via Catmull-Rom, dithering)
- **Data flow**: UI → ColorConfig (palette_colors array, color_mode, palette_scale/offset) → Uniforms.update_color() → GPU palette lookup per pixel
- **Config**: palette_scale (0.1–10.0 log), palette_offset (0–1), dither_strength (0–2), max 8 color stops

### Lighting & Ambient Occlusion

Blinn-Phong lighting with ambient, diffuse, and specular components. Soft ambient occlusion via multi-step SDF sampling along the surface normal. Configurable AO steps (0–16) and intensity.

- **Files**: `fractal-core/src/sdf.rs` (LightingConfig struct), `fractal-ui/src/panels/color_settings.rs` (lighting sliders), `fractal-renderer/shaders/raymarcher.wgsl` (lighting calculation, AO loop)
- **Data flow**: UI → LightingConfig → Uniforms.update_lighting() → GPU
- **Parameters**: ambient (0–1), diffuse (0–1), specular (0–1), shininess (1–128), light direction [0.577, 0.577, 0.577]

### Rendering Pipeline & Anti-Aliasing

Single fullscreen ray marching pass using a 3-vertex triangle (no vertex buffer). wgpu with Naga transpiler converts WGSL to native GPU shaders at runtime. Supersampling anti-aliasing at 1x (default), 2x (diagonal), or 4x (RGSS pattern).

- **Files**: `fractal-renderer/src/pipeline.rs` (FractalPipeline, shader compilation, render pass), `fractal-renderer/src/context.rs` (wgpu device/queue/surface setup), `fractal-renderer/shaders/raymarcher.wgsl` (vertex + fragment shaders)
- **Data flow**: App.render() → get_current_texture() → create encoder → pipeline.render() (clear BLACK + draw 3 vertices) → egui overlay pass (LoadOp::Load) → submit + present
- **Configuration**: RayMarchConfig (max_steps, epsilon, max_distance, ao_steps, normal_epsilon, sample_count)

### Uniform Buffer Layout

512-byte `#[repr(C)]` struct sent to GPU each frame. No `vec3` fields (WGSL aligns vec3 to 16 bytes). Explicit `_padN` fields ensure byte-perfect alignment between Rust and WGSL. Palette stored as 8 `[f32; 4]` slots.

- **Files**: `fractal-renderer/src/uniforms.rs` (Uniforms struct with byte offset comments, update methods), `fractal-renderer/shaders/raymarcher.wgsl` (matching WGSL struct at binding 0, group 0)
- **Key rule**: When adding new uniform fields, update both the Rust struct (with padding) AND the WGSL struct. Verify 512-byte size (compile-time assertion in tests).

### Session Save/Load

Saves complete fractal state as JSON with optional 320x180 PNG thumbnail (base64-encoded). Platform-aware storage: filesystem on native/Android, localStorage on WASM. Confirmation dialogs for overwrite and delete. Reserved `__` prefix IDs are system-internal (read-only in UI).

- **Files**: `fractal-core/src/session.rs` (SavedSession struct), `fractal-app/src/session_manager.rs` (SessionManager, StorageBackend trait, FileSystemStorage, LocalStorageBackend), `fractal-ui/src/panels/session_panel.rs` (UI with thumbnail preview, Load/Save/Delete buttons, confirmation dialogs), `fractal-renderer/src/thumbnail.rs` (offscreen render + GPU→CPU copy)
- **Data flow**: Save: UiState + Camera → SavedSession → JSON → StorageBackend.save(). Load: StorageBackend.load() → JSON → SavedSession → restore into UiState + Camera
- **Backward compat**: All nested structs use `#[serde(default)]`. SavedSession has `version: "1"` for future schema migrations.

### Splash Screen

Displays a branded loading screen during startup: background image (800x450), app name, version+commit, loading status, app icon + copyright. Window starts at splash resolution, maximizes after splash ends. Early black clear frame eliminates OS white flash. Minimum display time configurable via `SPLASH_MIN_DURATION_SECS` constant (default 2s).

- **Files**: `fractal-app/src/app.rs` (SplashState struct, render_splash_frame(), splash lifecycle in render()), `fractal-app/assets/splash.png` (background image), `fractal-app/assets/icon.png` (corner icon)
- **Lifecycle**: App::new() → early black clear → load splash textures → render splash frames for SPLASH_MIN_DURATION_SECS → set_maximized(true) → normal fractal rendering
- **Platform**: Native only (splash textures `None` on WASM; WASM uses HTML loading indicator)

### Data-Driven Settings (AppSettings)

All slider/drag value min/max/speed/decimals are defined in `AppSettings` struct and persisted as TOML. Includes app behavior flags (e.g., `auto_load_last_session`). Editable via Debug → Control Settings panel or by hand-editing `settings.toml`.

- **Files**: `fractal-ui/src/app_settings.rs` (AppSettings struct hierarchy: FloatRange, IntRange, per-fractal ranges, camera/rendering/lighting/color/debug ranges), `fractal-ui/src/default_app_settings.toml` (commented defaults, embedded at compile time), `fractal-app/src/config_manager.rs` (load_settings/save_settings for native + WASM), `fractal-ui/src/panels/control_settings_panel.rs` (editor UI)
- **Data flow**: Startup: config_manager.load_settings() → ui_state.settings. UI changes: set settings_dirty → save_settings_if_dirty() writes TOML. Panels read settings for slider ranges.
- **File location**: Native: `{data_dir}/ModernFractalViewer/settings.toml`. WASM: localStorage key `fractal_settings`.

### Hot-Reload

Feature-gated (`--features hot-reload`). Polls shader file and config TOML every 500ms for changes. On shader change: recompiles WGSL → rebuilds render pipeline → swaps atomically. On config change: re-parses TOML → updates AppSettings. Compile errors logged; old shader/config continues.

- **Files**: `fractal-app/src/hot_reload.rs` (HotReloader struct, HotReloadEvent enum, file mtime polling), `fractal-renderer/src/pipeline.rs` (reload_shader() method, shader_path() resolver, feature-gated disk loading in resolve_shader_source())
- **Data flow**: HotReloader.poll() → ShaderChanged → read file → pipeline.reload_shader() → new RenderPipeline. ConfigChanged → read TOML → toml::from_str → update ui_state.settings.
- **Error handling**: Uses device.push_error_scope() to catch shader compile errors without panicking

### Last Session Auto-Save/Load

Auto-saves current state to reserved `__last_session` slot on app exit (without thumbnail for speed). Auto-loads on next launch if `settings.auto_load_last_session` is true (default: false). The slot appears in the session list as read-only (Load button only, no Save/Delete).

- **Files**: `fractal-app/src/app.rs` (save_last_session() called on CloseRequested, auto-load in App::new()), `fractal-ui/src/panels/session_panel.rs` (hides Save/Delete for `__` prefix IDs)
- **Data flow**: Exit: build SavedSession from current state → session_manager.save_overwrite("__last_session"). Launch: load("__last_session") → restore params/camera.

### Version Tracking

Build-time capture of git tag and commit hash via `build.rs`. Displayed in splash screen, debug overlay, and Debug section of the control panel.

- **Files**: `fractal-app/build.rs` (git describe + rev-parse → cargo:rustc-env), `fractal-app/src/app.rs` (formats `env!("APP_VERSION")` and `env!("APP_COMMIT")` into ui_state.version_info)
- **Rebuild trigger**: `cargo:rerun-if-changed=.git/HEAD` and `.git/refs/`

### Input Handling

Mouse, touch, and keyboard input with egui priority (egui consumes events first, remaining events go to camera). Touch supports single-finger orbit, two-finger pinch-zoom, and two-finger pan with midpoint tracking.

- **Files**: `fractal-app/src/input.rs` (InputState struct, TouchPoint, pinch_distance/midpoint helpers), `fractal-app/src/app.rs` (handle_window_event dispatches to handle_keyboard/mouse/touch)
- **Key bindings**: Esc (toggle panel), R (reset camera), Space (toggle auto-rotate)

### Debug Overlay

Toggleable overlay (top-right) showing FPS, version info, camera position, and zoom level. Controlled by `show_debug` checkbox in Debug section.

- **Files**: `fractal-app/src/app.rs` (debug egui::Window in render()), `fractal-ui/src/panels/mod.rs` (Debug collapsing section with checkboxes for debug, VSync, auto-rotate)

### Mesh Export

GPU-accelerated SDF volume sampling followed by CPU mesh extraction and file export. Supports three extraction algorithms, post-processing (smoothing, decimation), and multiple output formats.

- **Extraction algorithms**: Marching Cubes (fast, deduplicated vertices), Dual Contouring (watertight, sharp features via QEF), Surface Nets (inherently smooth)
- **Post-processing**: Laplacian smoothing, Taubin volume-preserving smoothing, QEM mesh decimation
- **Export formats**: glTF Binary (.glb) with PBR material, Wavefront OBJ (.obj), Stanford PLY (.ply) with per-vertex RGBA
- **Quality controls**: Adaptive iso-level (scales with voxel size for thin feature preservation), boundary extension (expands sampling volume to avoid clipping)
- **Files**: `fractal-core/src/mesh/` (all extraction and export modules), `fractal-renderer/src/compute.rs` + `shaders/sdf_volume.wgsl` (GPU SDF sampling), `fractal-app/src/app.rs` (export orchestration: `start_export`, `spawn_export_thread`), `fractal-ui/src/panels/export_panel.rs` (UI)
- **Data flow**: `ExportConfig` (UI) → GPU compute (SDF volume) → async readback → CPU extraction (MC/DC/SN) → smoothing → decimation → vertex coloring → file export
- **Platform**: Desktop only (disabled on Android/WASM due to GPU readback requirements)

### Cross-Platform Storage

Trait-based storage abstraction (`StorageBackend`) with platform-specific implementations. Sessions and config share the same data directory pattern but use different subdirectories/files.

- **Files**: `fractal-app/src/session_manager.rs` (StorageBackend trait, FileSystemStorage for native, LocalStorageBackend for WASM), `fractal-app/src/config_manager.rs` (settings TOML I/O, WASM localStorage)
- **Paths**: Native: `{dirs::data_dir()}/ModernFractalViewer/` (saves/ for sessions, settings.toml for config). Android: `{internal_data_path}/` (passed via _data_dir_override). WASM: localStorage keys `fractal_save_*` and `fractal_settings`.

---

## Testing

- Do NOT write tests that only assert default values from `Default::default()` or constructors. Default values change frequently and such tests add no behavioral coverage. Tests should verify logic, constraints, invariants, and round-trips instead.
- Snapshot/golden-image tests are feature-gated behind `snapshot-tests` and require a GPU: `cargo test -p fractal-renderer --features snapshot-tests`
- To regenerate golden images: `GENERATE_GOLDEN=1 cargo test -p fractal-renderer --features snapshot-tests`
- Every new feature or bug fix **must** include new tests or updates to existing tests that verify the change.

For the full test inventory and detailed testing instructions, see [TESTING.md](TESTING.md).

---

## Conventions

- **Serde backward compatibility**: All serializable structs use `#[serde(default)]` so that old saved sessions still deserialize when new fields are added. Use `#[serde(alias = "old_name")]` when renaming fields. Follow this pattern for any new serializable fields.
- **GPU adapter limits**: The renderer requests `adapter.limits()` instead of `Limits::default()` to support low-power GPUs (e.g., Raspberry Pi VideoCore VI). Do not hardcode limit assumptions.
- **Android page size**: `.cargo/config.toml` sets `-z max-page-size=16384` for all Android targets (16 KB page alignment required by Android 15+). Do not remove these flags.
- **Do not use `with_visible(false)` on winit windows**: On Windows, creating a window with `with_visible(false)` prevents the event loop from delivering `RedrawRequested` events. If `set_visible(true)` is called from inside the render loop, it creates a deadlock — render never runs, so the window never becomes visible. The workaround is to keep the window visible from creation and use in-app splash/loading UI instead.
- **wgpu swapchain textures are uninitialized**: Each swapchain buffer may contain arbitrary data (often white) when first acquired. Always issue a `LoadOp::Clear` on every frame until you are sure all swapchain buffers have been written at least once. The number of buffers depends on `desired_maximum_frame_latency` and the platform.
- **Splash/loading screen duration should be time-based**: Frame-count-based splash dismissal (e.g., "show for 2 frames") results in sub-millisecond visibility at high FPS. Use `Instant::elapsed()` with a minimum duration (e.g., 1 second) instead.
- **Early black clear eliminates OS white flash**: On Windows, the OS paints a white background on new windows before any GPU rendering. To eliminate this, render a `LoadOp::Clear(BLACK)` frame immediately after `RenderContext::new()` inside `App::new()`, before any other initialization (pipeline, egui, etc.). Also start the window at splash size (not maximized) and only maximize after the splash phase ends.
- **Feature documentation**: Every new feature must be documented in the Features section of this file. Include: brief description, key files, data flow, and platform notes. This ensures new contributors can understand and modify any feature without reading every source file.

---

## Key Technical Details

- **Deep zoom**: Uses double-single arithmetic (`hi: f32 + lo: f32`) in WGSL to emulate f64 precision (~14 decimal digits), enabling zoom to ~10^12. Implemented via Knuth's TwoSum and Veltkamp multiplication.
- **Android**: NativeActivity (no Java), loads `libfractal_app.so` (cdylib). Requires Vulkan level 1 hardware feature.
- **Web entry**: `crates/fractal-app/index.html` is the Trunk-managed HTML entry. Assets are copied via `data-trunk` directives.
- **Windows icon**: Embedded at build time via `crates/fractal-app/build.rs` using `winresource`.
- **Last session auto-save**: The app auto-saves the current state to a reserved `__last_session` slot on exit and restores it on next launch. This slot is hidden from the session UI list (IDs starting with `__` are filtered out in `refresh_session_slots()`).
- **Control settings (data-driven UI)**: All slider/drag value ranges are defined in `AppSettings` (TOML config) instead of hardcoded. Config file is at `{data_dir}/settings.toml`. Edit via Debug → Control Settings panel or by hand.

---

## Resources

Consult these when working on SDF implementations, ray marching, lighting, or rendering code in `raymarcher.wgsl`:

Links can be found in: https://iquilezles.org/articles
