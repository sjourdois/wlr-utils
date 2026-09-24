# Compositor compatibility

`wlr-utils` is built on a handful of Wayland protocols. A compositor that
advertises them works; one that doesn't, doesn't (there is no portal fallback).
The quickest way to check your own compositor is the `doctor` command, which
**every tool** exposes — a `doctor` subcommand on `wlr-peek` / `wlr-shot` / `wlr-draw`,
and a `--doctor` flag on `wlr-chooser` / `wlr-switcher`:

```console
$ wlr-peek doctor
```

It prints your tool version, OS, compositor + version, which of the protocols below
the running compositor advertises, which capture protocol the engine uses there, and
whether screen capture and focus-aware sources will work — so it doubles as the
environment block a bug report needs. Any tool prints the same report, so a
single-tool install can produce it too.

## Protocols used

| Protocol | Used for | Needed by |
| --- | --- | --- |
| `ext-image-copy-capture-v1` + `ext-image-capture-source-v1` + the output / foreign-toplevel source managers | the **capture engine** (frames of an output or a window) | everything |
| `wlr-screencopy` (`zwlr_screencopy_manager_v1`, v3) | the capture engine's fallback, **outputs only**, used when `ext-image-copy-capture-v1` is absent | screen capture on a compositor without the `ext` protocols |
| `ext-foreign-toplevel-list-v1` | enumerating windows | `wlr-chooser`, `-w`, `wlr-peek mirror`, window record/watch |
| `wlr-foreign-toplevel-management` (`zwlr_foreign_toplevel_manager_v1`) | focusing the picked window, and reading which window is focused (its `activated` state) | `wlr-switcher` |
| `cosmic-toplevel-management` (`zcosmic_toplevel_manager_v1`) + `cosmic-toplevel-info` v2+ | focusing the picked window on COSMIC, which has no `wlr-foreign-toplevel-management` | `wlr-switcher` on cosmic-comp |
| `wlr-layer-shell` (`zwlr_layer_shell_v1`) | full-screen overlays | the region selector (`-s`), `wlr-peek loupe`/`color`, `wlr-switcher`, `wlr-chooser`, `wlr-draw` |
| `wlr-data-control` (`zwlr_data_control_manager_v1`) | clipboard copy | `-c`/`--clipboard` |
| `keyboard-shortcuts-inhibit` (`zwp_keyboard_shortcuts_inhibit_manager_v1`) | grabbing keys under a layer-shell grab | `wlr-switcher` (so `Alt+Tab` reaches it) |
| `linux-dmabuf` (`zwp_linux_dmabuf_v1`) | zero-copy GPU capture (CPU `wl_shm` is the fallback) | live previews: `wlr-chooser`, `wlr-switcher`, `wlr-peek mirror`, `wlr-shot record` |
| `xdg-output` (`zxdg_output_manager_v1`) | accurate logical geometry (fractional scale, positions) | recommended; falls back to `wl_output` |
| `cursor-shape-v1` (`wp_cursor_shape_manager_v1`) | an overlay setting its own cursor | recommended, every overlay; without it the overlay shows whatever cursor the last client left — none, if that one hid it |
| `tablet-v2` (`zwp_tablet_manager_v2`) | graphics tablet (stylus) input | optional, `wlr-draw`; without it, mouse only |
| compositor IPC, or `cosmic-toplevel-info` (`zcosmic_toplevel_info_v1`, v2+) on COSMIC | "the active window" / "the current output" (`-a`, `--current-output`); an IPC also names the process behind a window (`--pid`) and, on sway, the windows in its scratchpad (`--scratchpad`), which no Wayland protocol does | a per-compositor focus backend |

The engine drives `ext-image-copy-capture-v1` where it is available, and
`wlr-screencopy` otherwise. `ext-image-capture-source-v1` landed in two steps: the base
protocol plus the **output** source arrived in **wlroots 0.19** (Sway ≥ 1.11), while the
**foreign-toplevel** source (`ext_foreign_toplevel_image_capture_source_manager_v1`) —
which window capture depends on — only arrived in **wlroots 0.20** (Sway ≥ 1.12).

So there are **two floors**:

