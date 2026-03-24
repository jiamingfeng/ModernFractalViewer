---
name: check-all
description: Pre-push cross-compilation verification for all platforms
---

# /check-all — Pre-Push Verification

Run all cross-compilation checks before pushing. Mirrors what CI enforces.

## Usage

```
/check-all
```

## Instructions

Run these checks sequentially using the Bash tool. Stop on first failure and report the error.

### Step 1: Workspace Check

```bash
cargo check --workspace
```

### Step 2: Hot-Reload Feature Check

```bash
cargo check --workspace --features hot-reload
```

### Step 3: Android Cross-Compilation Check

```bash
cargo check -p fractal-app --target aarch64-linux-android
```

## Output

Report pass/fail for each step. On failure, show the compiler error and suggest a fix. On success:

```
check-all: all 3 checks passed
  [pass] cargo check --workspace
  [pass] cargo check --workspace --features hot-reload
  [pass] cargo check -p fractal-app --target aarch64-linux-android
```

## Notes

- CI enforces `-D warnings` (RUSTFLAGS), so warnings are errors. Do NOT set RUSTFLAGS locally — just run the checks as shown above.
- If the Android target is not installed, suggest: `rustup target add aarch64-linux-android`
