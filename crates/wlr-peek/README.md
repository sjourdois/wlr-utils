# wlr-peek

[![CI](https://github.com/sjourdois/wlr-utils/actions/workflows/ci.yml/badge.svg)](https://github.com/sjourdois/wlr-utils/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/wlr-peek.svg)](https://crates.io/crates/wlr-peek)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](#license)

Inspect the screen on wlroots and derivatives. The "look at the screen and extract
something" companion to [`wlr-shot`](../wlr-shot) (which produces image artifacts),
built on the shared [`wlr-capture`](../wlr-capture) engine.

<p align="center">
  <img src="https://raw.githubusercontent.com/sjourdois/wlr-utils/main/docs/assets/wlr-peek/color.gif"
       alt="wlr-peek colour picker: a magnifying loupe and the hex value of the pixel under the crosshair" width="410">
  <img src="https://raw.githubusercontent.com/sjourdois/wlr-utils/main/docs/assets/wlr-peek/loupe.gif"
       alt="wlr-peek loupe: a full-screen magnifier panning and zooming" width="410">
</p>
<p align="center">
  <img src="https://raw.githubusercontent.com/sjourdois/wlr-utils/main/docs/assets/wlr-peek/mirror.png"
       alt="wlr-peek mirror: a floating picture-in-picture window zooming a region of the screen" width="410">
  <img src="https://raw.githubusercontent.com/sjourdois/wlr-utils/main/docs/assets/wlr-peek/cli.gif"
       alt="wlr-peek CLI: ocr and watch running against a page on screen" width="410">
</p>

<p align="center"><sub>📖 See every tool in action on the <a href="https://sjourdois.github.io/wlr-utils/">showcase</a>.</sub></p>

## Install

> **Want the whole suite?** Install the bundle instead — `cargo install wlr-utils` gets
> every tool (`wlr-chooser`, `wlr-switcher`, `wlr-overlayd`, `wlr-peek`, `wlr-shot`,
> `wlr-draw`) in one go. The single-tool install below is the lighter, à-la-carte option.

Building needs the Rust the [main README](../../README.md#from-source) names, and on
Debian/Ubuntu `build-essential pkg-config clang libwayland-dev libxkbcommon-dev
libgbm-dev libtesseract-dev libleptonica-dev` (Arch: `base-devel clang wayland
libxkbcommon mesa tesseract leptonica`).

```sh
cargo install wlr-peek
```

Or build just this binary from the [wlr-utils](../../README.md) workspace:

```sh
cargo build --release -p wlr-peek
```

The default features are `ocr`, `watch`, `gpu` and `i18n`. `ocr` links the system
Tesseract and Leptonica for `ocr` and `grep`, which is what `clang` and their `-dev`
packages are for. `--no-default-features` drops all four, so the binary loses OCR, the
`watch` subcommand, the focus-based sources (`mirror -a`/`--current-output`,
`--follow window`), the dma-buf capture path and the Fluent catalog; pick the ones you
want back with `--features`.

## Requirements

**Works on** — screens and regions on every compositor that captures screens (sway,
Hyprland, niri, labwc, Wayfire, river, dwl, cosmic-comp); windows (`-w`, `--app-id`,
`--title`, `--pick-window`) where windows can be captured (Sway ≥ 1.12, Hyprland ≥ 0.54,
river ≥ 0.4, cosmic-comp, and partly labwc ≥ 0.20 and dwl ≥ 0.9). `-a` and
`--current-output` need sway, Hyprland, cosmic-comp or niri (`--current-output` only).
Not on GNOME or KDE. Details in [COMPATIBILITY.md](../../COMPATIBILITY.md).

- **Screens and regions** (`color`, `loupe`, `region`, screen `mirror`/`watch`) —
  `ext-image-copy-capture-v1` with the output source (**Sway ≥ 1.11 / wlroots ≥ 0.19**),
  or `wlr-screencopy`.
- **Windows** (`-w`, window `mirror`) — the foreign-toplevel source and
  `ext-foreign-toplevel-list-v1` (**Sway ≥ 1.12 / wlroots ≥ 0.20**), which
  `wlr-screencopy` does not stand in for.
- **The clipboard** (`color --clipboard`, `ocr -c`) — `zwlr_data_control_manager_v1`.
- **At run time** — `libegl1`: the frozen overlays (`color`, `loupe`, `region`) render
  through EGL/GLES, on a layer named `wlr-peek` for
  [compositor rules](../../README.md#compositor-rules). `libfontconfig1` looks up the UI
  font when it is there (the embedded fonts otherwise), and `libgbm` serves the zero-copy
  dma-buf path `mirror` and `watch` stream through; one-shot reads (`color`, `ocr`) go
  through shared memory. `--no-gpu` (or `WLR_NO_GPU=1`) forces shared memory everywhere.
- **OCR languages** — the data pack of each language you read: `tesseract-ocr-<lang>` on
  Debian and Ubuntu, `tesseract-data-<lang>` on Arch (`eng` by default; the suite's `.deb`
  recommends it).

`wlr-peek doctor` says what your compositor advertises, and whether the dma-buf path
works here.

## Quick start

```sh
wlr-peek color                                  # pick a colour anywhere → #4D9AFF
wlr-peek ocr                                    # select a region, print its text
wlr-peek loupe                                  # magnify the screen; Esc quits
wlr-peek mirror -s                              # a live picture-in-picture of a region
wlr-peek watch -s && notify-send "it changed"   # wait for a region to change
```

## Subcommands

### `color` — colour picker (pipette)

Freezes every output, shows a magnifying loupe that follows the cursor with the hex
value of the pixel under the crosshair, and picks the pixel you click.

```console
$ wlr-peek color                 # prints e.g. #4D9AFF
$ wlr-peek color --format rgb     # rgb(77, 154, 255)
$ wlr-peek color --clipboard      # copy to the Wayland clipboard instead
```

- Move the cursor; the loupe magnifies the pixels around it.
- **Click** (or **Enter**) to pick the pixel under the crosshair.
- **Esc** to cancel (exit status 1). `Ctrl+[` does the same, here and wherever an
  overlay below takes `Esc`.

`--format hex|rgb|plain` chooses the output. `--clipboard` runs a small background
daemon that serves the colour as text on the wlroots clipboard until replaced.

### `ocr` — recognise text in a region (Tesseract)

Captures a source and runs it through Tesseract, printing the recognised text.

```console
$ wlr-peek ocr                    # select a region interactively (default)
$ wlr-peek ocr -g "100,200 640x480"
$ wlr-peek ocr --active-window     # OCR the area the focused window covers (compositor IPC)
$ wlr-peek ocr --app-id firefox    # OCR a window by app id — even occluded
$ wlr-peek ocr -l fra+eng -c       # French+English, copy to the clipboard
```

With no source flag it selects a region interactively (the default). Other sources
mirror `wlr-shot`: `-g "X,Y WxH"`, `-o NAME`, `-w ID`, `--app-id`/`--title`,
`-a/--active-window`, `--current-output`. `-w`, `--app-id`/`--title` capture the
**window itself**, so they read a window that is occluded or on another workspace —
unlike `-a`, which captures the screen area the focused window occupies.
`-l/--lang` picks the Tesseract language(s) (default `eng`; each needs its data pack,
see [Requirements](#requirements)). `-c` copies the text instead, like
`color --clipboard`; `--clipboard-foreground` keeps that server in the foreground.

### `loupe` — full-screen magnifier

Freezes the screen and magnifies around the cursor; the point under the cursor stays
put as you move, scroll to change the zoom, **Esc** to quit.

```console
$ wlr-peek loupe
```

It is **frozen**, not live: a full-screen *live* magnifier would capture its own
output (a feedback loop), and Wayland does not give a regular client the global
cursor position to follow it from a floating window. For a *live* zoom of a fixed
region, use `mirror -g` (below).

### `mirror` — live mirror (picture-in-picture)

A window of its own that mirrors live content.

```console
$ wlr-peek mirror                  # no source: launch wlr-chooser to pick a window
$ wlr-peek mirror <ID>             # mirror a window (ID as printed by wlr-chooser)
$ wlr-peek mirror --pick-window    # pick a window via the chooser (explicit)
$ wlr-peek mirror -s               # select a region with the mouse, then mirror it
$ wlr-peek mirror --app-id firefox # mirror a window by application id
$ wlr-peek mirror -o DP-4          # mirror a whole output / screen
$ wlr-peek mirror --current-output # the focused output
$ wlr-peek mirror -a               # the active window's area (needs focus info)
$ wlr-peek mirror -g "100,200 640x480" --zoom 4   # a fixed region, magnified
```

Picking a window, with no source or `--pick-window`, runs `wlr-chooser`: it must be on
`PATH`, which a `cargo install wlr-peek` on its own does not provide.

It mirrors a window (`ID` or `-w ID`, `--app-id`/`--title`, or `--pick-window`), or a
region/output as a live loupe (`-s`, `-g "X,Y WxH"`, `-o NAME`, `--current-output`, and
`-a`, the area the focused window covers), magnified by `--zoom` (default ×2).
Region/output mode is mono-output for now (clipped to the output its top-left corner
sits on). Keep the window outside the mirrored region to avoid feedback.

The compositor stacks it like any other window, with the app id `wlr-peek-mirror`. To
keep it floating, on top and on every workspace, in sway:

```
for_window [app_id="wlr-peek-mirror"] floating enable, sticky enable
```

For a region, **`--follow`** chooses what it tracks: `output` (default — shows whatever
workspace is on that screen) or `window` — captures the **window under the region** and
crops to it, so the loupe follows that window across moves and workspaces. Only Sway
says which window is under a region; elsewhere, and with no window there, it falls back
to `output`.

```console
$ wlr-peek mirror -s --follow window   # loupe that sticks to the window you picked
```

Drag to move, the bottom-right grip to resize, the toolbar to collapse to a badge or
close; **Space** freezes, **c** collapses, **+/-** or the wheel set opacity, **r**
re-picks (window mode), **Esc** or **q** closes. One mirror per window: a second one
of the same window exits quietly.

### `region` — select a region/point, print its geometry (slurp replacement)

Reuses the same frozen overlay as `wlr-shot -s` to select a region with the mouse and
print it as `X,Y WxH` (slurp's format) — a native, dependency-free slurp replacement.
Exits 1 if cancelled (Esc).

```console
$ wlr-peek region                  # drag a region → "X,Y WxH"
$ wlr-peek region -p               # pick a point → "X,Y"
$ wlr-peek region -f '%x %y %w %h' # custom format
$ grim -g "$(wlr-peek region)" shot.png        # feed any slurp-compatible tool
```

### `watch` — change monitor

Streams a source and fires when its content **changes**, or once it goes **idle**
(stops changing). Same sources as the other tools: `-s` (interactive region), `-g`,
`-o`, `--current-output`, `-w`/`--app-id`/`--title`/`--pick-window`, `-a` (a region is
single-output, like `mirror`/`record`).

```console
$ wlr-peek watch -s && notify-send "it changed"              # fire once, then notify
$ wlr-peek watch --app-id thunderbird --on idle --for 5s     # wake when the window settles
$ wlr-peek watch -o DP-4 --on change --threshold 2 --repeat --exec 'mpc next'
```

- `--on change` (default) fires when the content changes, printing `change N.N%`;
  `--on idle` fires once it has been stable for `--for` (default `2s`), printing `idle`.
- `--threshold PCT` ignores changes smaller than that percentage of the watched
  pixels (default 0 = any change) — raise it to skip a blinking cursor or clock. It
  also decides what counts as stable for `--on idle`.
- By default it prints one line and exits 0 on the first trigger (composes with
  `&&`); `--repeat` keeps watching and fires every time (with `--on idle`, again
  after each further `--for` of stillness). `--exec CMD` runs a shell command on each
  trigger. `--timeout DUR` gives up (exit 2) after `DUR` without a trigger. If the
  watched window closes or the output goes away, it exits 3.

Capture is damage-driven, so `watch` is cheap: a static source delivers no frames.

### `grep` — find text on screen (visual grep)

OCRs a source and prints where matching text is, in global logical coordinates
(slurp-compatible `X,Y WxH`, so it feeds `mirror`/`shot`/other tools).

```console
$ wlr-peek grep --current-output "TODO"
2741,318 58x19	TODO
$ wlr-peek grep -i error                    # case-insensitive, in a region you select
```

Sources: `-g`, `-o`, `-a`, `--current-output`, or (default) an interactive region.
Matching is a substring of each recognised word, so a pattern with a space matches
nothing (`-i` for case-insensitive, `-l` for the Tesseract language as in `ocr`). Exits 1
when nothing matches — and on a cancelled selection or an error.

`grep` takes no window source: its answer is a position on screen, which a window
capture does not have. Use `ocr --app-id` for a window's text.

### `doctor` and `migrate-config`

`wlr-peek doctor` reports what your compositor supports (see [Requirements](#requirements)).
`wlr-peek migrate-config` moves an old `theme.toml` and `keys.toml` into the suite's
`~/.config/wlr-utils/config.toml`, then deletes them; `migrate-config -` prints the new
file instead.

## Troubleshooting

- **`Tesseract has no language data for …`** — install that language's data pack (see
  [Requirements](#requirements)).
- **`Picking a window needs wlr-chooser…`** — install the `wlr-chooser` crate or the
  suite, or name the window with `-w`, `--app-id` or `--title`.
- **The mirror is tiled with the other windows** — add the `for_window` rule above.
- **`-a` or `--current-output` is unavailable** — they need a focus backend (sway,
  Hyprland, niri for `--current-output`, cosmic-comp); use `-s`, `-g` or `-o` elsewhere.
- **What does my compositor support?** `wlr-peek doctor` says, and its output is what a
  bug report needs.

## Uninstall

```sh
cargo uninstall wlr-peek          # crates.io install
```

wlr-peek writes no state files; `mirror` only leaves a lock file per mirrored window in
`$XDG_RUNTIME_DIR`. It reads the suite's shared `~/.config/wlr-utils/config.toml`, which
`migrate-config` writes; leave it if another tool of the suite still uses it.

## License

MIT OR Apache-2.0.
