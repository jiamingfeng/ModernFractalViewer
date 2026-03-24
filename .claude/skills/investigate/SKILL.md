---
name: investigate
description: Structured bug investigation tailored to the fractal viewer codebase
---

# /investigate — Bug Investigation

Perform structured root cause analysis before implementing any fix.

## Usage

```
/investigate [symptom description or issue]
```

## Instructions

### 1. Identify the Layer

Determine which crate layer the bug is in:
- **fractal-core** — Math, SDF, types, serialization → pure logic bugs, NaN/infinity, serde issues
- **fractal-renderer** — GPU, shaders, uniforms → visual glitches, alignment issues, pipeline errors
- **fractal-ui** — egui panels, UI state → layout bugs, wrong ranges, missing controls
- **fractal-app** — Platform orchestration, input, sessions → startup crashes, input issues, save/load failures

### 2. Check Platform-Specific Code

This codebase uses `#[cfg(...)]` extensively. Check if the bug is platform-specific:
- **Native vs WASM vs Android** — entry points, async runtime, time, logging, storage all differ
- Search for `#[cfg(target_arch = "wasm32")]`, `#[cfg(target_os = "android")]`, `#[cfg(not(...))]`
- Key file: `crates/fractal-app/src/app.rs` (heavy platform branching)

### 3. Trace the Data Flow

For rendering bugs, trace the uniform pipeline:
```
UI input → Config struct (fractal-core) → Uniforms.update_*() (fractal-renderer) → GPU buffer → WGSL shader
```

Read these files in order:
- The config struct in `fractal-core/src/` (fractals/mod.rs, sdf.rs, camera.rs)
- `crates/fractal-renderer/src/uniforms.rs` (Rust → GPU mapping)
- `crates/fractal-renderer/shaders/raymarcher.wgsl` (shader consumption)

For shader bugs specifically:
- Check `sdf_common.wgsl` for shared SDF utilities
- Check `sdf_volume.wgsl` if mesh export is affected
- Verify byte offsets between Rust Uniforms struct and WGSL struct match

### 4. Form Hypotheses

List at least 2 possible root causes. For each:
- What evidence would confirm it?
- What evidence would refute it?
- Gather evidence by reading code, checking types, tracing values

### 5. Audit Impact

Before proposing a fix:
- Use Grep to find ALL call sites of functions you plan to modify
- Check if the fix affects other platforms (`#[cfg]` branches)
- Check if the fix affects serialization (would old saves break?)
- Check if the fix requires shader changes (both `raymarcher.wgsl` and `sdf_volume.wgsl`)

### 6. Present Findings

```markdown
## Investigation: [Symptom]

### Symptom
[What goes wrong, when, on which platform]

### Root Cause
[What's actually happening and why]

### Evidence
[Code traces, values, logic that confirms the root cause]

### Fix
[Proposed change with specific files and lines]

### Impact
[What else is affected, backward compatibility, platforms]
```

## Common Bug Patterns in This Codebase

- **Uniform alignment mismatch**: Rust struct padding vs WGSL layout (check byte offsets in uniforms.rs)
- **Serde deserialization failures**: Missing `#[serde(default)]` on new fields breaks old saves
- **Platform-specific crashes**: Code works on desktop but fails on WASM (no std::time) or Android (no filesystem)
- **Shader precision**: f32 precision loss at deep zoom (should use double-single arithmetic)
- **LOD interaction**: `effective_iterations` affects all 6 SDF functions — changes propagate widely
- **Swapchain uninitialized**: Missing `LoadOp::Clear` causes white flash or garbage pixels
