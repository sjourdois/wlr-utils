# wlr-shot

[![CI](https://github.com/sjourdois/wlr-utils/actions/workflows/ci.yml/badge.svg)](https://github.com/sjourdois/wlr-utils/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/wlr-shot.svg)](https://crates.io/crates/wlr-shot)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](#license)

Screen capture for **wlroots and derivatives**, built on the shared
[`wlr-capture`](../wlr-capture) engine (`ext-image-copy-capture-v1`, correct
strides, occlusion-independent).

It captures an output, the whole layout, a region (interactive `-s` or `-g`/slurp),
or a window — as a screenshot (PNG, JPEG, PPM or PAM) or a recording: H.264 video,
timelapse, or an animated GIF/WebP.

<p align="center">
  <img src="https://raw.githubusercontent.com/sjourdois/wlr-utils/main/docs/assets/wlr-shot/select.gif"
       alt="wlr-shot's interactive region selector: dragging a bright rectangle over a dimmed, frozen screen" width="820">
</p>

<p align="center"><sub>📖 See every tool in action on the <a href="https://sjourdois.github.io/wlr-utils/">showcase</a>.</sub></p>

## Install

> **Want the whole suite?** Install the bundle instead — `cargo install wlr-utils` gets
> every tool (`wlr-chooser`, `wlr-switcher`, `wlr-overlayd`, `wlr-peek`, `wlr-shot`,
> `wlr-draw`) in one go. The single-tool install below is the lighter, à-la-carte option.

Building needs the Rust the [main README](../../README.md#from-source) names, and on
Debian/Ubuntu `build-essential pkg-config clang libwayland-dev libxkbcommon-dev
libgbm-dev libavcodec-dev libavformat-dev libavutil-dev libavfilter-dev libavdevice-dev
libswscale-dev libswresample-dev libva-dev libpipewire-0.3-dev` (Arch: `base-devel clang
wayland libxkbcommon mesa ffmpeg libva libpipewire`).

```sh
cargo install wlr-shot
```

Or build just this binary from the [wlr-utils](../../README.md) workspace:

```sh
cargo build --release -p wlr-shot
```

The FFmpeg and PipeWire packages are for `record`. A screenshots-only binary needs none of
them: `cargo build -p wlr-shot --no-default-features --features i18n`. Recording without
sound drops PipeWire alone: `--no-default-features --features i18n,video,gpu`.

## Requirements

**Works on** — screens and regions on every compositor that captures screens (sway,
Hyprland, niri, labwc, Wayfire, river, dwl, cosmic-comp); windows (`-w`, `--app-id`,
`--title`, `--pick-window`) where windows can be captured (Sway ≥ 1.12, Hyprland ≥ 0.54,
river ≥ 0.4, cosmic-comp, and partly labwc ≥ 0.20 and dwl ≥ 0.9). Not on GNOME or KDE.
Details in [COMPATIBILITY.md](../../COMPATIBILITY.md).

- **Screens and regions** — `ext-image-copy-capture-v1` with the output source (**Sway ≥
  1.11 / wlroots ≥ 0.19**), or `zwlr_screencopy_manager_v1`.
- **Windows** — `ext-foreign-toplevel-list-v1` and the foreign-toplevel capture source
  (**Sway ≥ 1.12 / wlroots ≥ 0.20**), which `wlr-screencopy` does not stand in for.
- **The clipboard** (`-c`) — `zwlr_data_control_manager_v1`. `xdg-output` gives exact
  logical geometry where the compositor has it.
- **At run time** — a working GL stack (`libegl1`): the region selector (`-s`) renders a
  frozen overlay through EGL/GLES, on a layer named `wlr-shot` for
  [compositor rules](../../README.md#compositor-rules). `libfontconfig1` looks up the UI
  font when it is there (the embedded fonts otherwise), and `libgbm` serves the zero-copy
  dma-buf path `record` uses; screenshots go through shared memory. `--no-gpu` (or
  `WLR_NO_GPU=1`) forces shared memory everywhere.
- **Hardware encoding** — NVIDIA's `libnvidia-encode` for NVENC, or a VAAPI driver for
  your GPU; without either, `record` encodes in software.

`wlr-shot doctor` says what your compositor exposes, and whether the dma-buf path works
here.

## Quick start

```sh
wlr-shot screenshot -s shot.png                # drag a region on a frozen screen
wlr-shot screenshot --all shot.png             # every screen, in one image
wlr-shot screenshot -s -c                      # a region, to the clipboard
wlr-shot screenshot --app-id firefox ff.png    # a window by name, even hidden
wlr-shot record -s clip.mp4                    # record a region; Ctrl-C stops
```

With a single screen, a command with no source captures it. With several, name one with
`-o` (`wlr-shot screenshot --list-outputs` lists them), or use `--all`, `-s` or
`--current-output`.

## Usage

```sh
wlr-shot screenshot [-s | -o NAME | --all | -g GEOM | -w ID | --app-id ID | --title TEXT
                     | --pick-window | -a | --current-output]
                    [--cursor] [-t png|jpeg|ppm|pam] [-q QUALITY]
                    [-c [--clipboard-foreground]] [FILE|-]
wlr-shot screenshot --list-outputs | --list-windows
wlr-shot record [-s | -o NAME | -g GEOM | -w ID | --app-id ID | --title TEXT
                 | --pick-window | -a | --current-output]
                [--cursor] [--encoder auto|nvenc|vaapi|software] [--device PATH]
                [--crf N] [--fps N] [--timelapse INTERVAL] [-d SECS]
                [--no-audio | --audio-source NODE] FILE
wlr-shot doctor
wlr-shot migrate-config [-]
```

Source (pick one; with a single screen, that screen by default):

- `-s, --select` — **interactively** drag a region on a frozen overlay (spans all
  outputs; releasing the button takes it, `Esc` or `Ctrl+[` cancels). No external tool
  needed.
- `-o, --output NAME` — a whole output (e.g. `DP-4`).
- `--all` — the whole layout: every output combined into one image.
- `-g, --geometry "X,Y WxH"` — a logical region (the format **slurp** prints),
  stitched across every output it covers. Pairs with slurp:
  `wlr-shot screenshot -g "$(slurp)" shot.png`.
- `-w, --window ID` — a window, by its `ext-foreign-toplevel` identifier (as
  printed by `wlr-chooser` or `--list-windows`).
- `--app-id APP_ID` and/or `--title TEXT` — a window, by application id (exact,
  case-insensitive) and/or a substring of its title (case-insensitive). The two
  combine, and unlike the other sources they are **not** mutually exclusive with
  each other. If more than one window matches, the command fails and lists the
  candidates rather than picking one arbitrarily — narrow it down, or use the
  `-w ID` it prints.
- `--pick-window` — launch `wlr-chooser` to choose the window interactively; it must be
  on `PATH`, which a `cargo install wlr-shot` on its own does not provide.
- `-a, --active-window` — the screen area the focused window covers (a region, not the
  window itself).
- `--current-output` — the focused output.

The last two need the compositor's focus info. Wayland exposes no portable way to
query focus, so these go through a per-compositor backend: **Sway** (`$SWAYSOCK`),
**Hyprland** (`hyprctl`), **niri** (`niri msg`, `--current-output` only: it gives no
window position) and **cosmic-comp** (`zcosmic_toplevel_info_v1`, a Wayland protocol —
COSMIC has no IPC socket).
Elsewhere they error with a hint (use `--pick-window` / `-o NAME` instead).

Contents:

- `--cursor` — composite the mouse cursor into the capture. It is left out by
  default, so a screenshot shows the screen rather than where the pointer happened
  to rest.

Encoding & destination:

- `-t, --type` — `png`, `jpeg`, `ppm` (binary, `P6`) or `pam`. Without it, the file's
  extension picks (`.jpg`/`.jpeg`, `.ppm`, `.pam`, `.png`), and anything else — stdout
  included — is PNG.
- `-q, --quality` — JPEG quality, 1–100 (default 90).
- `-c, --clipboard` — copy to the Wayland clipboard instead of writing a file. A
  small daemon detaches to serve the selection (wlroots `data-control`, the
  protocol `wl-copy` uses) until another client replaces it — the clipboard is
  pull-based, so the data must outlive the command. `--clipboard-foreground` keeps
  it in the foreground (for scripts/debugging). Needs a compositor exposing
  `zwlr_data_control_manager_v1`.
- `FILE` — destination, or `-` for stdout (the default). Ignored with `--clipboard`.
- `--list-outputs` — print `NAME<TAB>WxH+X,Y` (logical geometry) and exit.
- `--list-windows` — print `ID<TAB>APP_ID<TAB>TITLE` for every capturable window
  and exit; the companion to `--app-id`/`--title`.

Because the default `FILE` is `-` (stdout), a screenshot pipes straight into an
annotation editor — **no temp file**. Example **Sway** bindings sending each source
to [satty](https://github.com/gabm/Satty), which saves the result to your Pictures
directory:

```sway
set $pics "$(xdg-user-dir PICTURES)/screenshot-%+.png"
bindsym Print               exec wlr-shot screenshot -s               - | satty -f - -o $pics
bindsym Shift+Print         exec wlr-shot screenshot --active-window  - | satty -f - -o $pics
bindsym Control+Print       exec wlr-shot screenshot --current-output - | satty -f - -o $pics
bindsym Control+Shift+Print exec wlr-shot screenshot --all            - | satty -f - -o $pics
```

Resolution: a whole output, or a region within a **single** output, is captured at
**native (physical) resolution** — so a fractionally-scaled monitor keeps full
pixel detail. A region spanning **several** outputs is composited at logical
resolution.

## Recording (`record`)

Stream a source to a file whose **format follows the extension**: `.mp4`/`.mkv` for
H.264 video, or **`.gif`/`.webp`** for an animated image (downscaled — GIF to 800 px,
WebP to 1280 px on the long side). Keep those to short clips of a **region**: every frame
stays in memory until the file is written, and per-frame GIF quantization on a full 4K
output is slow. The same source flags as `screenshot` apply — `-o`/sole output,
`--current-output`, `-w ID`/`--app-id`/`--title`/`--pick-window`, `-a`, `-g`, and `-s` —
except a region (`-g`, `-s`, `-a`) records a **single** output for now (the one its
top-left corner sits on). Recording a **window** follows it across workspaces and even while
occluded; `--app-id`/`--title` resolve to an identifier once, at start, so a window
opened later can't steal the recording.

```sh
wlr-shot record -o DP-4 out.mp4                 # an output, until Ctrl-C
wlr-shot record --pick-window -d 30 clip.mp4    # a window, 30 seconds
wlr-shot record -s region.mp4                   # a region (single output)
wlr-shot record -s --fps 15 demo.gif            # a region as an animated GIF
wlr-shot record -s demo.webp                    # …or animated WebP (smaller)
wlr-shot record -o DP-4 --crf 18 sharp.mp4      # visually lossless, a bigger file
wlr-shot record -o DP-4 --timelapse 2s day.mp4  # a frame every 2s, played at --fps
wlr-shot record -o DP-4 --no-audio clip.mp4     # video only (audio is on by default)
```

- `--cursor` — composite the mouse cursor into the recording (left out by default).
  A demo of a pointer-driven interaction needs it.
- `--encoder` — `auto` (default) tries hardware (NVENC, then VAAPI), then software
  `libx264`, keeps the first that works on this machine and says which. Force one with
  `nvenc`/`vaapi`/`software`.
- `--device PATH` — DRM render node for VAAPI (default `/dev/dri/renderD128`).
- `--crf N` — constant quality, `0`–`51`: **lower is better and bigger**. `0` is
  lossless, about `18` looks lossless; left out, each encoder keeps its own default.
  VAAPI has no lossless mode and refuses `0`: use `--crf 1`, or `--encoder software`.
  Mind the direction: `screenshot --quality` is a JPEG scale that runs the other way.
- `--fps N` — frame rate (default 30). Capture is damage-driven (a frame only arrives
  when the screen changes), so a normal recording emits a **constant** `--fps`,
  repeating the last frame through static stretches; `--timelapse` instead samples one
  frame per interval and plays them back at `--fps`, so the footage is sped up.
- `--timelapse INTERVAL` — sample one frame every `INTERVAL` (`2s`, `500ms`, `1m`).
- `-d, --duration SECS` — stop automatically; otherwise **Ctrl-C** ends and finalises
  the file. (Recording a window also ends when the window closes.)
- **Audio** — a video recording captures **system audio** (the default sink's monitor)
  as an AAC track by default, via **native PipeWire**. `--no-audio` records silently;
  `--audio-source NODE` captures another node instead, by its name or serial — a
  microphone, for instance (`pactl list short sources` lists the names). If PipeWire
  cannot be reached, the recording goes on without sound and says so. Timelapses and
  GIF/WebP carry no audio.
  For a host with no PipeWire server running, build with `--features audio-fallback`
  to add a **Pulse/ALSA** path (via FFmpeg's libavdevice, no extra system dep), tried
  after PipeWire.

## Troubleshooting

- **`multiple outputs; specify -o NAME among: …`** — with several screens, name one with
  `-o`, or use `--all`, `-s` or `--current-output`.
- **Several windows match `--app-id`/`--title`** — the error lists them; narrow the match,
  or pass the `-w ID` it prints.
- **`-a` or `--current-output` is unavailable** — they need a focus backend (sway,
  Hyprland, niri for `--current-output`, cosmic-comp); use `-s`, `-g` or `-o` elsewhere.
- **`recording without audio (…)`** — PipeWire could not be reached; the video is still
  recorded. Pass `--no-audio` to record silently on purpose.
- **What does my compositor support?** `wlr-shot doctor` says, and its output is what a
  bug report needs.

## Uninstall

```sh
cargo uninstall wlr-shot          # crates.io install
```

wlr-shot writes no state files. It reads the suite's shared
`~/.config/wlr-utils/config.toml`, which `migrate-config` writes from an old
`theme.toml`; leave it if another tool of the suite still uses it.

## License

MIT OR Apache-2.0.
