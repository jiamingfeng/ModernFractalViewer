## What's New

### ✨ New Features
- **Session Management**: Save and load sessions to preserve your fractal explorations
- **Splash Screen**: Added a splash screen on application startup
- **PBR Lighting**: Added physically-based rendering (PBR) lighting option for more realistic visuals
- **Coordinate Gizmo**: Added a coordinate gizmo for spatial orientation
- **Configurable UI**: Added configurable UI setup for personalized layouts
- **App Settings**: Renamed "Control Settings" to "App Settings" for clarity
- **Color Spread**: Updated Color|Scale to "Color Spread" for better color control
- **VSync Debug Option**: Added VSync toggle to debug options (#8)
- **ARM64 Linux Support**: Added arm64 Linux build target
- **App Icon**: Added application icon with a script to update icons across all platforms
- **Test Workflow**: Added automated test workflow with unit tests and screenshot tests

### 🐛 Bug Fixes
- Fixed Menger Sponge rendering issues
- Fixed color banding in rendering
- Fixed zoom factor and normal epsilon calculations
- Solved shadow rendering issues
- Fixed close/open panel button behavior
- Fixed session UI and Android session save failures
- Fixed side panel UI issues
- Fixed web build refresh and redraw delay
- Fixed ARM64 rendering warning
- Fixed Android 16KB memory alignment issue
- Fixed wGPU and WASM build issues
- Fixed log window issues
- Updated shadow and lighting section

### 📖 Documentation
- Added CLAUDE.md for project guidance and build commands
- Added user documentation and features docs
- Updated README, user guide, and testing documentation
- Updated build status badges

### Downloads
| Platform | File |
|----------|------|
| Windows (x64) | `fractal-viewer-*-windows-x64.zip` |
| macOS (ARM) | `fractal-viewer-*-macos-arm64.tar.gz` |
| macOS (x64) | `fractal-viewer-*-macos-x64.tar.gz` |
| Linux (x64) | `fractal-viewer-*-linux-x64.tar.gz` |
| Linux (ARM64) | `fractal-viewer-*-linux-arm64.tar.gz` |
| Web (WASM/WebGPU) | `fractal-viewer-*-web.tar.gz` |
| Android | `fractal-viewer-*-android.apk` |

### Controls
- **Desktop**: Left-drag to orbit, right-drag to pan, scroll to zoom, `R` to reset, `Space` to toggle auto-rotate, `Esc` to toggle UI
- **Android**: Single-finger drag to orbit, two-finger pinch to zoom, two-finger drag to pan
- **Web**: Same as desktop (mouse controls)

### System Requirements
- **Desktop**: GPU with Vulkan, Metal, or DX12 support
- **Web**: Browser with WebGPU support (Chrome 113+, Edge 113+)
- **Android**: Device with Vulkan support (Android 11+)
