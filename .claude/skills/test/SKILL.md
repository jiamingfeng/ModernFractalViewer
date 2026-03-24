---
name: test
description: Run the project test suite with optional crate or snapshot targeting
---

# /test — Run Tests

Run the project test suite.

## Usage

```
/test [target]
```

- No argument: `cargo test --workspace`
- Crate name (e.g., `fractal-core`): `cargo test -p <crate>`
- `snapshot`: run GPU snapshot tests (`cargo test -p fractal-renderer --features snapshot-tests`)
- `golden`: regenerate golden images (`GENERATE_GOLDEN=1 cargo test -p fractal-renderer --features snapshot-tests`)

## Instructions

1. Parse `$ARGUMENTS` to determine the target:
   - Empty or `all` → run `cargo test --workspace`
   - A crate name (`fractal-core`, `fractal-renderer`, `fractal-ui`, `fractal-app`) → run `cargo test -p <crate>`
   - `snapshot` → run `cargo test -p fractal-renderer --features snapshot-tests`
   - `golden` → run `GENERATE_GOLDEN=1 cargo test -p fractal-renderer --features snapshot-tests`

2. Run the appropriate test command using the Bash tool.

3. Report results: number of tests passed/failed, and show any failure details.

## Notes

- CI uses `cargo nextest` with the `ci` profile. If `cargo nextest` is available locally, prefer it: `cargo nextest run --workspace`
- Snapshot tests require a GPU and are excluded from CI
- The nextest CI profile config is at `.config/nextest.toml`
- Testing guidelines: do NOT write tests that only assert default values. Tests should verify logic, constraints, invariants, and round-trips.
