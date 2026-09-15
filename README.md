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
| Navigation | Multi-document tabs, wheel zoom at cursor, space/middle-drag pan, fit / 100% |
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

## Verification

The CI pipeline (`.github/workflows/ci.yml`) on every push:

1. Builds the Rust engine and the Avalonia app for **Windows x64** and **Linux x64**.
2. Publishes each as a **self-contained single-file** binary.
3. Runs the real exe with `--selftest` — the engine paints, selects, filters,
   adjusts, exports PNG/PSD/ORA, re-opens the files and replays the full undo
   history. Exit code gates the build.
4. Launches the real app with `--autodemo` (drives genuine brush/layer/filter
   operations through the UI), then captures **real screenshots** of the running
   window (runner desktop via CopyFromScreen on Windows; Xvfb + ffmpeg on Linux).
5. Uploads binaries + screenshots as artifacts.

## License

MIT
