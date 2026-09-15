# Aurora Studio

**A professional image editor and digital painting application — a true native desktop program.**

- **UI:** Avalonia 11 (C# / .NET 8) — native compiled Fluent dark interface, GPU-accelerated (Skia/ANGLE) rendering with automatic software fallback
- **Engine:** Rust (`aurora_engine`) — speed and memory safety, panic-guarded C ABI
- **Zero web technologies.** No Electron, no WebView, no browser runtime.
- **Zero-install:** ships as one self-contained `AuroraStudio.exe` (~45 MB) — the .NET runtime and the Rust engine are embedded; nothing else to install.

## Feature highlights

| Area | Details |
|------|---------|
| Layers | Create / delete / duplicate / reorder / groups / opacity / 16 blend modes / masks (add from selection, apply, delete) |
| Brush engine | Brush / Pencil / Eraser, hardness, flow, spacing, **Windows Ink pen pressure** (size & flow), tile-based flow accumulation |
| Selections | Rect, ellipse, lasso, magic wand (tolerance, contiguous), feather, invert, add (Shift) / subtract (Alt) |
| Transform | Move, scale, rotate, flip (layer & canvas), crop, image resize (nearest/bilinear/bicubic), canvas resize with anchor, perspective warp |
| Color | Eyedropper, HSV picker + hue bar, RGB/HEX fields, swatches, paint bucket (tolerance, contiguous), linear/radial gradients with dithering |
| Adjustments | Curves (per-channel monotone cubic), Levels, Brightness/Contrast, Hue/Saturation/Lightness — all selection-aware |
| Filters | Gaussian blur, sharpen (unsharp mask), add noise, pixelate, twirl, wave, emboss — all selection-aware, dialogs with live preview |
| Shapes & text | Rectangle / ellipse / line with fill & stroke, text tool with font family/size/bold/italic (Skia-rendered) |
| Navigation | Multi-document tabs, wheel zoom at cursor, space/middle-drag pan, **Hand (H)** & **Zoom (Z)** tools, fit / 100% |
| Undo | Full command-based undo/redo with history panel (click any state to jump) |
| Files | PNG (compression 0–9), JPEG (quality 1–100), WebP, GIF (single frame or layers-as-frames), BMP, TIFF, SVG import (resvg) / export (embedded raster), OpenRaster (.ora, layered), PSD (layered read **and** write) |
| Workspace | Panel visibility presets (Default / Minimal / Painting), settings persisted in `%APPDATA%\AuroraStudio\settings.json` |
| Shortcuts | Photoshop-style keys (V/B/E/M/L/W/I/G/T/U/C, Ctrl+Z/Y, Ctrl+D, `[`/`]` brush size, …) |

## Build from source

```bash
# 1. Rust engine (cdylib)
cd engine && cargo build --release

# 2. Stage the engine next to the app sources
cp target/release/libaurora_engine.so ../app/aurora_engine.so      # Linux
# copy target\release\aurora_engine.dll -> app\aurora_engine.dll   # Windows

# 3. Avalonia app — self-contained single file
cd ../app
dotnet publish -c Release -r win-x64 --self-contained true \
  -p:PublishSingleFile=true -p:IncludeNativeLibrariesForSelfExtract=true \
  -p:EnableCompressionInSingleFile=true -o publish
# => publish/AuroraStudio.exe
```

## Tool inventory (17 tools)

| # | Tool | Shortcut | Engine path |
|---|------|----------|-------------|
| 1 | Move | V | affine layer warp |
| 2 | Brush | B | tile-journaled pressure brush |
| 3 | Pencil | N | hardness 1.0, tight spacing |
| 4 | Eraser | E | destination-out stroke |
| 5 | Rectangular Select | M | mask rect (add/subtract) |
| 6 | Elliptical Select | J | AA ellipse mask |
| 7 | Lasso Select | L | even-odd polygon scanline |
| 8 | Magic Wand | W | scanline flood fill, tolerance |
| 9 | Eyedropper | I | composite pixel pick |
| 10 | Paint Bucket | G | scanline flood fill on layer |
| 11 | Gradient | R | linear/radial, dithered |
| 12 | Text | T | Skia raster → layer pixels |
| 13 | Shape | U | rect/ellipse/line, fill+stroke |
| 14 | Crop | C | canvas crop with rule-of-thirds |
| 15 | Free Transform | Ctrl+T | affine scale+rotate, perspective warp |
| 16 | Hand (Pan) | H / Space | viewport pan |
| 17 | Zoom | Z | cursor-anchored zoom in/out |

Plus menu commands: image/canvas resize, rotate/flip, crop-to-selection, flatten,
layer groups/masks/merge, feather/invert selection, 7 filters, 4 adjustments,
undo/redo with click-to-jump history, and export to 10 formats.

## Verification

The CI pipeline (`.github/workflows/ci.yml`) on every push:

1. Builds the Rust engine and the Avalonia app for **Windows x64** and **Linux x64**.
2. Publishes each as a **self-contained single-file** binary.
3. Runs the real exe with `--selftest` — the engine paints, selects, filters,
   adjusts, exports PNG/PSD/ORA, re-opens the files and replays the full undo
   history. Exit code gates the build.
4. Runs the real app with `--tooldemo` — a **43-step audit that drives every
   tool, filter, adjustment, layer operation, transform and export format
   through the real UI code paths**, verifies observable pixel results, and
   writes `toolreport.json`. A single failing step fails the build.
5. Launches the real app with `--autodemo` (drives genuine brush/layer/filter
   operations through the UI), then captures **real screenshots** of the running
   window (runner desktop via CopyFromScreen on Windows; Xvfb + ffmpeg on Linux).
6. Uploads binaries + tool report + screenshots as artifacts.

### Crash safety

- Every engine export is wrapped in a panic guard; failures return error codes
  instead of unwinding across the FFI boundary.
- All mutating operations recomposite immediately (no stale canvas reads).
- The UI installs global exception shields (`Dispatcher`, `AppDomain`,
  `TaskScheduler`) plus per-command and per-pointer-event guards: an error now
  shows a dialog and writes `%APPDATA%\AuroraStudio\crash.log` instead of the
  process silently exiting.
- The active layer is always queried live from the engine (`aurora_active_layer`),
  never from a cached snapshot — tools can no longer paint into the wrong layer.

## License

MIT
