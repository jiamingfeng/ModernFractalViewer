---
name: review
description: Code review against ModernFractalViewer project conventions
---

# /review — Code Review

Review code changes against project conventions defined in CLAUDE.md.

## Usage

```
/review [target]
```

- No argument: review staged changes (`git diff --cached`)
- `all`: all uncommitted changes (`git diff HEAD`)
- `file:path`: specific file
- `pr`: current PR changes (`gh pr diff`)

## Instructions

1. Get the diff based on the target argument using the Bash tool.
2. Read CLAUDE.md and relevant source files for context.
3. Review against the checklist below.
4. Report findings grouped by severity (critical / warning / suggestion).

## Review Checklist

### Serde & Serialization

- [ ] New serializable fields have `#[serde(default)]` for backward compatibility
- [ ] Renamed fields use `#[serde(alias = "old_name")]`
- [ ] `SavedSession` version field is updated if schema changes

### GPU Uniforms & Shaders

- [ ] No `vec3` or `[f32; 3]` in uniform structs (use individual f32 fields + padding)
- [ ] Byte offsets in Rust `Uniforms` struct match WGSL struct layout
- [ ] Explicit `_padN` fields maintain alignment
- [ ] Total uniform struct size is 512 bytes (compile-time assertion)
- [ ] Changes to `raymarcher.wgsl` SDF functions are also reflected in `sdf_volume.wgsl`
- [ ] SDF functions use `effective_iterations` (not `u.iterations`) for LOD compatibility

### Rendering Conventions

- [ ] GPU adapter limits: uses `adapter.limits()`, not `Limits::default()`
- [ ] No `with_visible(false)` on winit windows
- [ ] Swapchain textures use `LoadOp::Clear` (not `LoadOp::Load` on first use)
- [ ] Time-based durations (not frame-count-based)

### Platform Compatibility

- [ ] Platform-specific code gated with `#[cfg(...)]`
- [ ] WASM: uses `web_time::Instant` (not `std::time::Instant`)
- [ ] Android: no hardcoded filesystem paths, respects `_data_dir_override`
- [ ] Android page size flags in `.cargo/config.toml` preserved

### Testing

- [ ] New feature or bug fix includes tests
- [ ] Tests verify logic/constraints/invariants, NOT just default values
- [ ] Snapshot tests updated if visual changes were made

### Documentation

- [ ] New features documented in `docs/DEVELOPMENT_GUIDE.md` Features section
- [ ] Includes: brief description, key files, data flow, platform notes

## Output Format

```markdown
## Code Review: [summary]

### Critical
- [blocking issues that must be fixed]

### Warnings
- [non-blocking issues that should be addressed]

### Suggestions
- [optional improvements]

### Looks Good
- [things done well worth noting]
```
