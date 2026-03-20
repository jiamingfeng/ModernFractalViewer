# Testing Guide

## Running Tests

### Unit Tests (no GPU required)

```bash
# Run all unit tests across the workspace
cargo test --workspace

# Run tests for a single crate
cargo test -p fractal-core
cargo test -p fractal-renderer
cargo test -p fractal-ui
cargo test -p fractal-app
```

### Snapshot / Golden-Image Tests (GPU required)

These tests render fractals on the GPU and compare against reference images. They require a GPU adapter (even headless).

```bash
# Run snapshot tests
cargo test -p fractal-renderer --features snapshot-tests

# Regenerate golden images after intentional rendering changes
GENERATE_GOLDEN=1 cargo test -p fractal-renderer --features snapshot-tests
```

### CI

CI runs unit tests on all PC platforms (Windows, macOS, Linux x64, Linux ARM64) via the
`.github/workflows/test.yml` workflow using `cargo-nextest`. Test results are published as
JUnit reports on each PR/push via `dorny/test-reporter`. The release workflow (`release.yml`)
also runs the full test suite before creating a release.

Android compilation is verified via `cargo check` (tests cannot run without a device).
Snapshot tests are excluded from CI since runners have no GPU.

CI enforces `RUSTFLAGS="-D warnings"` so builds fail on any compiler warning.
When running local checks, do **not** set `RUSTFLAGS` manually — just use `cargo check --workspace`.

## Test Inventory

| Crate | Tests | What's covered |
|-------|------:|----------------|
| `fractal-core` | 18 | Camera orbit/zoom/pan/reset, fractal type invariants, serde round-trips, palette preset constraints |
| `fractal-renderer` | 7 unit + 8 snapshot | Uniform buffer layout (512-byte size check, field updates), GPU golden-image regression |
| `fractal-ui` | 2 | UI state management (reset preserves fractal type, set_fractal_type) |
| `fractal-app` | 11 | Session manager CRUD (save/load/delete/list), ID and timestamp format, date math (`days_to_ymd`) |
| **Total** | **46** | |

## Golden / Snapshot Tests

### What They Are

Snapshot tests render each fractal type with default parameters using headless wgpu (no window), then compare the resulting pixels against stored reference images. This catches unintended visual regressions — if a shader change or parameter tweak alters the output, the test fails.

The tests are defined in `crates/fractal-renderer/tests/snapshot_tests.rs` and cover:
- All 6 fractal types with default parameters
- 2 additional color mode variants (normal-based, iteration-based)

### Golden Image Format

- **Format:** Raw RGBA8 pixel data — no header, no compression
- **Resolution:** 128x128 pixels
- **File size:** 65,536 bytes each (128 x 128 x 4 bytes/pixel)
- **Location:** `crates/fractal-renderer/tests/golden/*.raw`

Each file is a flat byte array: `[R, G, B, A, R, G, B, A, ...]` in row-major order.

### Tolerance

Pixel comparison uses a per-channel tolerance of **+/-2** (out of 0-255). This accommodates minor differences between GPU vendors (NVIDIA, AMD, Intel, software renderers) and driver versions. If a pixel channel differs by more than 2 from the reference, the test fails and reports the differing bytes.

If you consistently see failures on a particular GPU, you may need to regenerate the golden images on that hardware.

### Generating / Regenerating Golden Images

Golden images must be regenerated whenever you **intentionally** change rendering output:
- Shader modifications (`raymarcher.wgsl`)
- Default parameter changes (`FractalParams`, `ColorConfig`, `LightingConfig`, etc.)
- Camera or lighting defaults
- Ray marching config defaults

To regenerate:

```bash
GENERATE_GOLDEN=1 cargo test -p fractal-renderer --features snapshot-tests
```

This overwrites all `.raw` files in the `tests/golden/` directory. After regenerating, verify the rendered output is correct (e.g., by inspecting with an image viewer that supports raw RGBA import), then commit the updated `.raw` files.

### Feature Gate

Snapshot tests are gated behind the `snapshot-tests` Cargo feature so that `cargo test --workspace` does not require a GPU. This means:
- Regular `cargo test --workspace` runs only unit tests (46 tests)
- `cargo test -p fractal-renderer --features snapshot-tests` runs unit tests + snapshot tests (54 total)

### Committing Golden Images

The `.raw` files **should be committed** to the repository. They are small (64 KB each, ~512 KB total) and are the reference data that tests compare against. Without them, anyone who clones the repo would need to generate them first.

## Testing Guidelines

- **No default-value tests:** Do not write tests that only assert values from `Default::default()` or constructors. Defaults change frequently and such tests add no behavioral coverage. Tests should verify logic, constraints, invariants, and round-trips instead.
- **Verify behavior, not implementation:** Test what the code does, not how it does it.
- **Serde round-trips:** All serializable structs should have round-trip tests to ensure backward compatibility.
