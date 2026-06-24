# wlr-capture

[![CI](https://github.com/sjourdois/wlr-utils/actions/workflows/ci.yml/badge.svg)](https://github.com/sjourdois/wlr-utils/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/wlr-capture.svg)](https://crates.io/crates/wlr-capture)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](#license)

The shared engine behind the [wlr-utils](https://github.com/sjourdois/wlr-utils)
tools ([`wlr-chooser`], [`wlr-pip`]). Two reusable bricks plus the overlay UI
helpers they share:

- **`wl`** — a native Wayland client that enumerates foreign toplevels and outputs
  (`ext-foreign-toplevel-list-v1`) and captures them at full resolution via
  `ext-image-capture-source-v1` + `ext-image-copy-capture-v1`. It computes the
  format-correct stride (so it works where `grim 1.5` fails with "Invalid
  stride"), and prefers a zero-copy GPU dma-buf path (allocated through `gbm`)
  with an automatic CPU shm fallback. Capture is occlusion-independent and
  damage-driven (windows on other workspaces stream live).
- **`gl`** — the EGL/GL dma-buf core: import a capture dma-buf as a GL texture
  (`EGL_EXT_image_dma_buf_import`) and `GpuReadback`, a headless offscreen context
  that reads such a dma-buf back to CPU RGBA8 (`glReadPixels` on a 1×1 pbuffer).
- **`clipboard`** — put a captured blob on the wlroots clipboard via
  `zwlr_data_control_v1` (the protocol `wl-copy` uses).
- **`sink`** — `FrameSink`, the common output seam for screenshot/record/timelapse;
  the default dma-buf path reads back through `GpuReadback`.
- **`render`** *(toolkit)* — an egui → `egui_glow` rendering core on an EGL/GLES
  context bound to a `wl_surface`, reusing `gl`'s dma-buf import for live textures.
  Any windowing host binds a `Gpu` to its surface and drives one egui frame per
  repaint.
- **`theme` / `i18n` / `icons`** *(toolkit)* — TOML theming, Fluent localisation
  (13 languages), and `.desktop`/icon-theme app-icon resolution.

## Status

This is primarily an **internal library** for the wlr-utils binaries; the public
API is not yet stabilised and may change between minor versions. It is published
so the tools can depend on it from crates.io.

Two features, both on by default:
- **`gpu`** pulls in `gbm` for the zero-copy dma-buf *capture* path; disable it for
  a pure-CPU (shm) build with no `libgbm` dependency. The `gl` dma-buf import +
  readback is built either way (it needs no `gbm`).
- **`toolkit`** is the egui/EGL overlay UI (`render` + `theme`/`i18n`/`icons`).
  Disable it (`--no-default-features`) for a headless build that only captures and
  reads back — dropping `egui`, `resvg`, `fontdb` and the i18n stack (a ~6× smaller
  dependency tree).

## License

Licensed under either of [Apache-2.0](../../LICENSE-APACHE) or
[MIT](../../LICENSE-MIT) at your option.

[`wlr-chooser`]: https://crates.io/crates/wlr-chooser
[`wlr-pip`]: https://crates.io/crates/wlr-pip
