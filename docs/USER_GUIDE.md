# Modern Fractal Viewer &mdash; User Guide

A comprehensive reference for all features in the Modern 3D Fractal Viewer.

---

## Quick Start

### Desktop (Windows / macOS / Linux)

```bash
cargo run -p fractal-app --release
```

On launch you'll see the default **Mandelbulb** fractal rendered in real time, with a floating control panel on the left.

### Web

Open the [live demo](https://jiamingfeng.github.io/ModernFractalViewer/) in a WebGPU-enabled browser (see [Platform Notes](#platform-notes) for browser requirements).

### Android

Install the APK built with `cargo-ndk`. The app launches fullscreen; tap the hamburger menu (top left) to open the control panel.

---

## Controls

### Mouse

| Action | Input |
|--------|-------|
| Orbit camera | Left-click + drag |
| Pan camera | Right-click + drag |
| Zoom in/out | Scroll wheel |

### Touch (Mobile / Tablet)

| Action | Input |
|--------|-------|
| Orbit camera | Single finger drag |
| Pan camera | Two finger drag |
| Zoom in/out | Pinch gesture |

### Keyboard Shortcuts

| Key | Action |
|-----|--------|
| **ESC** | Toggle the control panel |
| **R** | Reset camera to default position |
| **Space** | Toggle auto-rotate mode |
| **L** + drag | Rotate the light direction (shows gizmo while held) |

---

## The Control Panel

The control panel is a draggable, resizable floating window anchored to the left edge by default. You can:

- **Close** it with the **X** button (top right) or press **ESC**
- **Reopen** it with the **hamburger menu** button that appears at the top left, or press **ESC** again
- **Scroll** within the panel if content overflows

The panel contains the following sections (each collapsible):

---

## Fractal Types

Select a fractal from the dropdown at the top of the panel. Six types are available, each with unique parameters:

### Mandelbulb

A 3D generalization of the Mandelbrot set. The iconic "bulb" shape emerges from raising a 3D coordinate to a power and iterating.

| Parameter | Range | Default | Effect |
|-----------|-------|---------|--------|
| Power | 1.0 &ndash; 16.0 | 8.0 | Controls the shape's symmetry. Power 8 gives the classic Mandelbulb; lower values create smoother forms, higher values add more lobes. |
| Iterations | 1 &ndash; 32 | 12 | More iterations reveal finer detail but cost more GPU time. |
| Escape Radius | 1.0 &ndash; 8.0 | 2.0 | The bailout threshold. Smaller values clip geometry; larger values add detail at the boundary. |

**Tips:** Start with power 8 and lower iterations (4&ndash;6) for smooth exploration. Increase iterations when you've found an interesting area.

### Menger Sponge

A fractal constructed by recursively removing cubes from a larger cube, creating a Swiss-cheese-like structure with infinite surface area.

| Parameter | Range | Default | Effect |
|-----------|-------|---------|--------|
| Iterations | 1 &ndash; 8 | 4 | Each iteration triples the detail level. 4&ndash;5 iterations look great without heavy GPU load. |

**Tips:** Try rotating slowly to appreciate the self-similar structure. Works well with the Orbit Trap color mode.

### Julia 3D

A 3D Julia set using quaternion-like iteration. The shape depends on a constant vector **C**.

| Parameter | Range | Default | Effect |
|-----------|-------|---------|--------|
| Iterations | 1 &ndash; 32 | 11 | More iterations refine the fractal boundary. |
| C.x | -2.0 &ndash; 2.0 | -0.8 | X component of the Julia constant. |
| C.y | -2.0 &ndash; 2.0 | 0.156 | Y component of the Julia constant. |
| C.z | -2.0 &ndash; 2.0 | 0.0 | Z component of the Julia constant. |

**Tips:** Small changes to C produce dramatically different shapes. Try C = (-0.1, 0.7, 0.0) for a coral-like form.

### Mandelbox

A box fractal that combines folding and spherical inversion. The "box scale" parameter controls whether the fractal is open or closed.

| Parameter | Range | Default | Effect |
|-----------|-------|---------|--------|
| Box Scale | -3.0 &ndash; 3.0 | 2.0 | Positive values create open structures; negative values (try -1.5) create enclosed, cave-like forms. |
| Iterations | 1 &ndash; 32 | 15 | More iterations add fine detail. |
| Fold Range | 0.5 &ndash; 2.0 | 1.0 | Size of the folding region. |
| Inner Radius | 0.01 &ndash; 1.0 | 0.25 | Controls the spherical inversion boundary. |

**Tips:** Negative box scale values are where the magic happens. Try scale = -1.5 with the Fire palette.

### Sierpinski

A 3D Sierpinski tetrahedron / fractal pyramid.

| Parameter | Range | Default | Effect |
|-----------|-------|---------|--------|
| Iterations | 1 &ndash; 20 | 12 | Controls recursion depth. |
| Size Ratio | 1.5 &ndash; 3.0 | 2.0 | The scaling factor per iteration. 2.0 gives the classic Sierpinski; other values create variations. |

**Tips:** Looks best with directional lighting. Try increasing iterations for a spongier appearance.

### Apollonian

An Apollonian gasket in 3D &mdash; packed spheres within spheres.

| Parameter | Range | Default | Effect |
|-----------|-------|---------|--------|
| Iterations | 1 &ndash; 12 | 8 | Controls the nesting depth of sphere packing. |

**Tips:** Zoom in to see the self-similar sphere packing at different scales.

---

## Rendering Settings

These control the ray marching algorithm that renders the fractal. Found in the **Rendering** collapsible section.

| Setting | Range | Default | What it does |
|---------|-------|---------|-------------|
| **Ray Steps** | 16 &ndash; 512 | 128 | Maximum number of steps each ray takes through the scene. Higher = more detail in complex areas, but slower. |
| **Surface Precision** | 0.00001 &ndash; 0.01 | 0.001 | How close a ray must get to the surface to count as a "hit". Smaller values reveal finer detail but may cause noise on some fractals. |
| **View Distance** | 10 &ndash; 1000 | 100 | Maximum distance a ray will travel before giving up. Increase for very large fractal structures. |
| **Shadow Detail** | 0 &ndash; 16 | 5 | Number of ambient occlusion samples. More samples = softer, more accurate shadows, but costs GPU time. Set to 0 to disable AO entirely. |
| **Shadow Depth** | 0.0 &ndash; 1.0 | 0.2 | Intensity of ambient occlusion darkening. 0 = no darkening, 1 = maximum darkening. |
| **Normal Precision** | 0.000001 &ndash; 0.01 | 0.0001 | Step size for computing surface normals (via finite differences). Smaller = smoother normals but potentially noisier on distant surfaces. |
| **Anti-Aliasing** | 1x / 2x / 4x | 1x | Super-sampling factor. 4x renders each pixel 4 times for smoother edges, at 4x the GPU cost. |

### Performance vs. Quality Tips

| Goal | Adjustments |
|------|-------------|
| **Smooth interactive exploration** | Ray Steps: 64&ndash;128, AA: 1x, Shadow Detail: 0&ndash;3 |
| **High quality still image** | Ray Steps: 256&ndash;512, AA: 4x, Shadow Detail: 8&ndash;16, Surface Precision: 0.0001 |
| **Debugging geometry** | Shadow Detail: 0, Color Mode: Normal |

---

## Color & Lighting

Found in the **Color Settings** collapsible section.

### Color Modes

Select a coloring algorithm from the **Color Mode** dropdown:

| Mode | Description |
|------|-------------|
| **Solid** | Uses a single base color for the entire surface. |
| **Orbit Trap** (default) | Colors based on how close the fractal orbit comes to a reference point. Produces smooth, organic-looking gradients. |
| **Iteration** | Colors based on how many iterations the fractal took to escape. Creates banded, contour-like patterns. |
| **Normal** | Colors based on the surface normal direction (RGB = XYZ). Useful for debugging geometry. Palette controls are hidden in this mode. |
| **Combined** | Blends multiple coloring channels for rich, complex color. |

### Palette System

The palette controls how colors are mapped across the fractal surface (hidden in Normal mode).

**Presets:** Choose from 8 built-in palettes:

| Preset | Colors | Character |
|--------|--------|-----------|
| Inferno | 5 | Dark purple through orange to yellow |
| Ocean | 5 | Deep blue through teal to white |
| Sunset | 5 | Purple through red-orange to light yellow |
| Magma | 5 | Dark purple through red to warm yellow |
| Viridis | 5 | Purple through teal to yellow-green |
| Classic | 2 | Orange to blue gradient |
| Fire | 6 | Black through red, orange, yellow to white |
| Ice | 5 | Dark blue through medium blue to white |

**Custom Palettes:** Click any color stop to open a color picker. You can:
- **Add** color stops (up to 8) with the **+ Add Color** button
- **Remove** color stops (minimum 2) with the **X** button next to each stop
- Editing any color automatically switches to "Custom" mode

### Color Spread & Color Shift

| Control | Range | Default | Effect |
|---------|-------|---------|--------|
| **Color Spread** | 0.1 &ndash; 10.0 | 1.6 | Multiplier on the palette lookup scale. Lower values stretch colors over a wider area; higher values compress them for more color variation. |
| **Color Shift** | 0.0 &ndash; 1.0 | 0.0 | Offsets the starting point of the palette cycle. Rotate through colors without changing the palette itself. |

### Background Color

Click the color swatch to pick a custom background color (default: very dark blue, `#0D0D1A`).

### Lighting

Two lighting models are available, selectable via the **Model** dropdown:

#### Blinn-Phong (default)

The classic real-time lighting model with separate diffuse and specular controls.

| Control | Range | Default | Effect |
|---------|-------|---------|--------|
| **Ambient Light** | 0.0 &ndash; 1.0 | 0.1 | Base illumination level. Higher values wash out shadows. |
| **Direct Light** | 0.0 &ndash; 1.0 | 0.8 | Intensity of the directional light source. |
| **Reflection** | 0.0 &ndash; 1.0 | 0.3 | Specular highlight intensity. |
| **Gloss** | 1.0 &ndash; 128.0 | 32.0 | Specular exponent. Higher = tighter highlights. |

#### PBR (GGX)

Physically-based rendering using the Cook-Torrance GGX microfacet BRDF (similar to glTF 2.0 / Unreal Engine 5). Uses a metallic-roughness workflow.

| Control | Range | Default | Effect |
|---------|-------|---------|--------|
| **Ambient Light** | 0.0 &ndash; 1.0 | 0.1 | Base illumination level. |
| **Roughness** | 0.0 &ndash; 1.0 | 0.5 | Surface roughness. 0 = mirror-smooth, 1 = fully rough/diffuse. |
| **Metallic** | 0.0 &ndash; 1.0 | 0.0 | Metalness. 0 = dielectric (plastic), 1 = metal (reflects surface color). |
| **Light Intensity** | 0.0 &ndash; 5.0 | 1.5 | Brightness of the direct light. |

#### Shared Controls (both models)

| Control | Range | Default | Effect |
|---------|-------|---------|--------|
| **Shadow Softness** | 1 &ndash; 64 | 8 | Controls penumbra width. Higher = softer shadows. Lower = harder shadows. |
| **Light Dir** (x, y, z) | &ndash; | (0.577, 0.577, 0.577) | Editable light direction vector (auto-normalized). |

#### Interactive Light Direction

Hold **L** and drag the mouse to rotate the light direction on a unit sphere. A gizmo overlay appears showing:
- **Yellow arrow**: Current light direction
- **RGB axes**: X (red), Y (green), Z (blue) reference axes
- **Hemisphere**: Visual boundary of the light sphere

A **coordinate axes gizmo** (XYZ) is always visible in the bottom-right corner for orientation reference.

### Noise Smoothing

| Control | Range | Default | Effect |
|---------|-------|---------|--------|
| **Noise Smoothing** | 0.0 &ndash; 2.0 | 1.0 | Dithering strength to reduce color banding. 0 = off, 1 = normal, up to 2 = aggressive smoothing. |

---

## Camera Controls

Found in the **Camera** collapsible section.

| Control | Range | Default | Effect |
|---------|-------|---------|--------|
| **FOV** | 30&deg; &ndash; 120&deg; | 60&deg; | Field of view. Lower values zoom in (telephoto); higher values create a fisheye effect. |
| **Zoom** | 0.05x &ndash; 1000x | 1.0x | Logarithmic zoom slider (same as scroll wheel). |

### Quick Views

| Button | What it does |
|--------|-------------|
| **Reset Camera** | Returns to the default position, orientation, and zoom |
| **Top View** | Looks straight down from above |
| **Front View** | Front-facing orthogonal view |

### Position Display

The panel shows the current camera position (X, Y, Z) as read-only values.

---

## Session Management

Found in the **Sessions** collapsible section (collapsed by default).

### Saving a Session

1. Type a name in the **Name** field (e.g., "Mandelbulb zoom detail")
2. Click **Save Current Session**

The save captures:
- All fractal parameters (type, power, iterations, etc.)
- Ray marching, lighting, and color configuration
- Camera position, orientation, and zoom
- A thumbnail preview of the current view

### Loading a Session

Saved sessions appear as a scrollable list with thumbnail previews. Each entry shows:
- Thumbnail image
- Session name
- Timestamp and fractal type

Click **Load** to restore all parameters exactly, producing an identical rendering.

### Deleting a Session

Click **Delete** next to any saved session to remove it permanently.

### Where Sessions Are Stored

| Platform | Location |
|----------|----------|
| **Windows** | `%APPDATA%\ModernFractalViewer\saves\` |
| **macOS** | `~/Library/Application Support/ModernFractalViewer/saves/` |
| **Linux** | `~/.local/share/ModernFractalViewer/saves/` |
| **Web** | Browser's `localStorage` |
| **Android** | App's internal data directory |

Each session is stored as a single `.json` file containing all parameters and an embedded thumbnail.

---

## Debug Panel

Found in the **Debug** collapsible section.

| Control | Default | Effect |
|---------|---------|--------|
| **Show debug info** | Off | Displays an overlay in the top-right corner with FPS, camera position, and zoom level |
| **VSync** | On | Synchronizes frame rendering with your display's refresh rate. Disabling may increase FPS but can cause screen tearing. |
| **Auto-rotate** | Off | Continuously orbits the camera around the fractal |
| **Speed** (when auto-rotate is on) | 0.5 | Rotation speed (0.1 &ndash; 2.0) |

---

## Performance Tips

1. **Lower Ray Steps** (64&ndash;128) while exploring. Increase for final renders.
2. **Disable Anti-Aliasing** (1x) during interactive use. Use 4x for screenshots.
3. **Reduce Shadow Detail** to 0&ndash;3 for faster rendering.
4. **Lower fractal iterations** while navigating. Fewer iterations = smoother camera movement.
5. **Enable VSync** to prevent unnecessary GPU work beyond your refresh rate.
6. **Use smaller window sizes** if performance is poor &mdash; the GPU renders every pixel.

---

## Platform Notes

### Desktop (Windows / macOS / Linux)

- Requires a GPU supporting Vulkan (Windows/Linux), Metal (macOS), or DirectX 12 (Windows)
- The app automatically selects the best available graphics backend

### Web (WebGPU)

Requires a browser with WebGPU support:

| Browser | Version | Notes |
|---------|---------|-------|
| Chrome | 113+ | WebGPU enabled by default |
| Edge | 113+ | WebGPU enabled by default |
| Firefox | Nightly | Enable `dom.webgpu.enabled` in `about:config` |
| Safari | 18+ | macOS Sequoia / iOS 18 |

### Android

- Requires a device with Vulkan support (most devices from 2017+)
- The control panel starts hidden; tap the hamburger menu to open it
- Touch controls are the primary input method
