# wlr-utils

[![CI](https://github.com/sjourdois/wlr-utils/actions/workflows/ci.yml/badge.svg)](https://github.com/sjourdois/wlr-utils/actions/workflows/ci.yml)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](#license)
[![Showcase](https://img.shields.io/badge/▶_showcase-sjourdois.github.io%2Fwlr--utils-8aadf4)](https://sjourdois.github.io/wlr-utils/)

### Capture · switch · inspect · annotate your screen — the native Wayland way.

⚡&nbsp;Zero-copy GPU capture &nbsp;·&nbsp; 👁️&nbsp;Sees occluded & off-workspace windows
&nbsp;·&nbsp; 🦀&nbsp;Rust &nbsp;·&nbsp; 🪟&nbsp;Wayland-native &nbsp;·&nbsp; 🎨&nbsp;Themeable &nbsp;·&nbsp; 🌍&nbsp;13&nbsp;languages

Five graphical tools for **wlroots and derivatives**.

| Tool | What it does | crate |
| --- | --- | --- |
| **[wlr-chooser](crates/wlr-chooser)** | Window & screen picker for screencast portals (`xdg-desktop-portal-wlr`) — a rofi-like overlay with live thumbnails. | [![v](https://img.shields.io/crates/v/wlr-chooser.svg)](https://crates.io/crates/wlr-chooser) |
| **[wlr-switcher](crates/wlr-chooser)** | Live **Alt-Tab / exposé** window switcher (macOS-style strip, full-screen grid, or card) with hold-to-switch and live previews. Ships with `wlr-chooser`, and with `wlr-overlayd` — the optional daemon that puts either overlay on screen in milliseconds. | [![v](https://img.shields.io/crates/v/wlr-chooser.svg)](https://crates.io/crates/wlr-chooser) |
| **[wlr-peek](crates/wlr-peek)** | **Inspect the screen** — colour picker, loupe, OCR, live picture-in-picture **mirror** (window or region), **change monitor** (`watch`), and **visual grep**. | [![v](https://img.shields.io/crates/v/wlr-peek.svg)](https://crates.io/crates/wlr-peek) |
| **[wlr-shot](crates/wlr-shot)** | **Screen capture** — screenshots of an output/region/window (PNG/JPEG/PPM/PAM), copy to clipboard; plus **recording** (MP4/MKV in H.264, or animated GIF/WebP) with **system audio** & **timelapse** (NVENC/VAAPI/libx264). Shoots and records **windows, even those you can't see** — occluded behind others, or on another workspace — by name, with no clicking. | [![v](https://img.shields.io/crates/v/wlr-shot.svg)](https://crates.io/crates/wlr-shot) |
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
| **Overlays** (pickers, region selection, annotation) | any | `wlr-layer-shell` |
| **Screen** capture (screenshots, recording, loupe, wlr-draw's freeze and save) | wlroots ≥ 0.19 · Sway ≥ 1.11, or any compositor with `wlr-screencopy` | `ext-image-copy-capture-v1` (else `wlr-screencopy`) |
| **Window** capture (switcher, `-w`, window mirror/record) | wlroots ≥ 0.20 · Sway ≥ 1.12 | adds the foreign-toplevel capture source and `ext-foreign-toplevel-list-v1` |

Tools degrade gracefully: where windows aren't capturable they keep their screen features
and say so. See [COMPATIBILITY.md](COMPATIBILITY.md) for the full matrix (Hyprland, niri,
labwc, …), or run the `doctor` command that every tool but `wlr-overlayd` exposes (e.g.
`wlr-shot doctor`, or `wlr-chooser --doctor`) to check your own compositor. It also
captures a frame through the GPU path and reports whether it can be imported, which is
the line to quote in a bug report.

Runtime libraries:

| Library | Needed by | Why |
| --- | --- | --- |
| `libegl1` | every tool | EGL/GLES overlay rendering |
| `libfontconfig1` | every tool | looking up the UI font; without it the overlay falls back to the embedded fonts, which have no CJK coverage |
| `libgbm` (Mesa) | every tool | zero-copy GPU capture path; `--no-gpu` (or `WLR_NO_GPU=1`) captures through shared memory instead, but the library must still be there |
| FFmpeg (`libav*`), `libpipewire-0.3` | `wlr-shot record` | encoding, and recording system sound |
| Tesseract, Leptonica, the `eng` tessdata pack | `wlr-peek ocr` / `grep` | text recognition |
| `xdg-desktop-portal-wlr` ≥ 0.8 | `wlr-chooser` | portal-based picking |
| `wlr-foreign-toplevel-management-v1`, or `cosmic-toplevel-management` | `wlr-switcher` | focusing windows |

## Install

Every route below installs the same six binaries (`wlr-chooser`, `wlr-switcher`,
`wlr-overlayd`, `wlr-peek`, `wlr-shot`, `wlr-draw`). The AUR and `.deb` packages also
install the themes and the systemd `--user` units for the `wlr-overlayd` and `wlr-draw`
daemons; enable the one you want with `systemctl --user enable --now wlr-overlayd.service`.

### Arch Linux

```sh
paru -S wlr-utils-bin    # prebuilt from the release (fast)
paru -S wlr-utils        # …or built from source
```

Both ship the same tools and conflict with each other — pick one. Any AUR helper works.

### Debian / Ubuntu

One package with the whole suite is attached to every
[release](https://github.com/sjourdois/wlr-utils/releases/latest), built **per distro** so
it links against that distro's FFmpeg / Leptonica. Pick the matching asset (the forky,
sid and 26.04 builds are experimental, and a release may lack them):

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
`wlr-shot record`, and a source build there hits the same limit.

> [!IMPORTANT]
> These `.deb`s link **dynamically** against the FFmpeg (`libavutil`) and Leptonica
> (`libleptonica`, formerly `liblept`) of the distro they were built on. If your installed
> versions don't match (different release, backports, a soname your distro doesn't ship),
> the package won't install or the tool won't start —
> `error while loading shared libraries: libavutil.so.NN`. Build from source instead: it
> links against whatever you have.

### NixOS / Nix

Packaged in [nixpkgs](https://github.com/NixOS/nixpkgs/tree/master/pkgs/by-name/wl/wlr-utils),
on the unstable channel.

```sh
nix-shell -p wlr-utils                 # try it
nix profile install nixpkgs#wlr-utils  # …or install it
```

On NixOS, add `wlr-utils` to `environment.systemPackages`.

### Prebuilt binaries

One archive with every binary, no Rust toolchain needed. It is built on Ubuntu 24.04 and linked to its FFmpeg, PipeWire and Leptonica, so elsewhere it
needs those same versions — a package or a source build is the safer route:

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
cargo install wlr-chooser        # window/screen picker + wlr-switcher (Alt-Tab/exposé) + wlr-overlayd
cargo install wlr-peek           # colour picker, loupe, OCR, live mirror, watch
cargo install wlr-shot           # screenshots + recording
cargo install wlr-draw           # annotation overlay
```

To install a checkout with its systemd units and themes, build the bundle and run the
install script. It installs into `/usr/local`; `PREFIX` and `DESTDIR` change that, as the
AUR packages do:

```sh
cargo build --release -p wlr-utils
sudo sh packaging/install.sh
```

Either route needs the development packages of the features you build; for the whole
suite, the ones CI installs:

```sh
# Debian / Ubuntu
sudo apt install clang libwayland-dev libxkbcommon-dev libfontconfig-dev libgbm-dev \
  libavcodec-dev libavformat-dev libavutil-dev libavfilter-dev libavdevice-dev \
  libswscale-dev libswresample-dev libva-dev libpipewire-0.3-dev \
  libtesseract-dev libleptonica-dev
# Arch
sudo pacman -S --needed cargo clang wayland libxkbcommon fontconfig mesa ffmpeg libva \
  libpipewire tesseract leptonica
```

Each tool's README says which of these its features pull in.

### Uninstall

Package installs come off the usual way (`paru -R wlr-utils-bin`, `sudo apt remove wlr-utils`);
run `systemctl --user disable --now` first on any unit you enabled.
A `cargo install` drops the binaries in `~/.cargo/bin`; remove the bundle with
`cargo uninstall wlr-utils`, or an individual tool the same way:

```sh
cargo uninstall wlr-utils        # the whole bundle
cargo uninstall wlr-draw         # …or just one: wlr-chooser / wlr-peek / wlr-shot
```

What `packaging/install.sh` installed comes off by hand, from the same `PREFIX`:

```sh
sudo rm -rf /usr/local/bin/wlr-{chooser,switcher,overlayd,peek,shot,draw} \
  /usr/local/lib/systemd/user/wlr-{overlayd,draw}.service \
  /usr/local/share/{wlr-utils,doc/wlr-utils,licenses/wlr-utils}
```

The prebuilt installer also puts the binaries in `~/.cargo/bin`, but cargo does not know
them, and it leaves a receipt; on Nix, remove the profile entry:

```sh
rm -f ~/.cargo/bin/{wlr-chooser,wlr-switcher,wlr-overlayd,wlr-peek,wlr-shot,wlr-draw} \
      ~/.config/wlr-utils/wlr-utils-receipt.json
nix profile remove wlr-utils
```

`wlr-draw` also registers an XDG autostart entry on first run (see its README). Drop the
checkbox in its tray menu, or delete the files by hand (honouring `$XDG_CONFIG_HOME` /
`$XDG_STATE_HOME` if you set them):

```sh
rm -f ~/.config/autostart/wlr-draw.desktop \
      ~/.local/state/wlr-draw/autostart-initialized
# and, if you enabled the systemd unit:
systemctl --user disable --now wlr-draw.service
rm -f ~/.config/systemd/user/wlr-draw.service   # only if you copied it there
```

If you started `wlr-overlayd` from its systemd unit, take that one out the same way —
otherwise it is only a line in your compositor's config:

```sh
systemctl --user disable --now wlr-overlayd.service
rm -f ~/.config/systemd/user/wlr-overlayd.service   # only if you copied it there
```

## Configuration

Every tool reads one file, `~/.config/wlr-utils/config.toml` (`$XDG_CONFIG_HOME` is
honoured). `~/.wlr-utils.toml` works too, and `/etc/xdg/wlr-utils/config.toml` is the
system's default: the first one found is the only one read. Nothing in it is required.
A mistake is reported on stderr and by `doctor`, and the rest still applies.

```toml
[theme]
name = "catppuccin-mocha"   # a theme, by name or by path
accent = "#89b4fa"          # …and any of its keys, set over it
corner-radius = 0           # square corners

[chooser]
tile-width = 360            # the overlays' sizes

[draw]
dwell-ms = 400

[draw.keys]
undo = ["u", "z"]
```

[`docs/config.toml`](docs/config.toml) lists every setting, commented out at its default:
copy it and uncomment what you change. Packages install it in `/usr/share/doc/wlr-utils`.
They install the themes in
`/usr/share/wlr-utils/themes`, named for `name` as `catppuccin-mocha`,
`catppuccin-macchiato`, `catppuccin-frappe`, `catppuccin-latte`, `nord`, `gruvbox-dark`,
`dracula` and `tokyo-night`. Yours go in `~/.config/wlr-utils/themes`, and that is also
where to copy [`docs/themes`](docs/themes) after a `cargo install`, which installs none. A
`name` holding a `/` is a path instead, relative to the folder of `config.toml` (`~` is
not expanded).

Until wlr-utils 2.0, the old `~/.config/wlr-chooser/theme.toml` and
`~/.config/wlr-draw/keys.toml` are still read when there is no `config.toml`, with a
warning. One command moves them in, comments included, then deletes them; with `-`, it
prints the new file instead and touches nothing, for a configuration kept in a dotfile
manager:

```sh
wlr-draw migrate-config     # also wlr-shot and wlr-peek, or wlr-chooser/wlr-switcher --migrate-config
wlr-draw migrate-config -
```

## Compositor rules

Each overlay is a `wlr-layer-shell` surface named after the command that shows it, so a
layer rule (blur, animation, …) can target one tool:

| Namespace | Overlay |
| --- | --- |
| `wlr-chooser` | the source picker, also when `wlr-shot` or `wlr-peek` asks it for a window |
| `wlr-switcher` | the window switcher and its exposé |
| `wlr-draw` | the annotation overlay |
| `wlr-shot` | region selection |
| `wlr-peek` | the colour picker, the loupe and region selection |

`wlr-peek mirror` is an ordinary window instead, with the app id `wlr-peek-mirror` for
window rules.

## Documentation

- **[wlr-chooser README](crates/wlr-chooser/README.md)** — portal setup, options,
  the `wlr-switcher` Alt-Tab/exposé, the `wlr-overlayd` daemon, theming and
  localisation.
- **[wlr-peek README](crates/wlr-peek/README.md)** — colour picker, loupe, OCR, live
  mirror, change monitor and visual grep.
- **[wlr-shot README](crates/wlr-shot/README.md)** — screenshots, recording and
  timelapse, with system audio and hardware encoding.
- **[wlr-draw README](crates/wlr-draw/README.md)** — the annotation overlay: daemon,
  control socket, tools and example key bindings.
- **[wlr-capture README](crates/wlr-capture/README.md)** — the shared engine.
- **[wlr-config README](crates/wlr-config/README.md)** — the configuration, its themes
  and the migration from the old files.
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
  ([#14](https://github.com/sjourdois/wlr-utils/pull/14)); overlays that set their own
  cursor, so one hidden by another window shows again
  ([#16](https://github.com/sjourdois/wlr-utils/pull/16)); sway's scratchpad, custom
  cycle keys and one-window auto-select in wlr-switcher
  ([#20](https://github.com/sjourdois/wlr-utils/pull/20)); `--move` in wlr-switcher
  ([#21](https://github.com/sjourdois/wlr-utils/pull/21))

## License

Licensed under either of [Apache-2.0](LICENSE-APACHE) or [MIT](LICENSE-MIT) at your
option.
