# Learning Rust, wgpu & egui — A Fractal Viewer Tutorial

> **Audience**: Experienced C++ / graphics developers new to the Rust ecosystem.
> Every concept is mapped to its C++ equivalent and references real code in this project.

---

## Table of Contents

1. [Rust for C++ Developers — Ownership & Types](#chapter-1-rust-for-c-developers--ownership--types)
2. [Rust for C++ Developers — Enums, Traits & Error Handling](#chapter-2-rust-for-c-developers--enums-traits--error-handling)
3. [Rust Project Structure — Workspace & Modules](#chapter-3-rust-project-structure--workspace--modules)
4. [wgpu Initialization — From Instance to Pixels](#chapter-4-wgpu-initialization--from-instance-to-pixels)
5. [The Render Pipeline — Shaders, Bind Groups & Drawing](#chapter-5-the-render-pipeline--shaders-bind-groups--drawing)
6. [CPU-GPU Data Flow — Uniforms & bytemuck](#chapter-6-cpugpu-data-flow--uniforms--bytemuck)
7. [WGSL Shader Deep Dive — Ray Marching a Fractal](#chapter-7-wgsl-shader-deep-dive--ray-marching-a-fractal)
8. [egui Integration — Immediate-Mode UI on wgpu](#chapter-8-egui-integration--immediate-mode-ui-on-wgpu)
9. [Putting It All Together — The Application Loop](#chapter-9-putting-it-all-together--the-application-loop)
10. [Compute Shaders & Advanced GPU Patterns](#chapter-10-compute-shaders--advanced-gpu-patterns)
- [Appendix A: Exercises](#appendix-a-exercises)
- [Appendix B: Quick Reference Tables](#appendix-b-quick-reference-tables)

---

## Chapter 1: Rust for C++ Developers — Ownership & Types

This chapter covers the single biggest mental shift from C++ to Rust: ownership and borrowing. If you internalize this chapter, the rest of Rust follows naturally.

### 1.1 Ownership = Compile-Time RAII

In C++ you manage lifetimes with RAII, `unique_ptr`, and `shared_ptr`. Rust replaces all three with a single concept: **ownership**.

```
C++                                 Rust
──────────────────────────────────  ──────────────────────────────────
std::unique_ptr<Texture> tex;       let tex: Texture;
// Destroyed at scope end           // Destroyed at scope end (Drop)

auto tex2 = std::move(tex);         let tex2 = tex;
// tex is now null — UB if used     // tex is GONE — compile error if used
```

Every value in Rust has exactly **one owner**. When the owner goes out of scope, the value is dropped (Rust's equivalent of a destructor). There's no garbage collector and no reference counting by default.

**In this project**, see how `FractalPipeline` owns its GPU resources:

```rust
// crates/fractal-renderer/src/pipeline.rs:21-28
pub struct FractalPipeline {
    pub render_pipeline: wgpu::RenderPipeline,  // Owned
    pub uniform_buffer: wgpu::Buffer,           // Owned
    pub uniform_bind_group: wgpu::BindGroup,    // Owned
    pub uniforms: Uniforms,                     // Owned (Copy type)
    format: wgpu::TextureFormat,                // Copy type
}
```

When a `FractalPipeline` is dropped, all its fields are dropped automatically — the `Buffer`, `BindGroup`, and `RenderPipeline` are released. No explicit cleanup, no destructor body needed. This is equivalent to a C++ struct where every member is a `unique_ptr`.

### 1.2 Move Semantics — Rust's Default

In C++ you explicitly `std::move()`. In Rust, assignment **always moves** unless the type implements `Copy`.

```rust
let pipeline = FractalPipeline::new(&ctx);
let pipeline2 = pipeline;   // pipeline is MOVED into pipeline2
// pipeline.render(...)      // Compile error: "value used after move"
```

> **Tip**: Rust's move is a bitwise memcpy — no move constructors, no moved-from state, no UB. The compiler simply forbids using the old name. This is *simpler* than C++ move semantics.

Types that are small and trivially copyable can opt into `Copy`, which makes assignment clone the bits implicitly:

```rust
// crates/fractal-core/src/fractals/mod.rs:15
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum FractalType {
    #[default]
    Mandelbulb = 0,
    Menger = 1,
    // ...
}
```

`Copy` on `FractalType` means you can assign it without moving — just like an `int` in C++.

### 1.3 Borrowing — References Without Ownership

Instead of passing ownership, you can **borrow** a value:

```
C++                                 Rust
──────────────────────────────────  ──────────────────────────────────
void render(const Pipeline& p);     fn render(p: &Pipeline);
void update(Pipeline& p);           fn update(p: &mut Pipeline);
```

Rust enforces **one rule** at compile time:

> You can have **either** one `&mut T` (exclusive/mutable) **or** any number of `&T` (shared/immutable) — but never both simultaneously.

This is the "borrow checker" and it eliminates data races, iterator invalidation, and use-after-free at compile time.

**In this project**, see how `render()` borrows the pipeline immutably while `update_uniforms()` borrows it mutably:

```rust
// crates/fractal-renderer/src/pipeline.rs:258-280
pub fn render(
    &self,                              // immutable borrow of pipeline
    encoder: &mut wgpu::CommandEncoder, // mutable borrow of encoder
    view: &wgpu::TextureView,          // immutable borrow of view
) {
    // self.uniforms is read-only here (can't accidentally modify mid-render)
    // encoder is mutated (render pass writes commands into it)
}

// crates/fractal-renderer/src/pipeline.rs:253
pub fn update_uniforms(&mut self, queue: &wgpu::Queue) {
    // &mut self — exclusive access, safe to modify self.uniforms
    queue.write_buffer(&self.uniform_buffer, 0, bytemuck::cast_slice(&[self.uniforms]));
}
```

> **Gotcha**: You can't call `pipeline.update_uniforms()` and `pipeline.render()` in the same expression because one needs `&mut self` and the other needs `&self`. Rust forces you to finish mutating before you start reading. In C++ this ordering bug would be silent.

### 1.4 Lifetimes — Scope Annotations for References

Lifetimes are Rust's way of ensuring references don't outlive the data they point to. Most of the time they're **inferred** (elided) and you never write them.

```rust
// Lifetime elided — Rust infers the return reference lives as long as `self`
pub fn name(&self) -> &str { ... }

// Equivalent to:
pub fn name<'a>(&'a self) -> &'a str { ... }
```

**In this project**, explicit lifetimes appear mainly in the egui widget builders:

```rust
// crates/fractal-ui/src/app_settings.rs:48
pub fn slider<'a>(&self, value: &'a mut f32) -> egui::Slider<'a> {
    let mut s = egui::Slider::new(value, self.min..=self.max);
    // ...
    s
}
```

The `'a` lifetime says: "the returned `Slider` borrows `value` and cannot outlive it." This is exactly the same guarantee as "the pointer inside `Slider` is valid as long as `value` is alive" — but checked at compile time.

> **Tip**: If you're fighting lifetimes, you're often fighting the wrong design. Restructure to reduce reference nesting. Most Rust code uses very few explicit lifetimes.

### 1.5 Arc and Mutex — Shared Ownership Across Threads

When you truly need shared ownership (like C++ `shared_ptr`), Rust provides `Arc<T>` (atomic reference count) and `Mutex<T>` (mutual exclusion).

```
C++                                          Rust
───────────────────────────────────────────  ─────────────────────────────────
std::shared_ptr<std::mutex<LogBuffer>> buf;  Arc<Mutex<VecDeque<LogEntry>>>
buf->lock();                                 buf.lock().unwrap()
```

**In this project**, the log capture system shares a ring buffer between the logger (any thread) and the UI (main thread):

```rust
// crates/fractal-app/src/log_capture.rs:29
pub type LogBuffer = Arc<Mutex<VecDeque<LogEntry>>>;
```

The logger writes to it from the `log::Log` trait impl, and the UI reads from it every frame:

```rust
// Writing (log_capture.rs:59-64)
if let Ok(mut buf) = self.buffer.lock() {
    if buf.len() >= MAX_ENTRIES {
        buf.pop_front();      // Ring buffer eviction
    }
    buf.push_back(entry);
}

// Reading (app.rs:900)
if let Ok(entries) = self.log_entries.lock() {
    for entry in entries.iter() { /* display in UI */ }
}
```

> **Gotcha**: `Mutex::lock()` returns a `Result` because the mutex can be "poisoned" (a thread panicked while holding the lock). Use `.lock().unwrap()` if you want to propagate panics, or `if let Ok(guard) = lock()` to silently skip poisoned mutexes (as this project does).

### 1.6 The `Window` Ownership Pattern

The fractal viewer demonstrates a common Rust pattern for shared window ownership:

```rust
// crates/fractal-app/src/main.rs:53-57
let window = Arc::new(
    event_loop
        .create_window(self.window_attrs.clone())
        .expect("Failed to create window"),
);
```

The window is wrapped in `Arc` because multiple systems need it: wgpu's `Surface` needs to reference it, egui-winit needs it for DPI scaling, and the app needs it for `request_redraw()`. In C++ you'd use `shared_ptr<Window>` — same idea, but Rust's type system guarantees you can't accidentally dereference a null `Arc`.

### 1.7 Key Differences Summary

| Concept | C++ | Rust |
|---------|-----|------|
| Default assignment | Copy (can be expensive) | Move (always cheap) |
| Explicit copy | Copy constructor / `=` | `.clone()` |
| Unique ownership | `unique_ptr<T>` | `T` (just own it) |
| Shared ownership | `shared_ptr<T>` | `Arc<T>` |
| Mutable reference | `T&` | `&mut T` |
| Const reference | `const T&` | `&T` |
| Destructor | `~Class()` | `impl Drop for T` |
| Null pointer | `nullptr` | Does not exist — use `Option<T>` |
| Data race prevention | Thread sanitizer (runtime) | Borrow checker (compile time) |

---

## Chapter 2: Rust for C++ Developers — Enums, Traits & Error Handling

Rust's enums and traits replace C++ inheritance hierarchies, `std::variant`, virtual methods, and exceptions. This chapter shows how.

### 2.1 Enums — Tagged Unions on Steroids

C++ `enum class` holds a numeric value. Rust enums hold **data**:

```
C++                                      Rust
──────────────────────────────────────── ────────────────────────────────────
enum class Shape { Circle, Rectangle };  enum Shape {
                                             Circle { radius: f32 },
// Data stored separately:                   Rectangle { w: f32, h: f32 },
struct ShapeData {                       }
    Shape kind;
    union { float radius; struct { float w, h; }; };
};
```

This is equivalent to `std::variant<Circle, Rectangle>` but with exhaustive pattern matching enforced by the compiler.

**In this project**, `FractalType` is a simple C-like enum — no data per variant:

```rust
// crates/fractal-core/src/fractals/mod.rs:15-25
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[repr(u32)]
pub enum FractalType {
    #[default]
    Mandelbulb = 0,
    Menger = 1,
    Julia3D = 2,
    Mandelbox = 3,
    Sierpinski = 4,
    Apollonian = 5,
}
```

Key attributes explained:
- `#[repr(u32)]` — Forces the discriminant to be a `u32` in memory. This is critical because the GPU shader reads this value as `u32 fractal_type` at byte offset 80 in the uniform buffer. Without `repr(u32)`, Rust picks whatever size it wants.
- `#[default]` on `Mandelbulb` — Makes `FractalType::default()` return `Mandelbulb`.
- `#[derive(...)]` — Auto-generates trait implementations (more on this in 2.4).

### 2.2 Pattern Matching — Exhaustive switch

Rust's `match` is like C++ `switch` but the compiler **forces you to handle every variant**:

```rust
// crates/fractal-core/src/fractals/mod.rs:41-50
pub fn name(&self) -> &'static str {
    match self {
        FractalType::Mandelbulb => "Mandelbulb",
        FractalType::Menger => "Menger Sponge",
        FractalType::Julia3D => "Julia 3D",
        FractalType::Mandelbox => "Mandelbox",
        FractalType::Sierpinski => "Sierpinski",
        FractalType::Apollonian => "Apollonian",
    }
}
```

If you add a new variant to `FractalType`, every `match` in the codebase that doesn't handle it becomes a compile error. In C++ you'd need `-Wswitch` and even then it's a warning.

Pattern matching also works with data-carrying enums:

```rust
// crates/fractal-ui/src/panels/fractal_params.rs:43-62
match params.fractal_type {
    FractalType::Mandelbulb => {
        changed |= Self::show_mandelbulb_params(ui, params, &ranges.mandelbulb);
    }
    FractalType::Menger => {
        changed |= show_iterations(ui, params, &ranges.menger.iterations);
    }
    FractalType::Julia3D => {
        changed |= Self::show_julia_params(ui, params, &ranges.julia3d);
    }
    // ... each type gets its own UI panel
}
```

> **Tip**: Rust enums are "closed" — you define all variants in one place and the compiler knows them all. Use enums when the set of variants is fixed (fractal types, error kinds). Use traits when the set is open-ended (storage backends, custom plugins).

### 2.3 Traits — Interfaces Without Inheritance

Rust has no class inheritance. Instead, shared behavior is defined through **traits** (similar to C++20 concepts or Java interfaces).

```
C++                                      Rust
──────────────────────────────────────── ────────────────────────────────────
class StorageBackend {                   trait StorageBackend {
public:                                      fn save(&self, id: &str, data: &str)
    virtual void save(                           -> Result<()>;
        string id, string data) = 0;        fn load(&self, id: &str)
    virtual string load(string id) = 0;          -> Result<String>;
    virtual ~StorageBackend() = default;     fn delete(&self, id: &str)
};                                               -> Result<()>;
                                             fn list(&self) -> Result<Vec<String>>;
                                         }
```

**In this project**, session storage uses a trait with platform-specific implementations:

```rust
// crates/fractal-app/src/session_manager.rs:47-55
trait StorageBackend {
    fn save(&self, id: &str, data: &str) -> Result<()>;
    fn load(&self, id: &str) -> Result<String>;
    fn delete(&self, id: &str) -> Result<()>;
    fn list(&self) -> Result<Vec<String>>;
}
```

Native (filesystem) and WASM (localStorage) each implement this trait. The code is conditionally compiled — only the relevant impl exists for each target:

```rust
#[cfg(not(target_arch = "wasm32"))]
impl StorageBackend for FileSystemStorage { ... }

#[cfg(target_arch = "wasm32")]
impl StorageBackend for LocalStorageBackend { ... }
```

> **Gotcha**: There's no inheritance hierarchy — you can't "extend" a struct. Composition is the Rust way. If you catch yourself thinking "this struct should inherit from that one," use a trait instead and implement it for both types.

### 2.4 Derive Macros — Auto-Generated Trait Implementations

In C++ you manually write copy constructors, `operator==`, `operator<<`, etc. Rust auto-generates these via `#[derive(...)]`:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum FractalType { ... }
```

| Derive | C++ Equivalent | What It Generates |
|--------|----------------|-------------------|
| `Debug` | `operator<<(ostream&)` | Format for `{:?}` debug printing |
| `Clone` | Copy constructor | `.clone()` method (explicit deep copy) |
| `Copy` | Trivially copyable | Implicit bitwise copy on assignment |
| `PartialEq` | `operator==` | `==` and `!=` comparison |
| `Eq` | (marks total equality) | Asserts `a == a` always true (not `NaN`) |
| `Serialize` | (no equivalent) | Serde serialization to JSON/TOML/etc. |
| `Deserialize` | (no equivalent) | Serde deserialization from JSON/TOML/etc. |
| `Default` | Default constructor | `Type::default()` returns sensible defaults |

You can also implement traits manually when the auto-generated version isn't right:

```rust
// crates/fractal-core/src/session.rs:46-62
impl Default for SavedSession {
    fn default() -> Self {
        Self {
            version: "1".to_string(),
            timestamp: String::new(),
            name: String::new(),
            fractal_params: FractalParams::default(),
            // ...
        }
    }
}
```

### 2.5 Error Handling — Result, Option, and the ? Operator

Rust has no exceptions. Errors are return values.

```
C++                                 Rust
──────────────────────────────────  ──────────────────────────────────
throw runtime_error("no adapter");  return Err(RenderError::NoAdapter);
try { ... } catch (...) { ... }     match result { Ok(v) => ..., Err(e) => ... }
T* maybe_null;                      Option<T>  // Some(value) or None
```

**`Result<T, E>`** is either `Ok(value)` or `Err(error)`. The `?` operator propagates errors up the call stack — like exceptions but explicit:

```rust
// crates/fractal-renderer/src/context.rs:34-58
pub async fn new(window: Arc<Window>) -> Result<Self, RenderError> {
    let surface = instance
        .create_surface(window)
        .map_err(|e| RenderError::SurfaceCreation(e.to_string()))?;  // ? propagates

    let adapter = instance
        .request_adapter(&options)
        .await
        .ok_or(RenderError::NoAdapter)?;  // Convert Option to Result, then propagate

    let (device, queue) = adapter
        .request_device(&descriptor, None)
        .await?;  // RenderError::DeviceRequest via #[from] auto-conversion

    Ok(Self { instance, adapter, device, queue, surface, config, format })
}
```

The `?` operator is syntactic sugar for "if this is `Err`, return it; otherwise unwrap the `Ok` value." It replaces 90% of C++ try/catch blocks.

### 2.6 Custom Error Types with thiserror

The `thiserror` crate generates `Display`, `Error`, and `From` implementations:

```rust
// crates/fractal-renderer/src/context.rs:9-19
#[derive(Error, Debug)]
pub enum RenderError {
    #[error("Failed to create surface: {0}")]
    SurfaceCreation(String),

    #[error("Failed to find suitable adapter")]
    NoAdapter,

    #[error("Failed to request device: {0}")]
    DeviceRequest(#[from] wgpu::RequestDeviceError),

    #[error("Surface error: {0}")]
    Surface(#[from] wgpu::SurfaceError),
}
```

- `#[error("...")]` — Generates the `Display` impl (the human-readable message)
- `#[from]` — Generates `impl From<wgpu::RequestDeviceError> for RenderError`, enabling the `?` operator to auto-convert

You can also define manual `From` impls when you want more control:

```rust
// crates/fractal-app/src/session_manager.rs:29-39
impl From<std::io::Error> for SessionError {
    fn from(e: std::io::Error) -> Self { SessionError::Io(e) }
}
impl From<serde_json::Error> for SessionError {
    fn from(e: serde_json::Error) -> Self { SessionError::Json(e) }
}
```

And a type alias to reduce boilerplate:

```rust
// crates/fractal-app/src/session_manager.rs:41
pub type Result<T> = std::result::Result<T, SessionError>;
```

> **Tip**: Use `thiserror` for library error types (structured, typed). Use `anyhow` for application-level "just give me the error message" handling. This project uses both patterns — `thiserror` in the renderer, manual error enums in the session manager.

### 2.7 Option — Nullable Without Null

Rust has no null pointers. Instead, `Option<T>` is either `Some(value)` or `None`:

```rust
// crates/fractal-app/src/input.rs:29-30
pub prev_pinch_distance: Option<f32>,
pub prev_pinch_midpoint: Option<(f32, f32)>,
```

You must handle both cases before accessing the value:

```rust
if let Some(prev_dist) = self.prev_pinch_distance {
    let zoom_delta = current_dist / prev_dist;
    self.camera.zoom(zoom_delta);
}
```

The compiler won't let you accidentally use a `None` value — no null pointer dereferences, ever.

> **Gotcha**: `.unwrap()` on a `None` panics (like a null dereference crash). Use `.unwrap()` only when you can **prove** the value is `Some`. Prefer `if let`, `match`, or `.unwrap_or_default()`.

---

## Chapter 3: Rust Project Structure — Workspace & Modules

This chapter maps Rust's project organization to the C++ build systems you already know.

### 3.1 Cargo Workspace — Like CMake Targets

A Cargo **workspace** groups multiple crates (libraries/binaries) into one project. It's the equivalent of a CMake project with multiple `add_library()` / `add_executable()` targets.

```toml
# Cargo.toml (workspace root)
[workspace]
resolver = "2"
members = [
    "crates/fractal-core",
    "crates/fractal-renderer",
    "crates/fractal-ui",
    "crates/fractal-app",
]
default-members = ["crates/fractal-app"]
```

The four crates form a clean dependency hierarchy:

```
fractal-core          Pure math/types (no GPU, no UI)
    ↓
fractal-renderer      GPU rendering (depends on core)
    ↓
fractal-ui            egui panels (depends on core)
    ↓
fractal-app           Orchestration (depends on all three)
```

> **Tip**: Each crate is an independent compilation unit. If you change `fractal-ui`, only `fractal-ui` and `fractal-app` recompile — `fractal-core` and `fractal-renderer` are untouched. This is why Rust incremental builds can be fast even for large projects.

> **Gotcha**: Circular dependencies are a compile error. If `fractal-renderer` depends on `fractal-ui` and `fractal-ui` depends on `fractal-renderer`, Cargo refuses. This forces clean architecture — a feature, not a limitation.

### 3.2 Shared Dependencies

The workspace root declares shared dependency versions so all crates use the same version:

```toml
# Cargo.toml (workspace root)
[workspace.dependencies]
wgpu = "24"
egui = "0.31"
glam = { version = "0.29", features = ["bytemuck", "serde"] }
serde = { version = "1", features = ["derive"] }
bytemuck = { version = "1.21", features = ["derive"] }
```

Each crate then refers to these by name:

```toml
# crates/fractal-renderer/Cargo.toml
[dependencies]
wgpu.workspace = true
bytemuck.workspace = true
fractal-core.workspace = true
```

This is like CMake's `find_package()` or Conan dependency management, but built into the toolchain.

### 3.3 Module System — Files Are Modules

In C++ you have headers (`.h`) and source files (`.cpp`). In Rust, **every file is a module** and visibility is explicit:

```
C++                                 Rust
──────────────────────────────────  ──────────────────────────────────
#include "camera.h"                 mod camera;        // Declares the module
namespace fractal_core { ... }      // File: src/camera.rs  (implicit)
```

The root of each crate is `lib.rs` (library) or `main.rs` (binary). It declares which modules exist:

```rust
// crates/fractal-core/src/lib.rs
pub mod benchmark_types;
pub mod camera;
pub mod fractals;
pub mod mesh;
pub mod sdf;
pub mod session;

// Re-export commonly used types at crate root
pub use camera::Camera;
pub use fractals::{FractalParams, FractalType};
pub use session::SavedSession;
```

The `pub use` re-exports let consumers write `fractal_core::Camera` instead of `fractal_core::camera::Camera`. This is like C++ `using` declarations in a namespace header.

### 3.4 Visibility — Explicit Access Control

```
C++                     Rust
──────────────────────  ──────────────────────────
public:                 pub           // Visible to everyone
protected:              pub(crate)    // Visible within this crate only
private:                (default)     // Visible within this module only
friend class Foo;       pub(super)    // Visible to parent module
```

**In this project**, the pipeline's format field is private — only `FractalPipeline` methods can access it:

```rust
// crates/fractal-renderer/src/pipeline.rs:21-28
pub struct FractalPipeline {
    pub render_pipeline: wgpu::RenderPipeline,  // Public
    pub uniform_buffer: wgpu::Buffer,           // Public
    pub uniform_bind_group: wgpu::BindGroup,    // Public
    pub uniforms: Uniforms,                     // Public
    format: wgpu::TextureFormat,                // Private — implementation detail
}
```

### 3.5 Submodules — Directories as Module Trees

When a module grows large, you split it into a directory. The directory needs a `mod.rs` file that acts as the module root:

```
crates/fractal-ui/src/panels/
├── mod.rs                  ← Module root (declares sub-modules)
├── benchmark_panel.rs
├── camera_controls.rs
├── color_settings.rs
├── control_settings_panel.rs
├── export_panel.rs
├── fractal_params.rs
└── session_panel.rs
```

```rust
// crates/fractal-ui/src/panels/mod.rs
mod benchmark_panel;
mod fractal_params;
mod camera_controls;
mod color_settings;
mod control_settings_panel;
mod export_panel;
mod session_panel;

pub use benchmark_panel::BenchmarkPanel;
pub use fractal_params::FractalParamsPanel;
pub use camera_controls::CameraControlsPanel;
// ...
```

Private `mod` declarations + public `pub use` re-exports is a very common pattern. The sub-modules are implementation details; consumers only see the re-exported types.

### 3.6 Feature Flags — Compile-Time Feature Toggles

Rust uses **feature flags** instead of C++ `#ifdef`:

```toml
# crates/fractal-app/Cargo.toml
[features]
hot-reload = []
snapshot-tests = []
benchmark = []
```

Code checks features with `#[cfg(feature = "...")]`:

```rust
// crates/fractal-renderer/src/pipeline.rs:56-67
fn resolve_shader_source() -> String {
    #[cfg(feature = "hot-reload")]
    {
        if let Some(paths) = Self::shader_paths() {
            if let (Ok(common), Ok(render)) = (
                std::fs::read_to_string(&paths.0),
                std::fs::read_to_string(&paths.1),
            ) {
                return format!("{common}\n{render}");
            }
        }
    }
    format!("{SDF_COMMON}\n{RAYMARCHER}")
}
```

Usage: `cargo run --features hot-reload`

Feature-gated code is completely **removed** from the binary when the feature is disabled — zero runtime cost.

### 3.7 Conditional Compilation for Platforms

Platform targeting uses `#[cfg(...)]`, analogous to C++ `#ifdef _WIN32` / `#ifdef __ANDROID__`:

```rust
// crates/fractal-app/src/log_capture.rs:67-79
// Forward to platform console
#[cfg(target_arch = "wasm32")]
{
    let msg = format!("[{}] {} - {}", record.level(), record.target(), record.args());
    match record.level() {
        log::Level::Error => web_sys::console::error_1(&msg.into()),
        log::Level::Warn => web_sys::console::warn_1(&msg.into()),
        _ => web_sys::console::log_1(&msg.into()),
    }
}
#[cfg(not(target_arch = "wasm32"))]
{
    eprintln!("[{}] {} - {}", record.level(), record.target(), record.args());
}
```

Common predicates:
- `#[cfg(target_arch = "wasm32")]` — WebAssembly
- `#[cfg(target_os = "android")]` — Android
- `#[cfg(not(target_arch = "wasm32"))]` — Desktop + Android (everything except web)

> **Tip**: Unlike C++ `#ifdef`, Rust `#[cfg]` is checked by the compiler with full type information. Typos in feature names are warnings, not silent no-ops.

### 3.8 Build Profiles — Release Optimization

```toml
# Cargo.toml
[profile.release]
lto = true           # Link-Time Optimization (like -flto)
codegen-units = 1    # Single codegen unit for maximum optimization
opt-level = 3        # Equivalent to -O3

[profile.dev]
opt-level = 1        # Some optimization even in debug (faster GPU code)

[profile.dev.package."*"]
opt-level = 3        # Dependencies compiled with -O3 even in debug
```

That last section is important: it means wgpu, egui, and other dependencies are fully optimized even during development, while your code gets fast debug builds. This is a common Rust trick for GPU applications.

### 3.9 Compile-Time Embedding with include_str! / include_bytes!

Rust can embed files directly into the binary at compile time:

```rust
// crates/fractal-renderer/src/pipeline.rs:10-13
const SDF_COMMON: &str = include_str!("../shaders/sdf_common.wgsl");
const RAYMARCHER: &str = include_str!("../shaders/raymarcher.wgsl");
```

This is like C++ `xxd -i` or CMake's `configure_file()` but built into the language. The shader source is baked into the binary — no file I/O needed at runtime. The hot-reload feature optionally overrides this with disk reads during development.

---

## Chapter 4: wgpu Initialization — From Instance to Pixels

wgpu is Rust's cross-platform GPU abstraction. It maps directly to Vulkan, Metal, DirectX 12, and WebGPU — one API for all of them. If you've used D3D12 or Vulkan, the concepts are familiar but much less verbose.

### 4.1 Concept Mapping

Before diving into code, here's how wgpu maps to APIs you already know:

| wgpu | D3D12 | Vulkan | OpenGL |
|------|-------|--------|--------|
| `Instance` | `IDXGIFactory` | `VkInstance` | (implicit) |
| `Adapter` | `IDXGIAdapter` | `VkPhysicalDevice` | (implicit) |
| `Device` | `ID3D12Device` | `VkDevice` | Context |
| `Queue` | `ID3D12CommandQueue` | `VkQueue` | (implicit) |
| `Surface` | `IDXGISwapChain` | `VkSurfaceKHR` + `VkSwapchainKHR` | Window surface |
| `CommandEncoder` | `ID3D12GraphicsCommandList` | `VkCommandBuffer` | (immediate) |
| `RenderPipeline` | `ID3D12PipelineState` | `VkPipeline` | Shader program + state |
| `BindGroup` | Descriptor table | `VkDescriptorSet` | Uniform bindings |
| `Buffer` | `ID3D12Resource` | `VkBuffer` | Buffer object |
| `Texture` | `ID3D12Resource` | `VkImage` | Texture object |

The biggest simplification: wgpu's `Surface` replaces the entire swapchain lifecycle. No manual swapchain creation, no recreation on resize — just call `surface.configure()`.

### 4.2 The Initialization Pipeline

All initialization lives in `crates/fractal-renderer/src/context.rs`. The `RenderContext` struct holds everything:

```rust
// crates/fractal-renderer/src/context.rs:22-30
pub struct RenderContext {
    pub instance: Instance,
    pub adapter: Adapter,
    pub device: Device,
    pub queue: Queue,
    pub surface: Surface<'static>,
    pub config: SurfaceConfiguration,
    pub format: TextureFormat,
}
```

The initialization follows a strict pipeline — each step depends on the previous:

**Step 1: Create Instance** (context.rs:40-43)
```rust
let instance = Instance::new(&wgpu::InstanceDescriptor {
    backends: wgpu::Backends::all(),
    ..Default::default()
});
```

`Backends::all()` means: try Vulkan on Windows/Linux, Metal on macOS, DX12 on Windows, WebGPU on browsers. wgpu picks the best available. This is not a runtime abstraction — the Naga compiler cross-compiles shaders at pipeline creation time.

**Step 2: Create Surface** (context.rs:46-48)
```rust
let surface = instance
    .create_surface(window)
    .map_err(|e| RenderError::SurfaceCreation(e.to_string()))?;
```

The surface wraps the platform's native window handle. On Windows this is an HWND, on macOS a CAMetalLayer, on web a `<canvas>` element.

**Step 3: Request Adapter** (context.rs:51-58)
```rust
let adapter = instance
    .request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        compatible_surface: Some(&surface),
        force_fallback_adapter: false,
    })
    .await
    .ok_or(RenderError::NoAdapter)?;
```

`HighPerformance` picks the discrete GPU on laptops with hybrid graphics. `compatible_surface` ensures the adapter can present to this window. Note the `.await` — this is one of the few async operations in wgpu.

**Step 4: Request Device and Queue** (context.rs:66-73)
```rust
let (device, queue) = adapter
    .request_device(&wgpu::DeviceDescriptor {
        label: Some("Fractal Device"),
        required_features: wgpu::Features::empty(),
        required_limits: adapter.limits(),
        memory_hints: wgpu::MemoryHints::Performance,
    }, None)
    .await?;
```

> **Gotcha**: The `required_limits: adapter.limits()` line is critical. The default `Limits::default()` assumes desktop-class hardware. On a Raspberry Pi 4B's VideoCore VI, the default requests 8 color attachments but the GPU only supports 4, causing a panic. Always use `adapter.limits()` unless you need a specific minimum.

**Step 5: Configure Surface** (context.rs:75-96)
```rust
let surface_caps = surface.get_capabilities(&adapter);
let format = surface_caps
    .formats
    .iter()
    .find(|f| !f.is_srgb())
    .copied()
    .unwrap_or(surface_caps.formats[0]);

let config = SurfaceConfiguration {
    usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
    format,
    width: size.width.max(1),
    height: size.height.max(1),
    present_mode: wgpu::PresentMode::AutoVsync,
    alpha_mode: surface_caps.alpha_modes[0],
    view_formats: vec![],
    desired_maximum_frame_latency: 2,
};
surface.configure(&device, &config);
```

Key decisions explained:

- **Non-sRGB format**: The code explicitly finds a non-sRGB format (`!f.is_srgb()`). This is because egui expects a linear framebuffer and handles gamma correction internally. Using an sRGB format causes washed-out colors and validation warnings.

- **`AutoVsync`**: Lets the driver pick the best VSync strategy for the display. You can switch to `Fifo` (always VSync) or `Immediate` (no VSync) at runtime.

- **`desired_maximum_frame_latency: 2`**: Allows 2 frames in flight. Higher values increase throughput but add input latency. 2 is a good default.

- **`.max(1)`**: Guards against zero dimensions (window minimized). wgpu panics on 0×0 surfaces.

### 4.3 Resize Handling

When the window resizes, you just reconfigure the surface — no swapchain recreation dance:

```rust
// crates/fractal-renderer/src/context.rs:116-122
pub fn resize(&mut self, width: u32, height: u32) {
    if width > 0 && height > 0 {
        self.config.width = width;
        self.config.height = height;
        self.surface.configure(&self.device, &self.config);
    }
}
```

> **Gotcha**: Always check for zero dimensions before configuring. A minimized window on Windows reports 0×0 size, and wgpu panics on zero-sized surfaces.

Compare this to D3D12 where you'd need to:
1. Wait for the GPU to finish
2. Release all swapchain buffer references
3. Call `IDXGISwapChain::ResizeBuffers()`
4. Re-acquire buffer references

wgpu handles all of that inside `surface.configure()`.

### 4.4 Surface Error Recovery

During rendering, the surface can become "lost" or "outdated" (e.g., the window was minimized or the display configuration changed):

```rust
// crates/fractal-app/src/app.rs:425-435
if let Err(e) = self.render() {
    match e {
        wgpu::SurfaceError::Outdated | wgpu::SurfaceError::Lost => {
            let size = self.window.inner_size();
            if size.width > 0 && size.height > 0 {
                self.render_ctx.resize(size.width, size.height);
            }
        }
        _ => log::error!("Render error: {}", e),
    }
}
```

This is much simpler than D3D12's `DXGI_ERROR_DEVICE_REMOVED` handling. A single `resize()` call recovers from most surface errors.

### 4.5 The async Pattern

wgpu's initialization is `async` because adapter and device requests may involve IPC with the GPU driver:

```rust
// crates/fractal-renderer/src/context.rs:34
pub async fn new(window: Arc<Window>) -> Result<Self, RenderError> { ... }
```

The app handles this differently per platform:

```rust
// Native (crates/fractal-app/src/main.rs:64):
pollster::block_on(App::new(window, None, log_entries))

// WASM:
wasm_bindgen_futures::spawn_local(async move { App::new(...).await })
```

`pollster::block_on()` is a minimal async runtime that just blocks until the future completes. On WASM, you can't block the browser's main thread, so `spawn_local()` schedules the future on the JavaScript event loop.

> **Tip**: If you're not doing anything else async, `pollster` is the simplest executor. You don't need the full `tokio` runtime just for wgpu initialization.

---

## Chapter 5: The Render Pipeline — Shaders, Bind Groups & Drawing

This chapter covers how the fractal viewer sets up its GPU pipeline, from shader compilation through draw calls. If you've set up a D3D12 PSO or a Vulkan pipeline, this will feel familiar — but significantly less verbose.

### 5.1 Pipeline Architecture Overview

The rendering pipeline has three layers of objects, each built from the previous:

```
BindGroupLayout   →  PipelineLayout   →  RenderPipeline
(what resources        (what bind        (complete GPU state:
 can be bound)          groups exist)     shaders + layout + fixed-function)
```

In D3D12 terms: `BindGroupLayout` = root parameter, `PipelineLayout` = root signature, `RenderPipeline` = PSO.

All of these are **immutable after creation** — you create them once and use them every frame. To change shaders, you create a new pipeline (which is what hot-reload does).

### 5.2 Shader Loading — include_str! and Concatenation

The project uses two WGSL files that are concatenated at load time:

```rust
// crates/fractal-renderer/src/pipeline.rs:10-13
const SDF_COMMON: &str = include_str!("../shaders/sdf_common.wgsl");
const RAYMARCHER: &str = include_str!("../shaders/raymarcher.wgsl");
```

- `sdf_common.wgsl` — Uniforms struct, SDF functions (Mandelbulb, Menger, etc.), `map()` dispatcher
- `raymarcher.wgsl` — Vertex shader, ray march loop, lighting, fragment shader

They're concatenated into a single shader module:

```rust
// crates/fractal-renderer/src/pipeline.rs:68
format!("{SDF_COMMON}\n{RAYMARCHER}")
```

> **Gotcha**: WGSL has no `#include` directive. This concatenation is the project's workaround. The compute pipeline reuses `sdf_common.wgsl` by concatenating it with a different file (`sdf_volume.wgsl`), which is why the SDF functions are shared.

wgpu compiles the WGSL to the native shader format using the Naga compiler:

```rust
// crates/fractal-renderer/src/pipeline.rs:103-106
let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
    label: Some("Fractal Shader"),
    source: wgpu::ShaderSource::Wgsl(wgsl_source.into()),
});
```

Naga translates:
- **Vulkan**: WGSL → SPIR-V
- **Metal**: WGSL → MSL
- **DirectX 12**: WGSL → HLSL → DXIL
- **WebGPU**: WGSL passes through directly

This happens once at pipeline creation, not per-frame. Performance is identical to hand-written SPIR-V.

### 5.3 Uniform Buffer Creation

The uniform buffer holds all parameters the shader needs (camera, fractal config, lighting, colors):

```rust
// crates/fractal-renderer/src/pipeline.rs:109-114
let uniforms = Uniforms::new();
let uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
    label: Some("Uniform Buffer"),
    contents: bytemuck::cast_slice(&[uniforms]),
    usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
});
```

- `UNIFORM` — Can be bound as a uniform buffer in shaders
- `COPY_DST` — Can be written to from the CPU via `queue.write_buffer()`
- `bytemuck::cast_slice()` — Safely reinterprets the Rust struct as raw bytes (covered in detail in Chapter 6)

### 5.4 Bind Group Layout and Bind Group

A **bind group layout** describes what resources can be bound. A **bind group** is the actual binding of specific resources.

```rust
// crates/fractal-renderer/src/pipeline.rs:117-129
let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
    label: Some("Uniform Bind Group Layout"),
    entries: &[wgpu::BindGroupLayoutEntry {
        binding: 0,
        visibility: wgpu::ShaderStages::FRAGMENT,  // Only fragment shader reads uniforms
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }],
});
```

This says: "binding 0 is a uniform buffer visible to the fragment shader." The vertex shader doesn't need uniforms because it generates fullscreen geometry procedurally (see 5.6).

The bind group connects the layout to the actual buffer:

```rust
// crates/fractal-renderer/src/pipeline.rs:132-139
let uniform_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
    label: Some("Uniform Bind Group"),
    layout: &bind_group_layout,
    entries: &[wgpu::BindGroupEntry {
        binding: 0,
        resource: uniform_buffer.as_entire_binding(),
    }],
});
```

In D3D12 terms, the layout is the root parameter definition, and the bind group is the descriptor table pointing to the CBV.

### 5.5 Render Pipeline Creation

The render pipeline combines shaders, vertex layout, and fixed-function state into a single immutable object:

```rust
// crates/fractal-renderer/src/pipeline.rs:166-203
device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
    label: Some("Fractal Render Pipeline"),
    layout: Some(pipeline_layout),
    vertex: wgpu::VertexState {
        module: shader,
        entry_point: Some("vs_main"),
        buffers: &[],                    // No vertex buffers! (see 5.6)
        compilation_options: Default::default(),
    },
    fragment: Some(wgpu::FragmentState {
        module: shader,
        entry_point: Some("fs_main"),
        targets: &[Some(wgpu::ColorTargetState {
            format,
            blend: Some(wgpu::BlendState::REPLACE),  // Overwrite, no blending
            write_mask: wgpu::ColorWrites::ALL,
        })],
        compilation_options: Default::default(),
    }),
    primitive: wgpu::PrimitiveState {
        topology: wgpu::PrimitiveTopology::TriangleList,
        front_face: wgpu::FrontFace::Ccw,
        cull_mode: None,        // No backface culling (fullscreen geometry)
        polygon_mode: wgpu::PolygonMode::Fill,
        ..Default::default()
    },
    depth_stencil: None,        // No depth buffer (screen-space ray marcher)
    multisample: wgpu::MultisampleState {
        count: 1,               // No hardware MSAA (AA done in shader via RGSS)
        mask: !0,
        alpha_to_coverage_enabled: false,
    },
    multiview: None,
    cache: None,
})
```

Key design decisions:
- **No vertex buffers** (`buffers: &[]`) — The fullscreen triangle is generated procedurally in the vertex shader
- **No depth buffer** — Ray marching handles depth in the fragment shader
- **No hardware MSAA** (`count: 1`) — Anti-aliasing is done in the fragment shader using rotated grid supersampling (RGSS)
- **No backface culling** — The fullscreen triangle only has one face

### 5.6 The Fullscreen Triangle Trick

Instead of drawing a quad (2 triangles, 6 vertices, vertex buffer), the shader generates a single oversized triangle from just 3 vertices using `vertex_index`:

```wgsl
// crates/fractal-renderer/shaders/raymarcher.wgsl:18-32
@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    var out: VertexOutput;

    // Triangle vertices: (-1,-1), (3,-1), (-1,3)
    // This covers the entire [-1,1] x [-1,1] clip space
    let x = f32(i32(vertex_index & 1u) * 4 - 1);
    let y = f32(i32(vertex_index >> 1u) * 4 - 1);

    out.position = vec4<f32>(x, y, 0.0, 1.0);
    out.uv = vec2<f32>((x + 1.0) * 0.5, (1.0 - y) * 0.5);

    return out;
}
```

The three vertices are at `(-1,-1)`, `(3,-1)`, and `(-1,3)`. The triangle extends beyond the viewport but the rasterizer clips it. The UV coordinates map `[0,0]` to the top-left and `[1,1]` to the bottom-right.

> **Tip**: A fullscreen triangle is slightly more efficient than a fullscreen quad because there's no diagonal edge down the middle that causes some fragments to be shaded twice at the seam. It also avoids needing a vertex buffer at all.

### 5.7 The Render Pass

Each frame, a render pass writes to the swapchain texture:

```rust
// crates/fractal-renderer/src/pipeline.rs:258-281
pub fn render(&self, encoder: &mut wgpu::CommandEncoder, view: &wgpu::TextureView) {
    let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("Fractal Render Pass"),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view,
            resolve_target: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                store: wgpu::StoreOp::Store,
            },
        })],
        depth_stencil_attachment: None,
        timestamp_writes: None,
        occlusion_query_set: None,
    });

    render_pass.set_pipeline(&self.render_pipeline);
    render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
    render_pass.draw(0..3, 0..1);  // 3 vertices, 1 instance = fullscreen triangle
}
```

Three draw commands:
1. `set_pipeline` — Bind the PSO
2. `set_bind_group` — Bind the uniform buffer at group 0
3. `draw(0..3, 0..1)` — Draw 3 vertices (the fullscreen triangle)

> **Gotcha**: `LoadOp::Clear(BLACK)` on every frame is important. Swapchain textures are **uninitialized** — they may contain data from previous frames or arbitrary garbage (often white). Always clear until you're sure all swapchain buffers have been rendered to at least once.

### 5.8 Hot-Reload — Shader Iteration Without Restart

The pipeline supports replacing shaders at runtime:

```rust
// crates/fractal-renderer/src/pipeline.rs:208-250
pub fn reload_shader(&mut self, device: &wgpu::Device, wgsl_source: &str) -> Result<(), String> {
    // Use error scope to catch compile errors without crashing
    device.push_error_scope(wgpu::ErrorFilter::Validation);

    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("Fractal Shader (Hot Reload)"),
        source: wgpu::ShaderSource::Wgsl(wgsl_source.into()),
    });

    let error = pollster::block_on(device.pop_error_scope());
    if let Some(err) = error {
        return Err(format!("Shader compile error: {err}"));
    }

    // Success — rebuild pipeline with new shader
    self.render_pipeline = Self::create_render_pipeline(device, self.format, &shader, &pipeline_layout);
    Ok(())
}
```

If the shader has a syntax error, the old pipeline continues rendering — the app doesn't crash. This uses wgpu's error scope mechanism: push a scope, attempt the operation, pop the scope to check for errors.

> **Tip**: Enable hot-reload with `cargo run --features hot-reload` during development. Edit the WGSL files, save, and see changes in ~500ms without restarting. This is invaluable for shader development.

---

## Chapter 6: CPU-GPU Data Flow — Uniforms & bytemuck

This chapter covers the most error-prone part of GPU programming: ensuring the CPU struct layout matches the GPU struct layout byte-for-byte. Rust's type system and bytemuck make this safer than C++, but the alignment rules still bite.

### 6.1 The Problem — Struct Layout Must Match

The GPU reads uniform data as raw bytes at fixed offsets. If the Rust struct has different padding or field order than the WGSL struct, you get corrupted rendering — not an error, just wrong data.

In C++ you'd use `#pragma pack` or manual offsetof assertions. Rust provides better tools.

### 6.2 repr(C) — Predictable Memory Layout

By default, Rust reorders struct fields for optimal size. `#[repr(C)]` forces C-compatible layout:

```rust
// crates/fractal-renderer/src/uniforms.rs:17-18
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct Uniforms {
    // Camera (48 bytes at offset 0)
    pub camera_pos: [f32; 4],      // 16 bytes, offset 0
    pub camera_target: [f32; 4],   // 16 bytes, offset 16
    pub camera_up: [f32; 4],       // 16 bytes, offset 32

    // Camera params (16 bytes at offset 48)
    pub camera_fov: f32,           // 4 bytes, offset 48
    pub aspect_ratio: f32,         // 4 bytes, offset 52
    pub _pad1: [f32; 2],           // 8 bytes, offset 56

    // Resolution and time (16 bytes at offset 64)
    pub resolution: [f32; 2],      // 8 bytes, offset 64
    pub time: f32,                 // 4 bytes, offset 72
    pub _pad2: f32,                // 4 bytes, offset 76

    // Fractal parameters (16 bytes at offset 80)
    pub fractal_type: u32,         // 4 bytes, offset 80
    pub power: f32,                // 4 bytes, offset 84
    pub iterations: u32,           // 4 bytes, offset 88
    pub bailout: f32,              // 4 bytes, offset 92
    // ... continues for 512 bytes total
}
```

> **Gotcha**: Without `#[repr(C)]`, Rust may reorder `camera_fov` and `aspect_ratio` and insert different padding. The result: your GPU reads `aspect_ratio` where it expects `camera_fov`. The rendering looks wrong but there's no error — this is incredibly hard to debug. Always use `#[repr(C)]` for GPU-shared structs.

### 6.3 Pod and Zeroable — Safe Byte Reinterpretation

bytemuck provides two derive macros that replace C-style casts:

- **`Zeroable`** — The struct can be safely initialized to all zeros (like `memset(0)`)
- **`Pod`** (Plain Old Data) — The struct can be safely reinterpreted as bytes and back

```rust
#[derive(Pod, Zeroable)]  // These enable bytemuck::cast_slice()
```

With these derives, you can safely convert the struct to bytes for GPU upload:

```rust
// crates/fractal-renderer/src/pipeline.rs:253-255
pub fn update_uniforms(&mut self, queue: &wgpu::Queue) {
    queue.write_buffer(&self.uniform_buffer, 0, bytemuck::cast_slice(&[self.uniforms]));
}
```

`bytemuck::cast_slice(&[self.uniforms])` converts `&[Uniforms]` to `&[u8]` — a zero-cost reinterpretation. This is equivalent to C++ `reinterpret_cast<const char*>(&uniforms)` but with compile-time safety: `Pod` derive fails if the struct contains references, bools, enums, or other non-Pod types.

> **Tip**: If you try to derive `Pod` on a struct with a `bool` field, it won't compile — `bool` has invalid bit patterns (only 0 and 1 are valid). Use `u32` for GPU booleans instead. This project uses `u32` for `fractal_type`, `lod_enabled`, etc.

### 6.4 WGSL Alignment Rules

WGSL has strict alignment requirements that differ from C:

| WGSL Type | Size | Alignment | Rust Equivalent |
|-----------|------|-----------|-----------------|
| `f32` | 4 bytes | 4 bytes | `f32` |
| `u32` | 4 bytes | 4 bytes | `u32` |
| `vec2<f32>` | 8 bytes | 8 bytes | `[f32; 2]` |
| `vec3<f32>` | 12 bytes | **16 bytes** | **DON'T USE IN STRUCTS** |
| `vec4<f32>` | 16 bytes | 16 bytes | `[f32; 4]` |

The critical trap: **`vec3<f32>` aligns to 16 bytes**, not 12. If you put a `vec3` followed by an `f32` in a WGSL struct, the `f32` sits at offset 16 (after the vec3's 16-byte alignment), not offset 12. This creates a 4-byte invisible gap.

This project avoids the trap entirely by using `vec4<f32>` for 3D vectors (with the 4th component as padding) or individual `f32` fields:

```wgsl
// crates/fractal-renderer/shaders/sdf_common.wgsl:10-16
struct Uniforms {
    camera_pos: vec4<f32>,      // vec4, not vec3 — 4th component unused
    camera_target: vec4<f32>,
    camera_up: vec4<f32>,
    camera_fov: f32,
    aspect_ratio: f32,
    _pad1: vec2<f32>,           // Explicit padding to 16-byte boundary
    // ...
}
```

> **Gotcha**: This is the #1 source of GPU uniform bugs. If you see "everything renders wrong after adding a field," check alignment. The WGSL spec's alignment rules are at: https://www.w3.org/TR/WGSL/#alignment-and-size

### 6.5 Manual Padding Fields

When fields don't naturally align to 16-byte boundaries, the project adds explicit padding:

```rust
// crates/fractal-renderer/src/uniforms.rs:25-28
pub camera_fov: f32,           // 4 bytes, offset 48
pub aspect_ratio: f32,         // 4 bytes, offset 52
pub _pad1: [f32; 2],           // 8 bytes, offset 56  ← explicit padding
```

Without `_pad1`, the next field (`resolution`) would be at offset 56 in Rust but the WGSL compiler might expect it at offset 64 (next 16-byte boundary for `vec2<f32>`). The padding ensures both sides agree.

> **Tip**: Document byte offsets in comments for every field, like this project does. When a bug appears, you can visually verify the layout matches. It's tedious but saves hours of debugging.

### 6.6 Compile-Time Size Assertion

The project statically asserts the struct is exactly the expected size:

```rust
// crates/fractal-renderer/src/uniforms.rs (after struct definition)
const _: () = assert!(std::mem::size_of::<Uniforms>() == 512);
```

If someone adds a field and forgets to update the padding, this fails at compile time — not at runtime with corrupted rendering.

In C++ you'd use `static_assert(sizeof(Uniforms) == 512)`. The Rust version is slightly more verbose but accomplishes the same thing.

### 6.7 The Update Pattern

Every frame, the app updates individual sections of the Uniforms struct, then flushes everything to the GPU in one call:

```rust
// crates/fractal-app/src/app.rs:838-846
self.pipeline.uniforms.update_camera(&self.camera, self.render_ctx.aspect_ratio());
self.pipeline.uniforms.update_resolution(width, height);
self.pipeline.uniforms.update_time(time);
self.pipeline.uniforms.update_fractal(&self.ui_state.fractal_params);
self.pipeline.uniforms.update_ray_march(&self.ui_state.ray_march_config);
self.pipeline.uniforms.update_lighting(&self.ui_state.lighting_config);
self.pipeline.uniforms.update_color(&self.ui_state.color_config);
self.pipeline.uniforms.frame_count = self.pipeline.uniforms.frame_count.wrapping_add(1);

// Single GPU upload after all CPU modifications
self.pipeline.update_uniforms(&self.render_ctx.queue);
```

This is efficient: modify CPU-side, then do one `queue.write_buffer()` call. No partial buffer updates, no mapping/unmapping. The entire 512 bytes are uploaded every frame.

> **Tip**: For a 512-byte uniform buffer, uploading the whole thing every frame is fine — it's a single DMA transfer. Partial updates via `write_buffer` with offsets only make sense for much larger buffers (10KB+).

### 6.8 Adding a New Uniform Field

To add a new field (e.g., `fog_density: f32`):

1. **Rust side** (`uniforms.rs`): Add the field in the reserved area, update padding, verify the size assertion still passes
2. **WGSL side** (`sdf_common.wgsl`): Add the matching field at the same byte offset
3. **Update method** (`uniforms.rs`): Add an `update_fog()` method
4. **App side** (`app.rs`): Call `update_fog()` before `update_uniforms()`

Both the `#[repr(C)]` layout and the WGSL struct must agree on every field's offset. The project reserves 68 bytes at the end for future expansion, so you don't need to change the total size for small additions.

---

## Chapter 7: WGSL Shader Deep Dive — Ray Marching a Fractal

WGSL (WebGPU Shading Language) is the shader language for wgpu. If you know GLSL or HLSL, you'll adapt quickly — WGSL is syntactically closer to HLSL but with Rust-like type safety.

### 7.1 WGSL vs GLSL vs HLSL — Syntax Comparison

| Concept | GLSL | HLSL | WGSL |
|---------|------|------|------|
| Variable (mutable) | `float x = 1.0;` | `float x = 1.0;` | `var x = 1.0;` |
| Variable (immutable) | `const float x = 1.0;` | `const float x = 1.0;` | `let x = 1.0;` |
| Function | `float foo(vec3 p)` | `float foo(float3 p)` | `fn foo(p: vec3<f32>) -> f32` |
| Ternary | `a ? b : c` | `a ? b : c` | `select(c, b, a)` |
| Entry point | `void main()` | `[numthreads]` | `@vertex fn vs_main(...)` |
| Builtins | `gl_VertexID` | `SV_VertexID` | `@builtin(vertex_index)` |
| Varying / inter-stage | `in/out` qualifier | Semantics (`TEXCOORD0`) | `@location(0)` |
| Uniform binding | `layout(binding=0)` | `register(b0)` | `@group(0) @binding(0)` |
| Vector types | `vec3`, `ivec2` | `float3`, `int2` | `vec3<f32>`, `vec2<i32>` |
| Matrix types | `mat4` | `float4x4` | `mat4x4<f32>` |
| Swizzle | `v.xyz` | `v.xyz` | `v.xyz` |
| Modulo | `mod(a, b)` | `fmod(a, b)` | `a % b` |

### 7.2 Key WGSL Syntax Differences

**`let` vs `var`**: `let` is immutable (like Rust's `let`), `var` is mutable (like Rust's `let mut`):

```wgsl
let max_steps = u.max_steps;     // Immutable — can't reassign
var t = u.near_clip;             // Mutable — t changes each iteration
```

**`select()` instead of ternary**: WGSL has no `?:` operator. Use `select(false_val, true_val, condition)`:

```wgsl
// crates/fractal-renderer/shaders/raymarcher.wgsl:97
let min_step = select(0.0, adaptive_epsilon * 0.2, u.lod_enabled != 0u);
```

> **Gotcha**: `select()` evaluates **both** arguments before choosing. Don't use it for bounds checks like `select(0.0, arr[i], i < len)` — the out-of-bounds access happens regardless of the condition. Use an `if` statement instead.

**Struct-based inter-stage data**: Instead of GLSL's `in/out` qualifiers, WGSL uses structs with `@location` attributes:

```wgsl
// crates/fractal-renderer/shaders/raymarcher.wgsl:12-15
struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
}
```

**Entry point decorators**: `@vertex`, `@fragment`, `@compute` replace GLSL's implicit `void main()`:

```wgsl
@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VertexOutput { ... }

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> { ... }
```

### 7.3 Uniform Access

Uniforms are declared as a global variable bound to a group and binding:

```wgsl
// crates/fractal-renderer/shaders/sdf_common.wgsl:102-103
@group(0) @binding(0)
var<uniform> u: Uniforms;
```

Access is straightforward — just use `u.field_name`:

```wgsl
let power = u.power;
let max_steps = u.max_steps;
let camera_pos = u.camera_pos.xyz;  // Swizzle vec4 down to vec3
```

### 7.4 Module-Level Mutable State

WGSL supports `var<private>` for per-invocation mutable state:

```wgsl
// crates/fractal-renderer/shaders/sdf_common.wgsl:107
var<private> effective_iterations: u32;
```

This is set by the ray marcher and read by SDF functions — it allows LOD to reduce iteration counts at far distances. It's like a thread-local global variable.

### 7.5 SDF Functions — The map() Dispatcher

Signed Distance Functions (SDFs) return the distance from a point to the nearest surface. The project has six SDF implementations dispatched by `map()`:

```wgsl
// crates/fractal-renderer/shaders/sdf_common.wgsl (map function)
fn map(pos: vec3<f32>) -> vec2<f32> {
    switch u.fractal_type {
        case 0u: { return sdf_mandelbulb(pos); }
        case 1u: { return sdf_menger(pos); }
        case 2u: { return sdf_julia(pos); }
        case 3u: { return sdf_mandelbox(pos); }
        case 4u: { return sdf_sierpinski(pos); }
        case 5u: { return sdf_apollonian(pos); }
        default: { return sdf_mandelbulb(pos); }
    }
}
```

The return type `vec2<f32>` encodes two values: `.x` is the distance, `.y` is the orbit trap (used for coloring).

**Example — Mandelbulb SDF** (sdf_common.wgsl:160-203):

```wgsl
fn sdf_mandelbulb(pos: vec3<f32>) -> vec2<f32> {
    var z = pos;
    var dr = 1.0;
    var r = 0.0;
    var trap = 1e10;

    let power = u.power;
    let iterations = effective_iterations;  // LOD-controlled!

    for (var i = 0u; i < iterations; i = i + 1u) {
        r = length(z);
        if (r > u.bailout) { break; }

        trap = min(trap, r);

        // Spherical coordinates → power → back to Cartesian
        let theta = acos(z.z / r);
        let phi = atan2(z.y, z.x);
        dr = pow(r, power - 1.0) * power * dr + 1.0;
        let zr = pow(r, power);
        z = zr * vec3<f32>(
            sin(theta * power) * cos(phi * power),
            sin(phi * power) * sin(theta * power),
            cos(theta * power)
        );
        z = z + pos;
    }

    let dist = 0.5 * log(r) * r / dr;
    return vec2<f32>(dist, trap);
}
```

### 7.6 The Ray March Loop

The core rendering algorithm: cast a ray from the camera, step forward by the SDF distance, repeat until hitting the surface or exceeding maximum distance:

```wgsl
// crates/fractal-renderer/shaders/raymarcher.wgsl:45-104
fn ray_march(ro: vec3<f32>, rd: vec3<f32>) -> RayMarchResult {
    var result: RayMarchResult;
    var t = u.near_clip;
    let epsilon = u.epsilon;

    // LOD: pixel angular size determines detail threshold
    let fov_factor = tan(u.camera_fov * 0.5);
    let pixel_angular_size = 2.0 * fov_factor / u.resolution.y;
    let lod_factor = f32(u.lod_enabled) * u.lod_scale * pixel_angular_size;

    for (var i = 0u; i < u.max_steps; i = i + 1u) {
        // LOD iteration reduction at far distances
        if (u.lod_enabled != 0u) {
            let pixel_footprint = t * pixel_angular_size;
            let lod_ratio = pixel_footprint * u.lod_scale / epsilon;
            let reduce = u32(clamp(log2(max(1.0, lod_ratio)), 0.0, f32(u.iterations) - 3.0));
            effective_iterations = u.iterations - reduce;
        }

        let pos = ro + rd * t;
        let res = map(pos);
        let d = res.x;

        // Adaptive epsilon grows with distance (LOD)
        let adaptive_epsilon = epsilon + t * lod_factor;

        if (d < adaptive_epsilon) {
            result.hit = true;
            result.distance = t;
            result.trap = res.y;
            return result;
        }

        if (t > u.max_distance) { break; }

        // Step forward (0.9× safety factor prevents overshooting)
        let min_step = select(0.0, adaptive_epsilon * 0.2, u.lod_enabled != 0u);
        t = t + max(d * 0.9, min_step);
    }
    return result;
}
```

The 0.9 safety factor is a common ray marching trick — stepping by the full SDF distance can overshoot the surface due to floating-point precision. The LOD system reduces both the hit threshold and the SDF iteration count at far distances, giving a significant performance boost.

### 7.7 Normal Calculation — Tetrahedron Technique

Surface normals are computed by sampling the SDF at four nearby points (tetrahedron technique — 4 evaluations instead of the 6 needed for finite differences):

```wgsl
// crates/fractal-renderer/shaders/raymarcher.wgsl:110-128
fn calc_normal(pos: vec3<f32>, t: f32) -> vec3<f32> {
    let h = max(max(u.normal_epsilon * t, lod_h), 1e-7);
    let k = vec2<f32>(1.0, -1.0);
    return normalize(
        k.xyy * map(pos + k.xyy * h).x +
        k.yyx * map(pos + k.yyx * h).x +
        k.yxy * map(pos + k.yxy * h).x +
        k.xxx * map(pos + k.xxx * h).x
    );
}
```

The `h` value (normal epsilon) scales with distance `t` — farther surfaces get coarser normals, preventing banding artifacts at far distances.

### 7.8 Double-Single Precision for Deep Zoom

For deep zoom (10^7+ magnification), `f32` precision runs out. The project emulates ~14 decimal digits using two `f32` values (Knuth's TwoSum algorithm):

```wgsl
// crates/fractal-renderer/shaders/sdf_common.wgsl:115-146
struct DS {
    hi: f32,
    lo: f32,
}

// Error-free addition: value = (a.hi + b.hi) + error
fn ds_add(a: DS, b: DS) -> DS {
    let s = a.hi + b.hi;
    let v = s - a.hi;
    let e = (a.hi - (s - v)) + (b.hi - v);
    let lo = e + a.lo + b.lo;
    let hi = s + lo;
    return DS(hi, lo - (hi - s));
}
```

This is a well-known technique in scientific computing — the "compensated summation" family of algorithms. It gives you `float64`-like precision on hardware that only supports `float32`.

### 7.9 WGSL Tips for GLSL/HLSL Developers

> **Tip**: WGSL is closer to HLSL than GLSL. If you know HLSL's `SV_VertexID`, `cbuffer`, and semantic syntax, WGSL will feel natural.

> **Gotcha**: No `#include` — WGSL has no preprocessor. The project works around this by concatenating WGSL files at load time in Rust. This is a common pattern in wgpu projects.

> **Gotcha**: No implicit type conversions. `1` is `i32`, `1u` is `u32`, `1.0` is `f32`. You must cast explicitly: `f32(my_uint)`, `u32(my_float)`.

> **Gotcha**: `for` loop syntax is C-style but with explicit types: `for (var i = 0u; i < count; i = i + 1u)`. Note `i = i + 1u`, not `i++` (there's no `++` operator in WGSL).

> **Tip**: The modulo operator `%` works on floats in WGSL (unlike C where you need `fmod()`). The Menger sponge uses this: `let a = (z * s % 2.0 + 2.0) % 2.0 - 1.0;`

---

## Chapter 8: egui Integration — Immediate-Mode UI on wgpu

egui is Rust's answer to Dear ImGui — an immediate-mode UI library. If you've used ImGui in C++, egui will feel very familiar. If you haven't, this chapter explains the paradigm from scratch.

### 8.1 Immediate Mode vs Retained Mode

**Retained mode** (Qt, WPF, HTML/DOM): You create widget objects, store them in a tree, and update properties. The framework renders and manages state.

**Immediate mode** (egui, Dear ImGui): You call functions every frame that both define and render the UI. There is no persistent widget tree — the UI is "rebuilt" each frame.

```
// Retained mode (pseudocode)
button = new Button("Click me");
button.on_click = handle_click;
panel.add(button);

// Immediate mode — this runs EVERY FRAME
if ui.button("Click me").clicked() {
    handle_click();
}
```

The immediate-mode approach is simpler for interactive tools: no state synchronization, no event callback spaghetti, no widget lifecycle management.

### 8.2 The Three Integration Crates

egui needs three crates to work with wgpu + winit:

| Crate | Purpose |
|-------|---------|
| `egui` | Core UI library — widgets, layout, painting commands |
| `egui-winit` | Bridges winit events → egui input (keyboard, mouse, DPI) |
| `egui-wgpu` | Renders egui paint commands using wgpu |

Setup (from `crates/fractal-app/src/app.rs:174-190`):

```rust
// 1. Create egui context (the core state machine)
let egui_ctx = egui::Context::default();

// 2. Create winit bridge (handles events, DPI, clipboard)
let egui_state = egui_winit::State::new(
    egui_ctx.clone(),
    egui::ViewportId::ROOT,
    &window,
    Some(window.scale_factor() as f32),
    None,  // max texture side
    None,  // egui options
);

// 3. Create wgpu renderer (turns paint commands into GPU draws)
let egui_renderer = egui_wgpu::Renderer::new(
    &render_ctx.device,
    render_ctx.format,  // Must match surface format
    None,               // No depth stencil
    1,                  // MSAA sample count
    false,              // No sRGB blending
);
```

> **Gotcha**: The `format` parameter to `egui_wgpu::Renderer` must match your surface format. If you use sRGB for the surface but pass it to egui, colors will be double-gamma-corrected (washed out). This project uses non-sRGB specifically to avoid this.

### 8.3 The Frame Cycle

Every frame, egui follows a three-step cycle:

```rust
// crates/fractal-app/src/app.rs:853-855

// Step 1: Gather input from winit
let raw_input = self.egui_state.take_egui_input(&self.window);

// Step 2: Run the UI — this is where all widget code executes
let full_output = self.egui_ctx.run(raw_input, |ctx| {
    FractalPanel::show(ctx, &mut self.ui_state);
    // ... debug overlay, log window, etc.
});

// Step 3: Process output — render shapes, handle platform events
// (textures delta, clipboard, cursor changes, etc.)
```

Inside the `ctx.run()` closure is where you write all your UI code. Every widget call both checks for interaction AND generates paint commands. The closure receives a mutable reference to your state, so widgets can modify it directly.

### 8.4 Input Routing — egui First, App Second

A critical pattern: egui gets first crack at all input events. If it consumes an event (e.g., a click on a slider), the app's camera controls don't process it:

```rust
// crates/fractal-app/src/app.rs:362-368
pub fn handle_window_event(&mut self, event: &WindowEvent, elwt: &ActiveEventLoop) {
    // Let egui handle events first
    let egui_response = self.egui_state.on_window_event(&self.window, event);

    if egui_response.consumed {
        return;  // egui handled it — don't orbit the camera
    }

    // App handles remaining events (camera orbit, zoom, etc.)
    match event { ... }
}
```

> **Gotcha**: If you skip this check, clicking a slider will also orbit the camera behind it. Always check `egui_response.consumed` before processing app-level input.

> **Gotcha**: `ui.ctx().wants_keyboard_input()` / `wants_pointer_input()` exist but can miss edge cases (e.g., clicking outside a text field to deselect it). The `on_window_event().consumed` pattern is more reliable.

### 8.5 Widget Tour — What's Available

Here's every egui widget used in this project, with real examples:

**ComboBox** — Dropdown selector:
```rust
// crates/fractal-ui/src/panels/fractal_params.rs:17-34
egui::ComboBox::from_label("")
    .selected_text(current_type.name())
    .show_ui(ui, |ui| {
        for fractal_type in FractalType::all() {
            if ui.selectable_value(
                &mut state.fractal_params.fractal_type,
                *fractal_type,
                fractal_type.name(),
            ).clicked() {
                state.set_fractal_type(*fractal_type);
                changed = true;
            }
        }
    });
```

**Slider** — Drag a value within a range:
```rust
// crates/fractal-ui/src/panels/fractal_params.rs:77
ui.add(ranges.power.slider(&mut params.power))
```

**DragValue** — Numeric input with drag or type:
```rust
ui.add(ranges.power.drag_value(&mut params.power))
```

**Checkbox** — Boolean toggle:
```rust
ui.checkbox(&mut state.show_debug, "Show debug info");
```

**Color picker** — Inline RGB color editor:
```rust
ui.color_edit_button_rgb(&mut color_array);
```

**CollapsingHeader** — Expandable section:
```rust
egui::CollapsingHeader::new("Fractal Type")
    .default_open(true)
    .show(ui, |ui| { /* section contents */ });
```

**ScrollArea** — Scrollable region:
```rust
egui::ScrollArea::vertical()
    .stick_to_bottom(true)  // Auto-scroll to bottom (for logs)
    .show(ui, |ui| { /* scrollable contents */ });
```

**Window** — Floating, draggable panel:
```rust
egui::Window::new("Debug")
    .anchor(egui::Align2::RIGHT_TOP, [-10.0, 10.0])
    .show(ctx, |ui| { /* window contents */ });
```

**Layout helpers**:
```rust
ui.horizontal(|ui| {       // Children arranged left-to-right
    ui.label("Power:");
    ui.add(slider);
});

ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
    if ui.button("X").clicked() { /* close */ }
});
```

### 8.6 Data-Driven Slider Ranges

Instead of hardcoding min/max values for every slider, the project uses data-driven ranges loaded from TOML:

```rust
// crates/fractal-ui/src/app_settings.rs:13-27
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(default)]
pub struct FloatRange {
    pub min: f32,
    pub max: f32,
    pub speed: Option<f32>,       // Drag speed (None = use slider)
    pub decimals: Option<usize>,  // Fixed decimal places (None = auto)
    pub logarithmic: bool,
}
```

The range struct has factory methods that create configured egui widgets:

```rust
// crates/fractal-ui/src/app_settings.rs:48-57
pub fn slider<'a>(&self, value: &'a mut f32) -> egui::Slider<'a> {
    let mut s = egui::Slider::new(value, self.min..=self.max);
    if self.logarithmic { s = s.logarithmic(true); }
    if let Some(d) = self.decimals { s = s.fixed_decimals(d); }
    s
}
```

Usage is clean — one line per control:

```rust
ui.add(ranges.power.slider(&mut params.power));
```

This pattern means slider ranges can be changed in a TOML config file without recompiling. The ranges are embedded at compile time from `default_app_settings.toml` and optionally overridden by a user config file at runtime.

### 8.7 The Response Pattern — Detecting Changes

Every egui widget returns a `Response` that tells you what happened:

```rust
if ui.add(ranges.power.slider(&mut params.power)).changed() {
    changed = true;
}
```

- `.clicked()` — Was this widget clicked this frame?
- `.changed()` — Did the value change this frame?
- `.hovered()` — Is the mouse over this widget?
- `.dragged()` — Is the widget being dragged?

> **Tip**: Use `.changed()` to trigger expensive updates only when the user actually modifies a value. Don't recompute everything every frame just because the UI ran.

### 8.8 Custom Painting — Gizmos and Graphs

egui provides a `Painter` API for custom drawing. The project uses it for:

**Axes gizmo** (bottom-right corner showing camera orientation):
```rust
// crates/fractal-app/src/app.rs (axes gizmo function)
let painter = ui.painter();
painter.circle_filled(center, radius, Color32::from_black_alpha(60));
painter.line_segment([center, x_end], Stroke::new(2.0, Color32::RED));
painter.text(x_end, Align2::CENTER_CENTER, "X", font, Color32::RED);
```

**Frame time graph** (in benchmark panel — color-coded performance visualization):
```rust
// Color-coded: green (<16.67ms), yellow (16-33ms), red (>33ms)
let color = if ms < 16.67 {
    Color32::GREEN
} else if ms < 33.33 {
    Color32::YELLOW
} else {
    Color32::RED
};
painter.line_segment([prev_point, point], Stroke::new(1.5, color));
```

### 8.9 Texture Management

Loading images into egui for display (used for session thumbnails):

```rust
let texture = egui_ctx.load_texture(
    "splash_bg",
    egui::ColorImage { size: [width, height], pixels },
    egui::TextureOptions::LINEAR,
);

// Display in a panel
ui.image(egui::load::SizedTexture::new(texture.id(), [80.0, 45.0]));
```

egui-wgpu handles the GPU texture upload and binding automatically. You just provide pixel data and get a handle back.

### 8.10 Panel Architecture

The project organizes UI into a hierarchy of panel structs, each with a static `show()` method:

```rust
// crates/fractal-ui/src/panels/mod.rs:24-31
pub struct FractalPanel;

impl FractalPanel {
    pub fn show(ctx: &Context, state: &mut UiState) -> bool {
        // Main window with all sub-panels
        egui::Window::new("fractal_panel")
            .title_bar(false)
            .movable(true)
            .show(ctx, |ui| {
                SessionPanel::show(ui, state);
                FractalParamsPanel::show(ui, state);
                ColorSettingsPanel::show(ui, state);
                CameraControlsPanel::show(ui, state);
                ExportPanel::show(ui, state);
                BenchmarkPanel::show(ui, state);
            });
    }
}
```

Each sub-panel is an independent struct with its own `show()` method. This gives clean separation without the overhead of retained-mode widget trees.

> **Tip**: egui panels are just functions that take `&mut Ui` and `&mut State`. There's no widget registration, no lifecycle, no signal/slot mechanism. If you want to add a new panel, just write a new `show()` function and call it.

---

## Chapter 9: Putting It All Together — The Application Loop

This chapter shows how winit, wgpu, and egui connect into a complete application loop. It covers the event loop, platform abstraction, and the full frame lifecycle.

### 9.1 winit 0.30 ApplicationHandler

winit 0.30 replaced the old closure-based event loop with a trait-based pattern. You implement `ApplicationHandler` on a struct:

```rust
// crates/fractal-app/src/main.rs:25-46
struct AppHandler {
    window_attrs: winit::window::WindowAttributes,
    app: Option<App>,
    window: Option<Arc<Window>>,
    log_entries: fractal_app::log_capture::LogBuffer,
}

// crates/fractal-app/src/main.rs:49-94
impl ApplicationHandler for AppHandler {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_none() {
            let window = Arc::new(
                event_loop.create_window(self.window_attrs.clone())
                    .expect("Failed to create window"),
            );
            self.window = Some(window.clone());

            match pollster::block_on(App::new(window, None, self.log_entries.clone())) {
                Ok(app) => self.app = Some(app),
                Err(e) => {
                    log::error!("Failed to create application: {}", e);
                    event_loop.exit();
                }
            }
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        if let Some(ref mut app) = self.app {
            app.handle_window_event(&event, event_loop);
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(ref window) = self.window {
            window.request_redraw();
        }
    }
}
```

Three key callbacks:
- **`resumed()`** — Called when the app can create windows. On desktop this happens once at startup. On Android it happens each time the app comes to the foreground.
- **`window_event()`** — All window events (input, resize, redraw, close).
- **`about_to_wait()`** — Called after all pending events are processed. This is where you request the next frame.

### 9.2 The Event Loop

```rust
// crates/fractal-app/src/main.rs:97-122
fn main() {
    let log_entries = fractal_app::log_capture::init(log::LevelFilter::Info);

    let event_loop = EventLoop::new().expect("Failed to create event loop");
    event_loop.set_control_flow(winit::event_loop::ControlFlow::Poll);

    let window_attrs = winit::window::WindowAttributes::default()
        .with_title("Modern Fractal Viewer")
        .with_inner_size(winit::dpi::LogicalSize::new(800u32, 450u32))
        .with_resizable(false);  // Resizable after splash

    let mut handler = AppHandler::new(window_attrs, log_entries);
    event_loop.run_app(&mut handler).expect("Event loop error");
}
```

`ControlFlow::Poll` means the event loop never sleeps — it continuously processes events and calls `about_to_wait()`, which requests a redraw, which triggers `RedrawRequested`. This gives the highest frame rate for real-time rendering.

> **Tip**: Use `ControlFlow::Wait` for editor-style apps that only need to redraw when something changes. `Poll` is for real-time rendering (games, visualizers).

### 9.3 The Complete Frame Lifecycle

Here's what happens each frame, in order:

```
1. about_to_wait()          → window.request_redraw()
2. WindowEvent::RedrawRequested
   ├── Initial configure    → resize surface on first frame (WASM fix)
   ├── Skip if minimized    → return if 0×0
   ├── update()             → camera animation, hot-reload polling
   └── render()
       ├── Acquire texture  → surface.get_current_texture()
       ├── Create encoder   → device.create_command_encoder()
       ├── Update uniforms  → batch CPU updates + write_buffer()
       ├── Fractal pass     → pipeline.render() (fullscreen triangle)
       ├── egui pass        → ctx.run() + renderer.render()
       ├── Submit           → queue.submit(encoder.finish())
       └── Present          → output.present()
```

The render method (from `crates/fractal-app/src/app.rs:796-846`):

```rust
fn render(&mut self) -> Result<(), wgpu::SurfaceError> {
    // 1. Acquire next swapchain texture
    let output = self.render_ctx.surface.get_current_texture()?;
    let view = output.texture.create_view(&Default::default());

    // 2. Create command encoder (like a D3D12 command list)
    let mut encoder = self.render_ctx.device.create_command_encoder(
        &wgpu::CommandEncoderDescriptor { label: Some("Render Encoder") }
    );

    // 3. Update all uniforms on CPU, then flush to GPU
    self.pipeline.uniforms.update_camera(&self.camera, self.render_ctx.aspect_ratio());
    self.pipeline.uniforms.update_resolution(width, height);
    self.pipeline.uniforms.update_time(time);
    self.pipeline.uniforms.update_fractal(&self.ui_state.fractal_params);
    self.pipeline.uniforms.update_ray_march(&self.ui_state.ray_march_config);
    self.pipeline.uniforms.update_lighting(&self.ui_state.lighting_config);
    self.pipeline.uniforms.update_color(&self.ui_state.color_config);
    self.pipeline.update_uniforms(&self.render_ctx.queue);

    // 4. Render fractal (fullscreen triangle)
    self.pipeline.render(&mut encoder, &view);

    // 5. Render egui on top
    let raw_input = self.egui_state.take_egui_input(&self.window);
    let full_output = self.egui_ctx.run(raw_input, |ctx| {
        FractalPanel::show(ctx, &mut self.ui_state);
    });
    // ... egui rendering with egui_renderer ...

    // 6. Submit and present
    self.render_ctx.queue.submit(std::iter::once(encoder.finish()));
    output.present();
    Ok(())
}
```

### 9.4 Platform Abstraction Pattern

The app runs on desktop, WASM, and Android with the same core logic. Platform differences are isolated via `#[cfg()]`:

**Time**:
```rust
// crates/fractal-app/src/app.rs:5-8
#[cfg(not(target_arch = "wasm32"))]
use std::time::Instant;
#[cfg(target_arch = "wasm32")]
use web_time::Instant;
```

**Async runtime**:
```rust
// Native: pollster::block_on(App::new(...))
// WASM:   wasm_bindgen_futures::spawn_local(async { App::new(...).await })
```

**Storage**:
```rust
// Native:  FileSystemStorage  → dirs::data_dir() / "ModernFractalViewer"
// Android: FileSystemStorage  → AndroidApp::internal_data_path()
// WASM:    LocalStorageBackend → web_sys::window().local_storage()
```

**Console logging**:
```rust
// Native:  eprintln!(...)
// WASM:    web_sys::console::log_1(...)
```

### 9.5 Session Persistence with Serde

The app saves and loads fractal exploration sessions as JSON:

```rust
// crates/fractal-core/src/session.rs:17-19
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SavedSession {
    pub version: String,
    pub fractal_params: FractalParams,
    pub ray_march_config: RayMarchConfig,
    pub lighting_config: LightingConfig,
    pub color_config: ColorConfig,
    pub camera: Camera,
    // ...
}
```

The `#[serde(default)]` attribute is critical for backward compatibility: if a saved JSON file is missing fields that were added in a newer version, those fields get their `Default` value instead of causing a parse error.

```rust
// Old save (v1): { "fractal_params": {...} }
// New code (v2): expects "lighting_config" field
// Without #[serde(default)]: ERROR — missing field
// With #[serde(default)]:    OK — uses LightingConfig::default()
```

> **Gotcha**: Every new field added to a serializable struct must have a sensible `Default` impl. If `Default` panics or returns garbage, old saves will deserialize to broken state. Test deserialization with empty JSON `{}` to verify.

### 9.6 Splash Screen and Initial Frame Timing

The app shows a splash screen during initialization to hide the shader compilation delay:

```rust
// crates/fractal-app/src/app.rs:807-833
if self.splash.is_some() {
    // Frame 1: hidden fractal render to warm up shader JIT
    if self.rendered_frames == 1 {
        self.pipeline.render(&mut encoder, &view);  // Hidden behind splash
    }

    self.render_splash_frame(&mut encoder, &view);

    // Time-based dismissal (not frame-count!)
    if self.start_time.elapsed().as_secs_f32() >= SPLASH_MIN_DURATION_SECS {
        self.splash = None;
        self.window.set_maximized(true);
    }
}
```

> **Gotcha**: Frame-count-based splash timing (e.g., "show for 2 frames") results in sub-millisecond visibility at high FPS. Always use `Instant::elapsed()` with a minimum duration.

> **Gotcha**: On Windows, the OS paints a white background on new windows before any GPU rendering. The app works around this by rendering a `LoadOp::Clear(BLACK)` frame immediately in `App::new()`, before any other initialization. The window also starts at splash size and only maximizes after the splash ends.

---

## Chapter 10: Compute Shaders & Advanced GPU Patterns

This chapter covers the compute shader pipeline used for mesh export, async GPU readback, and the staging buffer pattern.

### 10.1 Compute vs Render Pipelines

Render pipelines process vertices → fragments → pixels. Compute pipelines run arbitrary parallel code on the GPU with no fixed-function stages.

| Feature | Render Pipeline | Compute Pipeline |
|---------|----------------|-----------------|
| Input | Vertices, textures | Buffers, textures |
| Output | Color/depth attachments | Storage buffers, textures |
| Invocation | Per-vertex + per-fragment | Per-workgroup thread |
| Use case | Drawing to screen | General-purpose GPU compute |

The fractal viewer uses compute shaders to sample the SDF on a 3D grid — creating a volume that CPU-side mesh extraction algorithms (Marching Cubes, Dual Contouring, Surface Nets) convert into exportable meshes.

### 10.2 Compute Pipeline Setup

The compute pipeline reuses the same SDF functions as the render pipeline:

```rust
// crates/fractal-renderer/src/compute.rs:43-106
pub fn new(device: &wgpu::Device, _uniform_buffer: &wgpu::Buffer) -> Self {
    // Same SDF functions, different entry point
    let common_source = crate::pipeline::sdf_common_source();
    let volume_source = include_str!("../shaders/sdf_volume.wgsl");
    let full_source = format!("{common_source}\n{volume_source}");

    let compute_pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some("SDF Volume Compute Pipeline"),
        layout: Some(&pipeline_layout),
        module: &shader_module,
        entry_point: Some("sample_volume"),
        compilation_options: Default::default(),
        cache: None,
    });
    // ...
}
```

The bind group has three bindings:

```rust
// crates/fractal-renderer/src/compute.rs:54-91
// binding 0: Uniforms (shared SDF parameters — fractal type, iterations, etc.)
// binding 1: VolumeParams (grid bounds, resolution, slab offset)
// binding 2: Storage buffer (output — read/write)
```

The key difference from the render pipeline is **binding 2** — a storage buffer (`BufferBindingType::Storage { read_only: false }`) instead of a uniform buffer. Storage buffers can be written to from the shader and can be much larger than uniform buffers.

### 10.3 VolumeParams — Compute-Specific Uniforms

The compute shader needs its own parameters separate from the render uniforms:

```rust
// crates/fractal-renderer/src/compute.rs:10-25
#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
struct VolumeParams {
    bounds_min: [f32; 3],
    _pad0: f32,
    bounds_max: [f32; 3],
    _pad1: f32,
    grid_size: [u32; 3],
    z_offset: u32,
    slab_z_count: u32,
    _pad3: u32,
    _pad4: u32,
    _pad5: u32,
}
```

Same patterns as Chapter 6: `#[repr(C)]`, `Pod`, `Zeroable`, manual padding for alignment. The `z_offset` and `slab_z_count` fields support the multi-slab pattern (10.5).

### 10.4 The Compute Shader

```wgsl
// crates/fractal-renderer/shaders/sdf_volume.wgsl
@compute @workgroup_size(4, 4, 4)
fn sample_volume(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let vertex_count = volume_params.grid_size + vec3<u32>(1u, 1u, 1u);

    // Bounds check — threads beyond the grid do nothing
    if (global_id.x >= vertex_count.x ||
        global_id.y >= vertex_count.y ||
        global_id.z >= volume_params.slab_z_count) {
        return;
    }

    // Map thread ID to world-space position
    let global_z = global_id.z + volume_params.z_offset;
    let t = vec3<f32>(
        f32(global_id.x) / f32(volume_params.grid_size.x),
        f32(global_id.y) / f32(volume_params.grid_size.y),
        f32(global_z) / f32(volume_params.grid_size.z),
    );
    let pos = mix(volume_params.bounds_min, volume_params.bounds_max, t);

    // Evaluate the SDF at this position
    let result = map(pos);

    // Store result in the output buffer
    let index = global_id.x
              + global_id.y * vertex_count.x
              + global_id.z * vertex_count.x * vertex_count.y;
    volume[index] = result;
}
```

**Workgroup size 4x4x4 = 64 threads** — Each workgroup processes a 4x4x4 block of the volume. The GPU schedules many workgroups in parallel. The bounds check handles the case where the grid isn't evenly divisible by 4.

`@builtin(global_invocation_id)` is the 3D index of this thread across all workgroups — equivalent to HLSL's `SV_DispatchThreadID`.

### 10.5 Multi-Slab Dispatch

GPU storage buffers have a maximum binding size (typically 128MB-4GB depending on the GPU). For large volumes, the data might not fit in a single buffer.

The solution: split the volume into Z-axis slabs and dispatch one slab at a time:

```rust
// crates/fractal-renderer/src/compute.rs (multi-slab logic)
let max_binding = device.limits().max_storage_buffer_binding_size as u64;
let max_elements_per_slab = max_binding / 8;  // 8 bytes per vec2<f32>
let max_z_per_slab = (max_elements_per_slab / elements_per_layer).max(1);

let mut z_cursor: u32 = 0;
while z_cursor < total_z {
    let slab_z = (total_z - z_cursor).min(max_z_per_slab);
    // Dispatch compute for this slab
    self.dispatch_slab(device, queue, uniform_buffer, slab_z, z_cursor, ...);
    // Read results back to CPU
    let data = self.read_slab_blocking(device, slab_elements);
    output.extend_from_slice(&data);
    z_cursor += slab_z;
}
```

Each slab dispatches the compute shader with a different `z_offset` in `VolumeParams`, and the output is concatenated on the CPU side.

### 10.6 Async GPU Readback

GPU buffers aren't directly accessible from CPU. To read computed data back, you need a **staging buffer** and an async map operation:

```
GPU Storage Buffer  →  copy_buffer_to_buffer  →  Staging Buffer  →  map_async  →  CPU
(STORAGE usage)        (command encoder)          (MAP_READ usage)   (async)        (Vec<T>)
```

**Step 1: Initiate the async map**
```rust
// crates/fractal-renderer/src/compute.rs:386-393
pub fn initiate_map_async(&self) -> mpsc::Receiver<Result<(), BufferAsyncError>> {
    let buffer_slice = self.staging_buffer.slice(..);
    let (tx, rx) = std::sync::mpsc::channel();
    buffer_slice.map_async(wgpu::MapMode::Read, move |result| {
        let _ = tx.send(result);
    });
    rx
}
```

This returns immediately — the GPU hasn't finished yet. A channel `rx` will receive the completion signal.

**Step 2: Poll each frame (non-blocking)**
```rust
// crates/fractal-renderer/src/compute.rs:400-426
pub fn try_read_volume(
    &self,
    rx: &mpsc::Receiver<Result<(), BufferAsyncError>>,
) -> Option<Vec<[f32; 2]>> {
    match rx.try_recv() {
        Ok(Ok(())) => {
            // GPU finished — read the data
            let buffer_slice = self.staging_buffer.slice(..);
            let data = buffer_slice.get_mapped_range();
            let result: &[[f32; 2]] = bytemuck::cast_slice(&data);
            let output = result.to_vec();
            drop(data);
            self.staging_buffer.unmap();
            Some(output)
        }
        Ok(Err(e)) => { log::error!("Map failed: {e:?}"); None }
        Err(TryRecvError::Empty) => None,  // Still pending
        Err(_) => None,                     // Channel disconnected
    }
}
```

> **Gotcha**: `map_async` callbacks only fire when `device.poll()` is called. If you never poll the device, the callback never fires and the data never arrives. The app calls `device.poll(Maintain::Poll)` each frame to drive progress.

> **Tip**: Use `Maintain::Poll` for non-blocking checks (returns immediately). Use `Maintain::Wait` when you need the result before continuing (blocks until GPU finishes). The non-blocking pattern keeps the UI responsive during long compute operations.

### 10.7 Row Alignment for Texture Readback

When reading back textures (used for thumbnails), there's an additional constraint — row alignment:

```rust
// crates/fractal-renderer/src/thumbnail.rs
let padded_bytes_per_row =
    (bytes_per_row + wgpu::COPY_BYTES_PER_ROW_ALIGNMENT - 1)
    & !(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT - 1);
```

`COPY_BYTES_PER_ROW_ALIGNMENT` is 256 bytes. If your texture is 80 pixels wide × 4 bytes/pixel = 320 bytes per row, the padded row is 512 bytes (next multiple of 256). You must account for this padding when reading pixel data back.

### 10.8 BGRA vs RGBA Format Handling

Different backends return different pixel formats:

```rust
// crates/fractal-renderer/src/thumbnail.rs
let is_bgra = matches!(self.format, wgpu::TextureFormat::Bgra8Unorm | ...);

if is_bgra {
    // Swap B ↔ R channels: BGRA → RGBA
    for chunk in row_data.chunks_exact(4) {
        pixels.extend_from_slice(&[chunk[2], chunk[1], chunk[0], chunk[3]]);
    }
} else {
    pixels.extend_from_slice(row_data);
}
```

DirectX typically uses BGRA, Vulkan/Metal use RGBA. The code detects the format and converts at readback time.

---

## Appendix A: Exercises

These exercises build on what you've learned. Each one touches a different part of the stack.

### Exercise 1: Add a New Fractal Type

**Difficulty**: Medium | **Chapters**: 2, 6, 7, 8

Add a "Kaleidoscopic IFS" fractal. Follow the CLAUDE.md checklist:

1. Add `KaleidoscopicIFS = 6` to `FractalType` in `crates/fractal-core/src/fractals/mod.rs`
2. Add `name()` match arm returning `"Kaleidoscopic IFS"`
3. Add `all()` entry
4. Implement `sdf_kaleidoscopic()` in `crates/fractal-renderer/shaders/sdf_common.wgsl`
5. Add `case 6u:` to the `map()` switch
6. Add UI controls in `crates/fractal-ui/src/panels/fractal_params.rs`
7. Add range definitions in `crates/fractal-ui/src/app_settings.rs`

**Verification**: `cargo run --release`, select the new type from the ComboBox, adjust parameters.

### Exercise 2: Add a New Uniform Field

**Difficulty**: Easy | **Chapters**: 6

Add `fog_density: f32` to control distance fog:

1. In `uniforms.rs`: Replace one of the `_reserved` bytes with `fog_density: f32`
2. In `sdf_common.wgsl`: Add `fog_density: f32` at the matching byte offset
3. In `uniforms.rs`: Add `update_fog()` method
4. In `app.rs`: Call `update_fog()` before `update_uniforms()`
5. In `raymarcher.wgsl`: Use `u.fog_density` to blend the background color based on distance

**Verification**: Verify the size assertion still passes. Run and adjust the fog slider.

### Exercise 3: Create a Custom egui Widget

**Difficulty**: Medium | **Chapters**: 8

Build a 2D XY parameter picker (a square area where dragging sets two values at once):

1. Create a new function in one of the UI panels
2. Use `ui.allocate_rect()` to reserve a square area
3. Use `ui.painter_at()` to draw a crosshair at the current value
4. Handle `response.dragged()` to update both X and Y values
5. Use it for Julia C's X and Y components

**Verification**: Drag inside the square, see Julia fractal update in real time.

### Exercise 4: Modify the Ray Marcher to Add Fog

**Difficulty**: Easy | **Chapters**: 7

In `raymarcher.wgsl`, after the ray march hit/miss, blend the surface color with the background based on distance:

```wgsl
let fog_factor = exp(-distance * u.fog_density);
final_color = mix(u.background_color.rgb, surface_color, fog_factor);
```

**Verification**: Enable hot-reload, edit the shader, save, see fog appear instantly.

### Exercise 5: Add a New Session Field with Backward Compatibility

**Difficulty**: Easy | **Chapters**: 2, 9

Add a `description: String` field to `SavedSession`:

1. Add the field to `SavedSession` in `crates/fractal-core/src/session.rs`
2. Add it to `Default` impl with `description: String::new()`
3. Ensure `#[serde(default)]` is on the struct
4. Add a text input in the session panel UI
5. Test: load an old save file (without the field) — it should load without errors

**Verification**: `cargo test --workspace` passes. Old saves load correctly.

---

## Appendix B: Quick Reference Tables

### B.1 Rust ↔ C++ Concept Mapping

| Rust | C++ | Notes |
|------|-----|-------|
| `let x = 5;` | `const auto x = 5;` | Immutable by default |
| `let mut x = 5;` | `auto x = 5;` | Mutable |
| `fn foo(x: &T)` | `void foo(const T& x)` | Shared borrow |
| `fn foo(x: &mut T)` | `void foo(T& x)` | Exclusive borrow |
| `String` | `std::string` | Owned, heap-allocated |
| `&str` | `std::string_view` | Borrowed string slice |
| `Vec<T>` | `std::vector<T>` | Dynamic array |
| `&[T]` | `std::span<T>` | Borrowed slice |
| `HashMap<K, V>` | `std::unordered_map<K, V>` | Hash map |
| `Option<T>` | `std::optional<T>` | Nullable value |
| `Result<T, E>` | (no equivalent) | Error-or-value |
| `Box<T>` | `std::unique_ptr<T>` | Heap-allocated, single owner |
| `Arc<T>` | `std::shared_ptr<T>` | Reference-counted |
| `Mutex<T>` | `std::mutex` + data | Mutex guards data, not scope |
| `trait Foo` | `class Foo { virtual ... }` | Interface / abstract base |
| `impl Foo for Bar` | `class Bar : public Foo` | Trait implementation |
| `enum { A(T), B(U) }` | `std::variant<A, B>` | Tagged union |
| `match x { ... }` | `switch` / `std::visit` | Pattern matching |
| `x.clone()` | Copy constructor | Explicit deep copy |
| `#[derive(Clone)]` | `= default` copy ctor | Auto-generated |
| `Drop` trait | Destructor `~T()` | Cleanup on scope exit |
| `pub` | `public:` | Visibility modifier |
| `pub(crate)` | `friend` (sort of) | Crate-internal visibility |
| `mod` | `namespace` + `#include` | Module declaration |
| `use` | `using` | Import names |
| `cargo build` | `cmake --build .` | Build system |
| `cargo test` | `ctest` | Run tests |
| `Cargo.toml` | `CMakeLists.txt` | Build config |

### B.2 wgpu ↔ D3D12 ↔ Vulkan Mapping

| wgpu | D3D12 | Vulkan |
|------|-------|--------|
| `Instance` | `IDXGIFactory` | `VkInstance` |
| `Adapter` | `IDXGIAdapter` | `VkPhysicalDevice` |
| `Device` | `ID3D12Device` | `VkDevice` |
| `Queue` | `ID3D12CommandQueue` | `VkQueue` |
| `Surface` | `IDXGISwapChain` | `VkSwapchainKHR` |
| `CommandEncoder` | `ID3D12GraphicsCommandList` | `VkCommandBuffer` |
| `RenderPipeline` | `ID3D12PipelineState` | `VkPipeline` (graphics) |
| `ComputePipeline` | `ID3D12PipelineState` | `VkPipeline` (compute) |
| `BindGroupLayout` | Root parameter | `VkDescriptorSetLayout` |
| `BindGroup` | Descriptor table | `VkDescriptorSet` |
| `PipelineLayout` | Root signature | `VkPipelineLayout` |
| `Buffer` | `ID3D12Resource` | `VkBuffer` |
| `Texture` | `ID3D12Resource` | `VkImage` |
| `TextureView` | RTV/DSV/SRV | `VkImageView` |
| `Sampler` | Static/dynamic sampler | `VkSampler` |
| `ShaderModule` | `ID3DBlob` (compiled) | `VkShaderModule` |
| `queue.submit()` | `ExecuteCommandLists` | `vkQueueSubmit` |
| `surface.get_current_texture()` | `GetCurrentBackBufferIndex` | `vkAcquireNextImageKHR` |
| `output.present()` | `Present` | `vkQueuePresentKHR` |
| `queue.write_buffer()` | `Map` + `memcpy` + `Unmap` | `vkMapMemory` + copy |
| `device.poll()` | Fence wait | `vkWaitForFences` |

### B.3 WGSL ↔ GLSL ↔ HLSL Syntax

| Concept | WGSL | GLSL | HLSL |
|---------|------|------|------|
| Float | `f32` | `float` | `float` |
| Integer | `i32`, `u32` | `int`, `uint` | `int`, `uint` |
| Vector | `vec3<f32>` | `vec3` | `float3` |
| Matrix | `mat4x4<f32>` | `mat4` | `float4x4` |
| Immutable var | `let x = 1.0;` | `const float x = 1.0;` | `const float x = 1.0;` |
| Mutable var | `var x = 1.0;` | `float x = 1.0;` | `float x = 1.0;` |
| Function | `fn f(x: f32) -> f32` | `float f(float x)` | `float f(float x)` |
| Ternary | `select(b, a, cond)` | `cond ? a : b` | `cond ? a : b` |
| Entry point | `@vertex fn vs()` | `void main()` | `VS_OUTPUT vs()` |
| Uniform bind | `@group(0) @binding(0)` | `layout(binding=0)` | `register(b0)` |
| Vertex ID | `@builtin(vertex_index)` | `gl_VertexID` | `SV_VertexID` |
| Fragment pos | `@builtin(position)` | `gl_FragCoord` | `SV_Position` |
| Thread ID | `@builtin(global_invocation_id)` | `gl_GlobalInvocationID` | `SV_DispatchThreadID` |
| Varying out | `@location(0) uv: vec2<f32>` | `out vec2 uv` | `TEXCOORD0` |
| Workgroup | `@workgroup_size(4,4,4)` | `layout(local_size_x=4...)` | `[numthreads(4,4,4)]` |
| Struct return | `-> VertexOutput` | Multiple `out` vars | Struct return |
| Discard | `discard;` | `discard;` | `discard;` |
| Modulo (float) | `a % b` | `mod(a, b)` | `fmod(a, b)` |

---

*This tutorial was written based on the ModernFractalViewer codebase. For build commands and development workflow, see [DEVELOPMENT_GUIDE.md](DEVELOPMENT_GUIDE.md).*
