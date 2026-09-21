# wlr-utils

[![CI](https://github.com/sjourdois/wlr-utils/actions/workflows/ci.yml/badge.svg)](https://github.com/sjourdois/wlr-utils/actions/workflows/ci.yml)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](#license)
[![Showcase](https://img.shields.io/badge/▶_showcase-sjourdois.github.io%2Fwlr--utils-8aadf4)](https://sjourdois.github.io/wlr-utils/)

### Capture · switch · inspect · annotate your screen — the native Wayland way.

⚡ Zero-copy GPU capture &nbsp;·&nbsp; 👁️ Sees occluded & off-workspace windows
&nbsp;·&nbsp; 🦀 Rust, no XWayland &nbsp;·&nbsp; 🎨 Themeable &nbsp;·&nbsp; 🌍 13 languages

Five graphical tools for **wlroots and derivatives**.

| Tool | What it does | crate |
| --- | --- | --- |
| **[wlr-chooser](crates/wlr-chooser)** | Window & screen picker for screencast portals (`xdg-desktop-portal-wlr`) — a rofi-like overlay with live thumbnails. | [![v](https://img.shields.io/crates/v/wlr-chooser.svg)](https://crates.io/crates/wlr-chooser) |
| **[wlr-switcher](crates/wlr-chooser)** | Live **Alt-Tab / exposé** window switcher (macOS-style strip, full-screen grid, or card) with hold-to-switch and live previews. Ships with `wlr-chooser`. | [![v](https://img.shields.io/crates/v/wlr-chooser.svg)](https://crates.io/crates/wlr-chooser) |
| **[wlr-peek](crates/wlr-peek)** | **Inspect the screen** — colour picker, loupe, OCR, live picture-in-picture **mirror** (window or region), **change monitor** (`watch`), and **visual grep**. | [![v](https://img.shields.io/crates/v/wlr-peek.svg)](https://crates.io/crates/wlr-peek) |
| **[wlr-shot](crates/wlr-shot)** | **Screen capture** — screenshots of an output/region/window (PNG/JPEG/PPM), copy to clipboard; plus **recording** (H.264, or animated GIF/WebP) with **system audio** & **timelapse** (NVENC/VAAPI/libx264). | [![v](https://img.shields.io/crates/v/wlr-shot.svg)](https://crates.io/crates/wlr-shot) |
| **[wlr-draw](crates/wlr-draw)** | **Draw on screen** — a transparent annotation overlay (gromit-mpx-style): freehand, shapes, arrows, text, dwell-to-snap, element move, plus presenter **spotlight**, **freeze-frame** and **save**. Daemon + control socket. | [![v](https://img.shields.io/crates/v/wlr-draw.svg)](https://crates.io/crates/wlr-draw) |

> **Point at a window you can't see.** `wlr-shot` and `wlr-peek` take `--app-id`/`--title`,
> so you can screenshot, record, mirror, OCR or watch a window *by name* — even one occluded
> behind others or on another workspace, with no clicking and no workspace switch
> (`wlr-shot screenshot --list-windows` shows what's open). Reaching hidden and off-screen
> windows is the whole point of building on `ext-foreign-toplevel` rather than grabbing a
> visible output.

They all share two library crates: **[wlr-capture](crates/wlr-capture)**, the wlroots
capture engine (`ext-image-copy-capture-v1`, full-resolution dma-buf zero-copy with a
CPU shm fallback) plus an egui/EGL rendering + dma-buf-import toolkit; and
**[wlr-i18n](crates/wlr-i18n)**, the shared Fluent localisation plumbing each tool builds
its own message catalog on.

<p align="center">
  <img src="https://raw.githubusercontent.com/sjourdois/wlr-utils/main/docs/assets/wlr-draw/annotate.gif" width="49%" alt="wlr-draw — annotate live on screen">
  <img src="https://raw.githubusercontent.com/sjourdois/wlr-utils/main/docs/assets/wlr-switcher/altab.gif" width="49%" alt="wlr-switcher — Alt-Tab with live previews">
</p>
<p align="center">
  <img src="https://raw.githubusercontent.com/sjourdois/wlr-utils/main/docs/assets/wlr-shot/select.gif" width="49%" alt="wlr-shot — frozen region selector">
  <img src="https://raw.githubusercontent.com/sjourdois/wlr-utils/main/docs/assets/wlr-peek/color.gif" width="49%" alt="wlr-peek — colour picker with loupe">
</p>
<p align="center"><sub>wlr-draw · wlr-switcher · wlr-shot · wlr-peek — see the <a href="https://sjourdois.github.io/wlr-utils/">showcase</a></sub></p>

## Requirements

A compositor that speaks the wlroots protocols — wlroots-based or not, since Hyprland,
niri and cosmic-comp implement them on their own stacks. What you get depends on which
capture protocols it exposes:

| Capability | Compositor floor | Wayland protocol |
| --- | --- | --- |
| **Screen** capture (screenshots, recording, loupe, annotation) | wlroots ≥ 0.19 · Sway ≥ 1.11, or any compositor with `wlr-screencopy` | `ext-image-copy-capture-v1` (else `wlr-screencopy`) + `wlr-layer-shell` |
| **Window** capture (switcher, `-w`, window mirror/record) | wlroots ≥ 0.20 · Sway ≥ 1.12 | adds `ext-foreign-toplevel-list-v1` |

Tools degrade gracefully: where windows aren't capturable they keep their screen features
and say so. See [COMPATIBILITY.md](COMPATIBILITY.md) for the full matrix (Hyprland, niri,
labwc, …), or run the `doctor` command that every tool exposes (e.g. `wlr-shot doctor`,
or `wlr-chooser --doctor`) to check your own compositor. It also captures a frame through
the GPU path and reports whether it can be imported, which is the line to quote in a bug
report.

Runtime libraries:

| Library | Needed by | Why |
| --- | --- | --- |
| `libegl1` | every tool | EGL/GLES overlay rendering |
| `libfontconfig1` | every tool | looking up the UI font; without it the overlay falls back to the embedded fonts, which have no CJK coverage |
| `libgbm` (Mesa) | every tool | zero-copy GPU capture path; pass `--no-gpu` (or set `WLR_NO_GPU=1`) to capture through shared memory instead |
| `xdg-desktop-portal-wlr` ≥ 0.8 | `wlr-chooser` | portal-based picking |
| `zwlr-foreign-toplevel-management-v1` | `wlr-switcher` | focusing windows |

## Install

Every route below installs the same five binaries (`wlr-chooser`, `wlr-switcher`,
`wlr-peek`, `wlr-shot`, `wlr-draw`).

### Arch Linux

```sh
paru -S wlr-utils-bin    # prebuilt from the release (fast)
paru -S wlr-utils        # …or built from source
```

Both ship the same tools and conflict with each other — pick one. Any AUR helper works.

### Debian / Ubuntu

One package with the whole suite is attached to every
[release](https://github.com/sjourdois/wlr-utils/releases/latest), built **per distro** so
it links against that distro's FFmpeg / Leptonica. Pick the matching asset:

| Distro | Asset suffix |
| --- | --- |
| Debian 12 (bookworm) | `…_amd64.bookworm.deb` |
| Debian 13 (trixie) | `…_amd64.trixie.deb` |
| Debian 14 (forky) / sid | `…_amd64.forky.deb` / `…_amd64.sid.deb` |
| Ubuntu 22.04 (jammy) | `…_amd64.jammy.deb` — screenshots only, no `wlr-shot record` |
| Ubuntu 24.04 (noble) / 26.04 | `…_amd64.noble.deb` / `…_amd64.ubuntu2604.deb` |

```sh
sudo apt install ./wlr-utils_*_amd64.trixie.deb   # apt pulls the dependencies in
```

Ubuntu 22.04's PipeWire / FFmpeg are too old to build the recorder, so that `.deb` omits
`wlr-shot record`; install from source there if you need it.

> [!IMPORTANT]
> These `.deb`s link **dynamically** against the FFmpeg (`libavutil`) and Leptonica
> (`liblept`) of the distro they were built on. If your installed versions don't match
> (different release, backports, a soname your distro doesn't ship), the tool won't start —
> `error while loading shared libraries: libavutil.so.NN` / `liblept.so.N`. Build from
> source instead: it links against whatever you have.

### NixOS / Nix

Packaged in [nixpkgs](https://github.com/NixOS/nixpkgs/tree/master/pkgs/by-name/wl/wlr-utils),
on the unstable channel.

```sh
nix-shell -p wlr-utils                 # try it
nix profile install nixpkgs#wlr-utils  # …or install it
```

On NixOS, add `wlr-utils` to `environment.systemPackages`.

### Prebuilt binaries

One archive with every binary, for any distro — no Rust toolchain needed:

```sh
curl --proto '=https' --tlsv1.2 -LsSf \
  https://github.com/sjourdois/wlr-utils/releases/latest/download/wlr-utils-installer.sh | sh
```

### From source

```sh
cargo install wlr-utils          # the whole suite
```

Each tool is also its own crate, if you want just one — it then pulls only that tool's
system dependencies (see its README):

```sh
cargo install wlr-chooser        # window/screen picker + wlr-switcher (Alt-Tab/exposé)
cargo install wlr-peek           # colour picker, loupe, OCR, live mirror, watch
cargo install wlr-shot           # screenshots + recording
cargo install wlr-draw           # annotation overlay
```

Building the checkout directly works the same way (`cargo build --release` puts every
binary in `target/release`). Either route needs the `-dev` packages of the features you
build: Tesseract for OCR, FFmpeg for recording, and `libgbm-dev` for the GPU capture path.

### Uninstall

Package installs come off the usual way (`paru -R wlr-utils-bin`, `apt remove wlr-utils`).
A `cargo install` drops the binaries in `~/.cargo/bin`; remove the bundle with
`cargo uninstall wlr-utils`, or an individual tool the same way:

```sh
cargo uninstall wlr-utils        # the whole bundle
cargo uninstall wlr-draw         # …or just one: wlr-chooser / wlr-peek / wlr-shot
```

`wlr-draw` also registers an XDG autostart entry on first run (see its README). Drop the
checkbox in its tray menu, or delete the files by hand (honouring `$XDG_CONFIG_HOME` /
`$XDG_STATE_HOME` if you set them):

```sh
rm -f ~/.config/autostart/wlr-draw.desktop \
      ~/.local/state/wlr-draw/autostart-initialized
# and, if you installed the systemd unit:
systemctl --user disable --now wlr-draw.service
rm -f ~/.config/systemd/user/wlr-draw.service
```

## Documentation

- **[wlr-chooser README](crates/wlr-chooser/README.md)** — portal setup, options,
  the `wlr-switcher` Alt-Tab/exposé, theming and localisation.
- **[wlr-peek README](crates/wlr-peek/README.md)** — colour picker, loupe, OCR, live
  mirror, change monitor and visual grep.
- **[wlr-shot README](crates/wlr-shot/README.md)** — screenshots, recording and
  timelapse, with system audio and hardware encoding.
- **[wlr-draw README](crates/wlr-draw/README.md)** — the annotation overlay: daemon,
  control socket, tools and example key bindings.
- **[wlr-capture README](crates/wlr-capture/README.md)** — the shared engine.
- **[wlr-i18n README](crates/wlr-i18n/README.md)** — the shared localisation plumbing.

## Contributing

Bug reports, translations and patches welcome — see
[CONTRIBUTING.md](CONTRIBUTING.md). Please keep `cargo fmt`, `cargo clippy` and
`cargo test` clean.

## Contributors

- [Jadi Mirmirani](https://github.com/jadijadi) — graphics tablet (stylus) support in
  wlr-draw ([#9](https://github.com/sjourdois/wlr-utils/pull/9))
- [Ben Reinhold](https://github.com/bR3iN) — faster overlay startup through fontconfig's
  cache ([#10](https://github.com/sjourdois/wlr-utils/pull/10)); most recently used
  window order and reliable quick Alt-Tab in wlr-switcher
  ([#14](https://github.com/sjourdois/wlr-utils/pull/14))

## License

Licensed under either of [Apache-2.0](LICENSE-APACHE) or [MIT](LICENSE-MIT) at your
option.
