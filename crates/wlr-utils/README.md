# wlr-utils

[![CI](https://github.com/sjourdois/wlr-utils/actions/workflows/ci.yml/badge.svg)](https://github.com/sjourdois/wlr-utils/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/wlr-utils.svg)](https://crates.io/crates/wlr-utils)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](#license)

The **[wlr-utils](https://github.com/sjourdois/wlr-utils)** suite in a single install —
five screen tools for **wlroots and derivatives**, all sharing one capture engine.

```sh
cargo install wlr-utils
```

It needs Rust 1.95 or newer and the development packages the
[main README](https://github.com/sjourdois/wlr-utils#from-source) lists. This installs
every binary at once:

| Binary | What it does |
| --- | --- |
| `wlr-chooser` | Window & screen picker for `xdg-desktop-portal-wlr` (rofi-like, live thumbnails). |
| `wlr-switcher` | Alt-Tab / exposé window switcher with live previews. |
| `wlr-overlayd` | Optional daemon that keeps the switcher's and the chooser's overlay warm, so it appears at once. |
| `wlr-peek` | Inspect the screen — colour picker, loupe, OCR, live mirror, change monitor, visual grep. |
| `wlr-shot` | Screenshots (PNG/JPEG/PPM/PAM) and recording (MP4/MKV in H.264, GIF/WebP) with system audio. |
| `wlr-draw` | Draw on screen — annotation overlay with shapes, text, spotlight, freeze-frame. |

Then the [first steps](https://github.com/sjourdois/wlr-utils#first-steps): five commands
to try, and the key bindings.

This crate is just a **bundle**: it ships no library and no logic of its own, only thin
binaries that re-export each tool. It is the full-featured build — GPU capture, OCR,
video and audio recording, tray — so it needs all of their system dependencies
(GPU/`libgbm`, FFmpeg, PipeWire, Tesseract). `--no-default-features` drops
`wlr-shot record` and with it the FFmpeg/PipeWire link. For a lighter, single-purpose
install, install a tool on its own — `cargo install wlr-shot` — and read that crate's
README for its exact requirements.

## Prebuilt bundle

Every [release](https://github.com/sjourdois/wlr-utils/releases/latest) ships one archive
containing all the binaries, plus a one-line installer. The archive is built on Ubuntu
24.04 and linked to its FFmpeg, PipeWire and Leptonica, so elsewhere it needs those same
versions — a package or a source build is the safer route.

The installer puts the binaries in `$CARGO_HOME/bin` (`~/.cargo/bin`) and adds that
folder to your `PATH` in your shell's profile, unless `WLR_UTILS_NO_MODIFY_PATH=1` is set;
it installs no theme and no systemd unit:

```sh
curl --proto '=https' --tlsv1.2 -LsSf \
  https://github.com/sjourdois/wlr-utils/releases/latest/download/wlr-utils-installer.sh | sh
```

The archive, unpacked by hand, installs itself with its systemd units, themes and example
configuration, into `/usr/local`:

```sh
curl -LO https://github.com/sjourdois/wlr-utils/releases/latest/download/wlr-utils-x86_64-unknown-linux-gnu.tar.xz
tar xf wlr-utils-x86_64-unknown-linux-gnu.tar.xz
cd wlr-utils-x86_64-unknown-linux-gnu
sudo sh install.sh                 # or elsewhere: sudo env PREFIX=/usr sh install.sh
# or for yourself alone, without root: PREFIX=~/.local sh install.sh
```

## Uninstall

```sh
cargo uninstall wlr-utils
```

The prebuilt installer's binaries are not cargo's to remove: delete them from
`~/.cargo/bin`, with the receipt it leaves, `~/.config/wlr-utils/wlr-utils-receipt.json`.
What `install.sh` installed comes off by hand, from the same `PREFIX` — the main README
[lists the paths](https://github.com/sjourdois/wlr-utils#uninstall).

`wlr-draw` registers an XDG autostart entry on first run — see the
[main README](https://github.com/sjourdois/wlr-utils#uninstall) for the leftover files to
remove.

## Documentation

Per-tool docs, requirements and options live in each crate's README:
[wlr-chooser](https://github.com/sjourdois/wlr-utils/tree/main/crates/wlr-chooser) ·
[wlr-peek](https://github.com/sjourdois/wlr-utils/tree/main/crates/wlr-peek) ·
[wlr-shot](https://github.com/sjourdois/wlr-utils/tree/main/crates/wlr-shot) ·
[wlr-draw](https://github.com/sjourdois/wlr-utils/tree/main/crates/wlr-draw).

## License

MIT OR Apache-2.0.
