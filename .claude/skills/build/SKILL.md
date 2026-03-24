---
name: build
description: Build for any target platform (desktop, android, web, hot-reload)
---

# /build — Build for Target Platform

Build the application for a specified target platform.

## Usage

```
/build [target]
```

- No argument or `desktop`: desktop release build
- `debug`: desktop debug build
- `android`: Android APK via cargo-ndk + gradle
- `web`: WASM/WebGPU via trunk
- `hot-reload`: desktop with shader hot-reload enabled

## Instructions

Parse `$ARGUMENTS` and run the appropriate build.

### Desktop (default)

```bash
cargo run -p fractal-app --release
```

For debug: `cargo run -p fractal-app`

### Android

1. Verify prerequisites are available:
   - `cargo-ndk` installed (`cargo ndk --version`)
   - Android NDK available (`$ANDROID_NDK_HOME` set)
   - Rust target installed (`rustup target list --installed | grep aarch64-linux-android`)

2. Build the native library:
   ```bash
   cargo ndk -t arm64-v8a -o android/app/src/main/jniLibs build -p fractal-app --release
   ```

3. Build the APK:
   ```bash
   cd android && ./gradlew assembleRelease
   ```

4. Report the APK location: `android/app/build/outputs/apk/release/`

If prerequisites are missing, tell the user what to install:
- `cargo install cargo-ndk`
- `rustup target add aarch64-linux-android`
- Android NDK setup instructions

### Web

1. Verify `trunk` is installed (`trunk --version`). If not: `cargo install trunk`
2. Verify wasm32 target: `rustup target add wasm32-unknown-unknown`
3. Build or serve:
   ```bash
   cd crates/fractal-app && trunk serve --release --port 8080
   ```
4. Tell the user to open http://localhost:8080

### Hot-Reload

```bash
cargo run -p fractal-app --features hot-reload
```

Tell the user: shader file at `crates/fractal-renderer/shaders/raymarcher.wgsl` and `settings.toml` are polled every 500ms. Edit, save, and see changes live.

## Notes

- Release profile uses LTO + codegen-units=1 (slower build, faster runtime)
- Android requires 16 KB page alignment (configured in `.cargo/config.toml`)
- Web entry HTML is at `crates/fractal-app/index.html`