- **Screen capture** — `ext-image-copy-capture-v1` + the **output** source (**wlroots ≥ 0.19
  / Sway ≥ 1.11**), or `wlr-screencopy` v3. Screenshots, recording, the loupe/colour
  picker, region select, and wlr-draw's freeze & save work here.
- **Window capture** — `ext-image-copy-capture-v1` with the **foreign-toplevel** source +
  list: **wlroots ≥ 0.20 / Sway ≥ 1.12**. `wlr-screencopy` addresses a `wl_output` and
  never a window, so it does not lift this floor. The Alt-Tab switcher,
  `-w`/`--pick-window`, and per-window mirror/record need it.

The tools **degrade gracefully**: where only screen capture is available the screen
features all work, while window-only paths fail with a clear message (`wlr-switcher` says so
and exits instead of showing an empty overlay; wlr-draw hides freeze/save when even screen
capture is missing). Run `wlr-peek doctor` to see which of the two your compositor offers.

On a compositor that advertises both capture protocols, `WLR_FORCE_SCREENCOPY=1` makes
the engine take the `wlr-screencopy` path.

### GPU capture

Live previews allocate their capture buffer through gbm and import it as a GL
texture, so no frame is copied through the CPU. Which DRM format modifier the
driver picks decides the buffer's layout — Intel and AMD hand out compressed
layouts with extra auxiliary planes, other drivers a single plane.

If a driver refuses to import its own buffer, captures do not fail: the tools drop
to shared memory and say so once. `--no-gpu` (or `WLR_NO_GPU=1`) forces that path
from the start, and `doctor` reports the negotiated fourcc, modifier and plane
count — quote its `GPU capture:` line in a bug report.

Single captures (screenshots, colour picks, OCR) always use shared memory: they end
up as CPU pixels anyway, so a GPU round trip would only cost an EGL context. An
output declaring a `wl_output` transform — a rotated or mirrored screen — captures
through shared memory too, so its frames come out upright.

## Compositors

The matrix below tracks the two capture floors plus `wlr-layer-shell` (needed by every
overlay: the switcher, region selector, loupe/colour picker and wlr-draw) and the focus IPC
backend (for `-a` / `--current-output`). Run `wlr-peek doctor` to check your own.

| Compositor | Screen capture | Window capture | Overlays (layer-shell) | Focus IPC |
| --- | --- | --- | --- | --- |
| **Sway** | ✅ ≥ 1.11 (wlroots 0.19) | ✅ ≥ 1.12 (wlroots 0.20) | ✅ | ✅ `$SWAYSOCK` (MRU, pid, scratchpad) |
| **Hyprland** | ✅ ≥ v0.54 | ✅ ≥ v0.54 | ✅ | ✅ `hyprctl` (MRU, pid) |
| **labwc** | ✅ ≥ 0.9 (wlroots 0.19) | 🟡 ≥ 0.20 (partial) | ✅ | ❌ |
| **cosmic-comp** | ✅ | ✅ | ✅ | ✅ `zcosmic_toplevel_info_v1` |
| **Wayfire** | ✅ ≥ 0.10 (wlroots 0.19) | ❌ (0.11 is on wlroots 0.20 but ships no window source) | ✅ | ❌ |
| **river** | ✅ ≥ 0.3 (wlroots 0.19) | ✅ ≥ 0.4 | ✅ | ❌ |
| **niri** | ✅ (`wlr-screencopy`) | ❌ | ✅ | 🟡 `niri msg` (MRU, pid, `-a` n/a) |
| **dwl** | ✅ (`wlr-screencopy`; `ext` ≥ 0.9) | 🟡 ≥ 0.9 (no focusing) | ✅ | ❌ |
| **Mutter** (GNOME) | ❌ | ❌ | ❌ | ❌ |
| **KWin** (KDE) | ❌ | ❌ | ✅ | ❌ |

✅ full · 🟡 partial · ❌ none. "MRU" marks a backend that also reports the window focus
history, for `--window-order mru`; "pid" one that names the process behind a window, for
`--pid`; "scratchpad" one that reports the windows kept aside, for `--scratchpad`.
Versions are from each project's release notes / merge requests (the per-interface
numbers on wayland.app are unreliable snapshots).

