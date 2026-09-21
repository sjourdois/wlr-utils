# Screenshot generator

Reproducible, hands-off screenshots and short animations of every wlr-utils
tool, used in the project READMEs and the GitHub Pages showcase.

Each scene spins up an **isolated, headless nested sway** in its own
`XDG_RUNTIME_DIR`, populates a small desktop, drives the tool with a synthetic
pointer + keyboard, and captures the result. The nested compositor uses the
headless wlroots backend (virtual in-memory outputs, no DRM master), so it runs
safely **alongside a live session and never touches your real screens**.

## Usage

```sh
cd tools/screenshots
./capture.sh              # build the tools and regenerate every asset
./capture.sh draw peek    # only the named scenes
SKIP_BUILD=1 ./capture.sh # reuse target/release binaries
```

Output lands in `../../docs/assets/<tool>/` as `*.png` (still) plus `*.mp4` and
`*.gif` (animations) where the scene is dynamic.

To render the overlays in French (or any locale): `SHOTS_LANG=fr_FR.UTF-8 ./capture.sh`.

Animations are recorded with `wlr-shot record`, which emits a constant frame rate,
so they run at the speed the scene was actually driven. The recording is a lossless
master (`SHOTS_CRF`, 0 by default) that both published files are cut from, and which
is then discarded: the GIF encoder crops each frame to the pixels that changed, and
that only pays off when the pixels that did not change are bit-identical. The GIF
keeps every frame of the master — set `SHOTS_GIF_FPS` to resample it to another rate
— and the MP4 is transcoded at `SHOTS_MP4_CRF` (20).

## Requirements

System tools: `sway`, `wtype`, `foot`, `ffmpeg`, `jq`, `curl`, `python3` with
`websockets`, ImageMagick, plus `batcat`/`tree` for the demo windows. The scenes
that show a desktop also need `chromium`, `galculator`, `mpv`, `btop` and `cmatrix`. The
first run also builds a tiny virtual-pointer injector:

```sh
( cd pointer && cargo build --release )
```

## Demo clip

Three windows on the demo desktop move on their own, so the live previews show
as live: a video, a system monitor and a terminal animation.

The video is a few seconds of the Big Buck Bunny trailer, fetched from
`download.blender.org` on first run, checked against a pinned digest, trimmed and
cached in `vendor/` (git-ignored, never committed). `SHOTS_CLIP_START` and
`SHOTS_CLIP_SECONDS` set the excerpt, `SHOTS_MPV_PAUSE=1` freezes it. Without the
download the desktop falls back to a generated test pattern and says so.

> (c) copyright 2008, Blender Foundation / www.bigbuckbunny.org — Creative
> Commons Attribution 3.0.

## Layout

| Path | Role |
|------|------|
| `lib.sh` | nested-compositor lifecycle, input injection, capture helpers |
| `nested-sway.conf` | the isolated compositor's config (one virtual output) |
| `foot.ini` | dark theme for the demo terminals |
| `btop.conf` | btop settings for the demo desktop (graph boxes, no process list) |
| `pointer/` | `shots-pointer`, a `zwlr_virtual_pointer_v1` injector (standalone crate, **not** in the workspace) |
| `cdp.py` | DevTools client that dismisses the browser's cookie dialog by button text |
| `scenes/*.sh` | one scene per tool |
| `capture.sh` | orchestrator: build + run every scene |

## Notes

- **Private runtime directory.** The nested session runs with
  `XDG_RUNTIME_DIR=$XDG_RUNTIME_DIR/wlr-shots`, created mode 0700 and recreated
  empty at each start. The tools derive their runtime paths from that variable —
  `wlr-draw`'s control socket, `wlr-chooser`'s and `wlr-peek`'s single-instance
  locks, the Wayland socket itself — so a capture cannot bind a name the live
  session already holds, nor drive a daemon of yours on the real screen.
  D-Bus, PipeWire and Pulse stay shared and are addressed by their own variables.
- **Why a virtual pointer?** A headless seat has no input devices, so it has no
  pointer capability and sway's `seat cursor` IPC delivers nothing to clients.
  `shots-pointer` creates a real virtual pointer, which the overlays then see.
- **Builds.** One workspace build, with `wlr-capture/gpu` on through feature
  unification. Single captures allocate shm directly, so an overlay tool opens no
  second EGL connection for a dma-buf readback (which fails with
  `eglCreateWindowSurface: BadAlloc`).
- **Scene checks.** The helpers that open a demo window wait for it in the nested
  tree and log `MISSING WINDOW: …` otherwise. `shots_expect_change` brackets an
  action with two cursor-free grabs and logs `NO VISIBLE CHANGE: …` when fewer
  than `SHOTS_CHANGE_MIN` pixels move.
- Run scenes one at a time — they share a fixed nested IPC socket path and must
  not overlap. `capture.sh` already serialises them.
