# wlr-capture

[![CI](https://github.com/sjourdois/wlr-utils/actions/workflows/ci.yml/badge.svg)](https://github.com/sjourdois/wlr-utils/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/wlr-capture.svg)](https://crates.io/crates/wlr-capture)
[![docs.rs](https://docs.rs/wlr-capture/badge.svg)](https://docs.rs/wlr-capture)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](#license)

The shared engine behind the [wlr-utils](https://github.com/sjourdois/wlr-utils)
tools (`wlr-chooser`, `wlr-switcher`, `wlr-peek`, `wlr-shot`, `wlr-draw`) — the
capture + overlay toolkit they all build on:

<p align="center">
  <img src="https://raw.githubusercontent.com/sjourdois/wlr-utils/main/docs/assets/wlr-switcher/expose.gif" width="32%" alt="wlr-switcher exposé">
  <img src="https://raw.githubusercontent.com/sjourdois/wlr-utils/main/docs/assets/wlr-draw/annotate.gif" width="32%" alt="wlr-draw annotation">
  <img src="https://raw.githubusercontent.com/sjourdois/wlr-utils/main/docs/assets/wlr-peek/color.gif" width="32%" alt="wlr-peek colour picker">
</p>
<p align="center"><sub>See all the tools in action on the
<a href="https://sjourdois.github.io/wlr-utils/">showcase</a>.</sub></p>

The reusable bricks plus the overlay UI helpers they share:

- **`wl`** — a native Wayland client that enumerates foreign toplevels and outputs
  (`ext-foreign-toplevel-list-v1`) and captures them at full resolution via
  `ext-image-capture-source-v1` + `ext-image-copy-capture-v1`, or via
  `zwlr-screencopy-v1` where those are absent — that one addresses a `wl_output`
  and never a window, so it carries screen capture only. It computes the
  format-correct stride (so it works where `grim 1.5` fails with "Invalid
  stride"). Live sessions take a zero-copy dma-buf path (allocated through `gbm`,
  every plane of the modifier declared); single captures, which end up as CPU
  pixels anyway, allocate shm directly. An import the driver refuses drops the
  client to shm (`Client::disable_gpu`, `WLR_NO_GPU`). Capture is
  occlusion-independent and damage-driven (windows on other workspaces stream live).
- **`gl`** — the EGL/GL dma-buf core: import a capture dma-buf as a GL texture
  (`EGL_EXT_image_dma_buf_import`) and `GpuReadback`, a headless offscreen context
  that reads such a dma-buf back to CPU RGBA8 (`glReadPixels` on a 1×1 pbuffer).
- **`clipboard`** — put a captured blob on the wlroots clipboard via
  `zwlr_data_control_v1` (the protocol `wl-copy` uses).
- **`sink`** — `FrameSink`, the common output seam for screenshot/record/timelapse;
  dma-buf frames read back through `GpuReadback` unless a sink consumes them on the GPU.
- **`stream` / `diff`** — a shared capture-session driver (arm / poll / reopen /
  give-up) and a frame-difference metric, shared by the mirror, recorder and monitor.
- **`capture` / `focus`** *(features)* — resolve a source to a `CapturedImage`
  (cropping + multi-output compositing), and compositor-IPC focus backends (Sway /
  Hyprland / niri) for active-window / current-output sources.
- **`overlay` / `mirror`** *(features)* — the frozen region/point/magnify selector and
  the floating live-mirror (PiP) host.
- **`video` / `audio`** *(features)* — FFmpeg encoding (H.264 NVENC/VAAPI/libx264, and
  animated GIF/WebP) and native-PipeWire audio capture (with an optional Pulse/ALSA
  fallback via libavdevice).
- **`render`** *(toolkit)* — an egui → `egui_glow` rendering core on an EGL/GLES
  context bound to a `wl_surface`, reusing `gl`'s dma-buf import for live textures.
  Any windowing host binds a `Gpu` to its surface and drives one egui frame per
  repaint.
- **`theme` / `icons`** *(toolkit)* — TOML theming and `.desktop`/icon-theme
  app-icon resolution. On-screen text is passed in by the caller; each tool crate
  owns its own Fluent catalog via the `wlr-i18n` crate.

## Status

This is primarily an **internal library** for the wlr-utils binaries; the public
API is not yet stabilised and may change between minor versions. It is published
so the tools can depend on it from crates.io.

A lean always-on core (`wl`, `gl`, `clipboard`, `sink`, `stream`, `diff`) plus opt-in
features. On by default: `gpu`, `toolkit`.

- **`gpu`** — the zero-copy dma-buf *capture* path (pulls `gbm`); without it a pure-CPU
  shm build (no `libgbm`). The `gl` dma-buf import + readback is built either way.
- **`toolkit`** — the egui/EGL overlay UI (`render` + `theme`/`icons`); drop it
  (`--no-default-features`) for a headless build that only captures and reads back —
  no `egui`/`resvg`/`fontconfig`.
- Off by default: **`compose`** (source→image), **`focus`** (compositor IPC),
  **`overlay`**, **`mirror`**, **`video`** (FFmpeg), **`audio`** (PipeWire;
  **`audio-fallback`** adds Pulse/ALSA).

## License

Licensed under either of [Apache-2.0](../../LICENSE-APACHE) or
[MIT](../../LICENSE-MIT) at your option.

[`wlr-chooser`]: https://crates.io/crates/wlr-chooser