Tested on **Sway** ≥ 1.12 (the development compositor), **Hyprland 0.56.2**,
**niri 26.04**, **KWin 6.7.5** and **Mutter 50.5**. On **cosmic-comp 1.8.0** the
advertised protocols, the focus backend and `wlr-switcher`'s focus change were checked in
a software-rendered virtual machine, where frame capture could not be exercised. The other
rows are untested: they were checked against each project's latest release and its
source on 2026-09-24.

Three caveats:

- **Mutter / KWin** — unsupported: neither exposes a capture protocol. `wlr-draw` does run
  on KWin, without its freeze and save.
- **niri** (26.04) exposes `wlr-screencopy` and none of the `ext` capture protocols, so
  the screen features work there and the window features do not. So did **dwl** before
  0.9; from 0.9 it captures windows too, but has no `wlr-foreign-toplevel-management`,
  so `wlr-switcher` previews the windows and cannot focus the one picked.
- **cosmic-comp** exposes no `wlr-foreign-toplevel-management`. `wlr-switcher` focuses the
  picked window through `cosmic-toplevel-management` there, and addresses it by its
  `ext-foreign-toplevel-list-v1` identifier.

Two things vary by compositor:

- **Focus-aware sources** — `-a` (active window) and `--current-output` need a
  per-compositor backend (see below). Backends ship for **Sway** (its IPC socket),
  **Hyprland** (`hyprctl`), **niri** (`niri msg`) and **cosmic-comp**
  (`zcosmic_toplevel_info_v1`, a Wayland protocol — COSMIC has no IPC socket). Without
  a backend, every *other* source still works: `-s` interactive select, `-g` geometry,
  `-o NAME`, `-w ID`, `--pick-window`. (niri exposes no per-window global rectangle, so
  its `-a` is unavailable — use `-g` / `--current-output`.) Ordering windows most
  recently focused first (`--window-order mru`) needs one too, and three of the four
  provide it: Sway from its tree's `focus` arrays, Hyprland from `focusHistoryID`, niri
  from `focus_timestamp`. COSMIC reports no focus history. Elsewhere, and there,
  windows are ordered by name.

  **Filtering the switcher by process** (`--pid`) needs one too, and the same three
  provide it: no Wayland protocol carries a pid, so the process behind a window comes
  from the compositor — Sway's tree, `hyprctl clients`, `niri msg windows`.
  `zcosmic_toplevel_info_v1` names no process, so COSMIC cannot answer. Without it
  `--pid` says so and exits; `--app-id` and `--title` need nothing of the sort and work
  wherever windows can be listed.

  **The scratchpad** (`--scratchpad`) is Sway's: the switcher reads it from Sway's tree
  and puts windows back through its IPC. Elsewhere the flag says so and exits.

  **Which tile `wlr-switcher` starts on** needs no backend: the window you are on comes
  from `wlr-foreign-toplevel-management`'s `activated` state. A quick `Alt+Tab` therefore
  switches away from that window, in either window order. cosmic-comp does not expose that
  protocol, so the switcher there starts on the first tile instead.
- **Zero-copy GPU capture** (`linux-dmabuf`) is optional; the CPU `wl_shm` path is the
  universal fallback.

> [!NOTE]
> **Help wanted.** If you run wlr-utils on Hyprland, niri, river, Wayfire, cosmic-comp or
> any other wlroots compositor, please report how it goes — run `wlr-peek doctor` and
> open an issue with the output. Validation reports (and focus backends for more
> compositors) are very welcome.

## Adding a compositor

Focus backends live in [`crates/wlr-capture/src/focus.rs`](crates/wlr-capture/src/focus.rs):
implement `FocusBackend` (a `focused_output()` and an `active_window_rect()`, plus an
optional `focus_order()` for `--window-order mru` and `window_pids()` for `--pid`) over
your compositor's IPC and add a detection branch in `detect()`. Both optional methods
key their answer by the `ext-foreign-toplevel-list-v1` identifier, which is what the
capture engine names a window by. The Sway, Hyprland and niri backends are short worked
examples; the cosmic-comp one shows the same trait over a Wayland protocol instead of a
socket.
