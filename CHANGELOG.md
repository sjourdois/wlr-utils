# Changelog

All notable changes to this project are documented here. The format is based on
[Keep a Changelog](https://keepachangelog.com/), and this project adheres to
[Semantic Versioning](https://semver.org/).

## 1.11.1 — 2026-10-07

### Fixed

- **The daemons no longer go deaf after a suspend** — once the computer had slept,
  `wlr-overlayd` and `wlr-draw` showed their overlay but took no key and no click,
  holding the keyboard until they were killed. They now take the keyboard and the
  pointer back whenever the compositor hands them out again; the `wlr-peek` mirror
  and the region picker too.
- **`wlr-switcher` and `wlr-chooser` draw once a frame** — they drew three times, and
  once more with every screen change while they were up: a dozen times a frame after
  a monitor woke, a quarter of a CPU core for as long as the overlay stayed open.
- **An old `theme.toml` link to a moved theme still applies it** — a link left
  pointing at a theme's old place gave "cannot read", then the offer to migrate, and
  the default theme. It now applies the installed theme of that name, as the migration
  does; a link to nothing is said once, without the offer the migration would refuse.
- **`wlr-draw` still starts on login after a Nix upgrade** — its autostart entry
  named the binary's store path, gone once the old generation was collected. It now
  names the `wlr-draw` on `PATH` that leads to it, such as the profile's.

## 1.11.0 — 2026-10-06

### Added

- **`--move` on `wlr-switcher`**
  ([#21](https://github.com/sjourdois/wlr-utils/pull/21), by
  [@bR3iN](https://github.com/bR3iN)) — brings the picked window onto the current
  workspace, on sway, Hyprland and cosmic-comp.
- **One `config.toml` for every tool** — `~/.config/wlr-utils/config.toml` holds the
  theme in `[theme]`, by `name` and with any of its keys set over it, and `wlr-draw`'s
  settings in `[draw]` and `[draw.keys]`; the packages now ship the themes. A mistake
  is named, with its file and key, on stderr and by `doctor`.
- **Square corners, and the overlays' sizes**
  ([#23](https://github.com/sjourdois/wlr-utils/issues/23), requested by
  [@zlokquay](https://github.com/zlokquay)) — `corner-radius` in
  `[theme]` rounds every corner in proportion, `0` squaring them all. `[chooser]` sets
  the card's size, the tiles' width, the spacing and the Alt-Tab row's size.
- **Mouse buttons in `wlr-draw`'s bindings**
  ([#25](https://github.com/sjourdois/wlr-utils/issues/25), requested by
  [@algmyr](https://github.com/algmyr)) — `button2`, `button8`, `button9` or a `BTN_`
  name, as in sway. The left and right buttons stay for drawing and moving.
- **`migrate-config`** — moves the old `theme.toml` and `keys.toml` into `config.toml`,
  comments included, then deletes them.
- **`wlr-draw reload`** — re-reads the configuration and keeps the drawing, however the
  daemon was started, and prints what it could not apply.
- **The systemd units from the binaries** — `wlr-draw print-unit` and
  `wlr-overlayd --print-unit` print them, for an install that did not put them in place.
- **The release archive installs itself** — `sudo sh install.sh` puts the binaries,
  the systemd units, the themes and the example configuration in place.

### Changed

- **Every overlay follows the theme** — the region selection, the colour picker, the
  loupe, `wlr-draw`'s status, palette and help, and the `wlr-peek` mirror take the
  theme's colours, font and corners, and every text scales with `font-size`. The first
  three were black and white, in fixed sizes. `wlr-draw`'s help shrinks to fit a short
  screen.

### Fixed

- **`wlr-peek ocr` says what to install** — a missing language names its package, and
  the `.deb` now recommends the English data, without which OCR could not start.
- **A missing `wlr-chooser` is named** — `wlr-peek mirror` and `--pick-window` exited
  without a word, or with "launching wlr-chooser", when it was not installed.
- **The hints point at the right `doctor`** — `wlr-chooser` and `wlr-switcher` sent you
  to `wlr-peek doctor`, which an install of `wlr-chooser` alone does not have.
- **`wlr-switcher` opens past the current window in every layout** — outside
  hold-to-switch (`--layout grid` or `card`, `--no-hold`), it opened on the window you
  were on.
- **Less CPU for live thumbnails without the GPU path** — with `--no-gpu`, or where the
  GPU path is unavailable, `wlr-chooser` and `wlr-switcher` refreshed them at about
  30 fps. They now stay at about 6 fps.
- **Screens carry the screen icon in `wlr-chooser --layout grid`** — they had the
  window one.
- **`wlr-chooser --grid` fits its rows whatever the font** — with a larger `font-size`
  in the theme, the last row was cut off, and with the default font the card ended in
  empty space.
- **A non-ASCII colour in `theme.toml` no longer crashes the overlays** — a value such
  as `"#éa"` made the theme loader panic, taking down `wlr-chooser`, `wlr-switcher`,
  `wlr-draw`, the `wlr-peek` mirror and the `wlr-overlayd` daemon. It is now ignored
  like any other colour that does not parse.
- **Killing `wlr-switcher` or `wlr-chooser` now closes the `wlr-overlayd` overlay** —
  the daemon kept an overlay up, keyboard held, after the invocation that asked for
  it was gone, and answered every new one `busy`. It now takes the overlay down as
  soon as its client hangs up.
- **`wlr-draw save rel.png` writes to your current directory** — a relative path was
  resolved from the daemon's.
- **A list for a held control no longer discards all of `wlr-draw`'s bindings** —
  `passthrough`, `constrain`, `spotlight` and `snap-invert` now take a list like the
  other bindings, and any of its keys or modifiers engages the control.
- **A key or modifier rebound in `wlr-draw` goes where you put it** —
  `pen = "r"` left `r` on the rectangle and the pen with no key, and
  `passthrough = "alt"` shared Alt with snap-invert. What had it by default now gives it
  up, and the help shows it as unassigned if nothing is left.
- **`systemctl --user reload wlr-draw` reloads instead of stopping the daemon** — it
  now re-reads the configuration and keeps the drawing.
- **`wlr-draw`'s on-screen hints name your keys** — after rebinding help, click-through
  or spotlight, the hints still showed the default keys.
- **The spotlight status shows its ○ mark** — it was an empty box.
- **A non-ASCII colour no longer crashes `wlr-draw`** — `wlr-draw color '#aébcd'`
  panicked instead of reporting an unknown colour, and the same `color` line sent by
  hand to the daemon's socket killed the thread that reads it: the overlay stayed up
  but took no more commands. Both now report it as an unknown colour.
- **`wlr-shot record --timelapse` no longer captures sound it cannot use** — it ran a
  PipeWire capture and announced `+ audio`, but a timelapse has no sound track.
- **`wlr-shot screenshot -t ppm` writes PPM** — it wrote PAM, which is now `-t pam`.
- **`wlr-shot screenshot shot.jpg` writes a JPEG** — without `-t`, the format now
  follows the file's extension; it was always PNG.
- **`wlr-shot record` falls back when hardware encoding is unavailable** — `--encoder
  auto` picked NVENC whenever FFmpeg was built with it, as Debian's and Arch's are, and
  failed on machines without NVIDIA. It now tries VAAPI, then `libx264`.
- **`wlr-peek watch --repeat --timeout` waits for a quiet stretch** — the timeout ran
  from the start and exited 2, "no trigger", however often it had fired; it now restarts
  at each trigger, as its help says.
- **`wlr-peek watch` exits 3 when its window or output goes away** — it exited 0, so
  `watch … && notify-send` fired when the watched window closed.
- **The systemd units start a `cargo install`ed daemon** — systemd looked `wlr-draw` and
  `wlr-overlayd` up in `/usr/bin` and `/usr/local/bin` only; the units now go through the
  user manager's `PATH`.
- **A mistake in the theme or in `wlr-draw`'s bindings is reported** — an unreadable
  theme file, a value that is no colour or a misspelled entry was ignored without a word.
- **Translated messages no longer wrap the values they insert in bidi isolation marks**
  — every language had them, English included.

### Deprecated

- **`~/.config/wlr-chooser/theme.toml` and `~/.config/wlr-draw/keys.toml`** — still read,
  with a warning, when there is no `config.toml`, until 2.0.

### Breaking

- **Each overlay's layer is named after its command** — a compositor rule can now tell
  `wlr-switcher` from `wlr-chooser`, which both used `wlr-chooser`, and region
  selection, the colour picker and the loupe use `wlr-shot` or `wlr-peek` instead of
  `wlr-overlay`. Rules written for the old names stop matching; the README lists the
  namespaces.
- **`wlr-peek mirror -w` takes a window ID** — `-w` opened the chooser, which is now
  `--pick-window` as everywhere else; `-w ID` names a window in every command, and
  `wlr-peek ocr` gains it.
- `wlr-capture`: the `overlay` functions (`select_region`, `pick_point`, `magnify` and
  their `_on` forms) take the layer-shell namespace to use, and
  `VideoEncoder::resolved_backend` is gone: with `Backend::Auto`, the encoder is only
  known once the first frame opens it, and the engine logs it then. `Theme` comes from
  the new `wlr-config` crate, and its `apply` from the `theme::ApplyTheme` trait. Shipped
  as a minor version: the crate is published to let the binaries be.

## 1.10.0 — 2026-09-24

### Added

- **`wlr-overlayd`, so the overlay appears at once**
  ([#11](https://github.com/sjourdois/wlr-utils/issues/11), requested by
  [@aoterman12365](https://github.com/aoterman12365)) — a daemon that keeps the
  Wayland connection, GPU context and glyph atlas warm for `wlr-switcher` and
  `wlr-chooser`: with it running, the overlay is on screen in about ten milliseconds
  instead of around a hundred.
- **`--scratchpad` on `wlr-switcher`**
  ([#20](https://github.com/sjourdois/wlr-utils/pull/20), by
  [@bR3iN](https://github.com/bR3iN)) — on sway, `only` offers the windows kept in
  the scratchpad, `exclude` the others, and `toggle` works like `scratchpad show`:
  one key brings a window out and puts it back.
- **`--cycle-key` on `wlr-switcher`**
  ([#20](https://github.com/sjourdois/wlr-utils/pull/20), by
  [@bR3iN](https://github.com/bR3iN)) — cycles on keys other than `Tab` /
  `Shift+Tab`, such as the one the binding itself is on.
- **A lone window is switched to at once**
  ([#20](https://github.com/sjourdois/wlr-utils/pull/20), by
  [@bR3iN](https://github.com/bR3iN)) — when a filter leaves `wlr-switcher` a single
  window under hold-to-switch, it focuses it without showing the overlay.
  `--auto-select` / `--no-auto-select` force either way.
- **`Ctrl+[` cancels like `Esc`**
  ([#20](https://github.com/sjourdois/wlr-utils/pull/20), by
  [@bR3iN](https://github.com/bR3iN)) — in every overlay.

### Changed

- **`wlr-switcher` waits unless it sees a modifier held** — hold-to-switch, on by
  default for the strip, took a modifier found released as a tap already over, so a
  run from a terminal, a bar or a script switched before showing anything. It now arms
  only on a modifier it sees held; `--hold` keeps the old reading, for the quickest
  Alt-Tab.
- **A build from git names its commit** — `--version` and `doctor` report
  `1.9.0-17-g7a434c1` instead of the release number when the binary is not built
  from a release tag.

### Fixed

- **An overlay no longer comes up with no cursor**
  ([#16](https://github.com/sjourdois/wlr-utils/pull/16), by
  [@bR3iN](https://github.com/bR3iN)) — raised over a window that had hidden the
  cursor, an overlay showed none. Each overlay now sets its own.
- **`doctor` reports the running sway's version** — it read the `sway` binary on
  `PATH`, which can differ after an upgrade or in a nested session.
- **A window renamed under an open overlay shows its new title** — the tiles kept the
  title and app id the window had when the overlay opened.

## 1.9.0 — 2026-09-21

### Added

- **One key per tile, on your own keyboard layout**
  ([#13](https://github.com/sjourdois/wlr-utils/issues/13), requested by
  [@aoterman12365](https://github.com/aoterman12365)) — `--hints` on `wlr-switcher`
  and `wlr-chooser` labels every tile with the key that picks it; pressing that key
  picks it straight away. The label is a physical key, shown as the character the
  active layout prints on it: the home row reads `asdfghjkl` on QWERTY and
  `qsdfghjkl` on AZERTY, and the key under the finger is the same one. `--hints top`
  takes the row above the letters instead. Nine tiles carry a hint from the home row,
  ten from the top row; beyond that, Tab, the arrows and the mouse still reach them.
  It needs a presentation with no filter field — `strip` or `grid` — and says so on
  the card.
- **`--layout` on `wlr-chooser`** — the picker offers the three presentations
  `wlr-switcher` has: `card` (the default), `grid` and `strip`. The portal runs the
  chooser with no argument, so it still gets the card. `--grid COLSxROWS` sizes the
  card and needs it.
- **`--format` on `wlr-chooser`** — `portal` (the default) writes the
  `Window: <identifier>` / `Monitor: <name>` line `xdg-desktop-portal-wlr` reads.
  `json` writes one object on one line instead, with the window's `identifier`,
  `app-id` and `title`, its `pid` where the compositor names one, or a screen's output
  `name`. Cancelling writes nothing and exits non-zero either way.
- **The pen's dwell-to-snap is yours to tune**
  ([#15](https://github.com/sjourdois/wlr-utils/issues/15), reported by
  [@algmyr](https://github.com/algmyr)) — `wlr-draw`'s `keys.toml` gains `dwell-ms`, the
  delay before a held-still stroke snaps to a clean shape, and `dwell`, whether it snaps
  at all. `d` (or `wlr-draw snap`) turns snapping on and off while drawing, and holding
  `Alt` inverts it for the stroke in progress. The status chip says `snap off` whenever
  a pen stroke would not snap.
- **`--app-id`, `--title` and `--pid` on `wlr-switcher` and `wlr-chooser`**
  ([#12](https://github.com/sjourdois/wlr-utils/issues/12)) — offer only the windows
  that match, instead of every open one. The app id is matched exactly, the title as a
  substring, both ignoring case, like the flags of the same name on `wlr-shot`; `--pid`
  keeps every window of the process. Any flag can be repeated; given together, a window
  has to match each kind. The windows they leave out are never captured, so a narrow
  list costs less than a full one. A filter that matches no window ends the run with a
  message instead of an empty overlay. Screens are not filtered. `--pid` needs a
  compositor that names the process behind a window — Sway, Hyprland and niri do, and
  where none does the run says so and exits.
- **`--cursor` on `wlr-shot`** — `screenshot` and `record` can composite the mouse
  cursor into the capture. It stays out by default. Works on both capture
  protocols.
- **`--crf` on `wlr-shot record`** — pick a constant-quality level from 0 (lossless)
  to 51, lower being better and bigger. Each encoder gets its own equivalent: `crf`
  on libx264, `cq` on NVENC, `qp` on VAAPI. Left out, every encoder keeps the
  default it uses today. VAAPI has no lossless H.264 mode and says so rather than
  quietly encoding lossy.
- **Screen capture over `wlr-screencopy`** — the capture engine uses
  `zwlr_screencopy_manager_v1` v3 where `ext-image-copy-capture-v1` is absent, so
  screenshots, recording, the loupe/colour picker, region select and wlr-draw's
  freeze & save work on **niri** and **dwl**. Both the `wl_shm` and the dma-buf
  paths are supported. `wlr-screencopy` captures a `wl_output` and never a window,
  so `wlr-switcher`, `-w`/`--pick-window` and the per-window mirror and recorder
  stay unavailable there and say so. `doctor` reports which capture protocol is in
  use; `WLR_FORCE_SCREENCOPY=1` selects `wlr-screencopy` on a compositor that
  advertises both.
- **`wlr-switcher` focuses the picked window on cosmic-comp** — COSMIC exposes no
  `wlr-foreign-toplevel-management`, so the switcher uses `cosmic-toplevel-management`
  there and names the window by its `ext-foreign-toplevel-list-v1` identifier. Checked
  against cosmic-comp 1.8.0. A compositor with neither protocol says so and exits;
  `doctor` reports which one is in use.
- **Focus backend for cosmic-comp** — `-a` (active window) and `--current-output`
  work on **cosmic-comp**. COSMIC exposes no IPC socket, so the backend reads
  `zcosmic_toplevel_info_v1` over Wayland; `get_cosmic_toplevel` ties each COSMIC
  toplevel to its `ext-foreign-toplevel-list-v1` handle, so windows are named the same
  way the capture engine names them. Checked against cosmic-comp 1.8.0. COSMIC reports
  no focus history, so `--window-order mru` falls back to `by-name` there.
- **Most recently used window order**
  ([#14](https://github.com/sjourdois/wlr-utils/pull/14), by
  [@bR3iN](https://github.com/bR3iN)) — `--window-order mru|by-name` on
  `wlr-switcher` and `wlr-chooser`. `mru` lists the window you were just on first, so
  a quick Alt-Tab goes back to the previous one; it's the switcher's new default,
  while the chooser keeps `by-name`. Sway reports focus history per container, so
  windows on different workspaces aren't interleaved: the current workspace's windows
  come first.
- **`--window-order mru` on Hyprland and niri** — the focus history now also comes
  from `hyprctl -j clients` (`focusHistoryID`) and from `niri msg --json windows`
  (`focus_timestamp`), so a quick Alt-Tab goes back to the window you were just on
  there too. Checked against a live compositor — Hyprland 0.56.2 and niri 26.04 — by
  comparing the reported order with a known focus sequence. Compositors with no focus
  IPC backend still fall back to `by-name`.

### Changed

- **`wlr-switcher` always starts on a window other than the one you're on** — the
  highlight now opens on the first listed window that isn't the current one,
  whatever the window order and whatever the compositor; `Tab` / `Shift+Tab` are
  unchanged. The current window is read from `zwlr-foreign-toplevel-management-v1`
  (the `activated` state) rather than from a compositor IPC, so a quick Alt-Tab
  switches away from it everywhere — where it previously could open on the window
  you were already using, outside Sway or with `--window-order by-name`.
- **Font lookup now goes through fontconfig**
  ([#10](https://github.com/sjourdois/wlr-utils/pull/10), by
  [@bR3iN](https://github.com/bR3iN)) — the overlays start noticeably faster, since
  fonts are resolved from the system's fontconfig cache instead of scanning every
  installed font at launch. How the `font` and `cjk-font` theme keys are resolved
  changes a little:
  - family names are matched the way fontconfig matches them: case and spacing no
    longer matter, and aliases such as `monospace` or `sans-serif` now work;
  - a `font` family that isn't installed now falls back to your system's default
    font rather than egui's built-in one, without a warning — check the spelling if
    the overlay doesn't look the way you expect;
  - a `cjk-font` that isn't installed is still skipped in favour of auto-detection.

  `libfontconfig1` is now a runtime dependency (added to the `.deb`s). Without it,
  the overlays fall back to egui's embedded fonts, which don't cover CJK.

- **`wlr-switcher` hold-to-switch no longer falls back to a classic picker**
  ([#14](https://github.com/sjourdois/wlr-utils/pull/14), by
  [@bR3iN](https://github.com/bR3iN)) — if the launch modifier is already released
  when the overlay gets the keyboard, the switch happens straight away. A strip bound
  to a key with no modifier now needs `--no-hold`.
- `wlr-draw` runs on KDE Plasma (KWin implements `wlr-layer-shell`), minus its freeze
  and save, which need a capture protocol KWin doesn't expose. The compatibility matrix
  is corrected accordingly: neither KWin nor Mutter offers one.
- The Sway focus backend talks to sway's IPC socket directly instead of running
  `swaymsg`.
- Dependency refresh: edgefirst-egl 0.32.1, rustix 1.1.5, toml 1.1.6, clap 4.6.7.

### Fixed

- **Captures of a rotated or mirrored output come out straight** — an output
  declaring a `wl_output` transform (a screen stood on its side, a mirrored one) was
  captured in the panel's orientation, so screenshots, regions, the frozen overlay
  and recordings came out turned. Every capture path now takes the transform back
  out, on both capture protocols, and matches `grim` byte for byte.
- **XWayland windows appear in `wlr-switcher` and `wlr-chooser`** — Steam, games and
  Java applications were listed as system windows and hidden unless
  `--include-system` was given. Their application id is now reported everywhere: they
  show their icon, `wlr-shot --app-id` and `wlr-peek --app-id` select them, and the
  switcher opens on the right window when one of them has the focus.
- **Quick Alt-Tabs in `wlr-switcher` switch reliably**
  ([#14](https://github.com/sjourdois/wlr-utils/pull/14), by
  [@bR3iN](https://github.com/bR3iN)) — releasing Alt before the first frame, or
  before the overlay had the keyboard, used to leave the overlay open. The overlay no
  longer flashes on screen during a quick tap either.
- **Window tiles show the right application icon** — an application id was matched
  against any `.desktop` file name containing it, so `thunar` picked up
  `thunar-volman-settings.desktop` and a window without an id took the first icon on
  disk. Matching now follows the file name, then `StartupWMClass`; a window without
  an id gets no icon.

### Breaking

- `wlr-capture`: `activate_window` selects by `ext-foreign-toplevel-list` identifier
  instead of app id and title, `SessionId` is an opaque handle, and `Client::connect`
  also binds `wlr-foreign-toplevel-management`. Shipped as a minor version: the crate is
  published to let the binaries be.

## 1.8.0 — 2026-09-11

### Added

- **Graphics tablet (stylus) support in `wlr-draw`**
  ([#9](https://github.com/sjourdois/wlr-utils/pull/9), by
  [@jadijadi](https://github.com/jadijadi)) — the tip acts as the left mouse button,
  and the eraser end switches to the Eraser tool while it's near the tablet. `doctor`
  now reports `tablet-v2`.

### Changed

- `wlr-draw` says how to finish a text label: the status chip reads "Enter to finish"
  and the help lists Enter / Esc, whose key names are now translated.
- Dependency refresh: edgefirst-egl 0.30, egui/egui_glow 0.36.2, pipewire 0.10.1.

### Fixed

- `wlr-draw`'s help legend no longer overlaps longer key names with their descriptions.

## 1.7.0 — 2026-08-19

### Fixed

- **Build against FFmpeg 9.0** ([#8](https://github.com/sjourdois/wlr-utils/issues/8)) —
  `ffmpeg-next` 9.0 detects the system libav* at build time and adapts to it.

### Added

- Packaged in **nixpkgs** (unstable channel) — `nix-shell -p wlr-utils`.

### Changed

- Dependency refresh: egui/egui_glow 0.36, resvg 0.48, fontdb 0.24, edgefirst-egl 0.28.

## 1.6.0 — 2026-07-28

### Fixed

- **dma-buf capture on Intel and AMD** ([#6](https://github.com/sjourdois/wlr-utils/issues/6)) —
  compressed DRM modifiers carry auxiliary planes and only the first one was declared, so
  the buffer could not be imported. `wlr-shot`/`wlr-peek` failed outright and
  `wlr-chooser`/`wlr-switcher` showed empty previews.
- Modifier attributes are only posted when `EGL_EXT_image_dma_buf_import_modifiers` is
  present, instead of failing the import on drivers that lack it.

### Added

- **`--no-gpu`** (and `WLR_NO_GPU=1`) — capture through shared memory.
- **`doctor` probes the GPU path for real**, reporting the negotiated fourcc, modifier and
  plane count rather than trusting the advertised globals.
- An automatic shm fallback when a dma-buf cannot be imported.

### Changed

- Single captures (screenshots, colour picks, OCR) use shared memory; live previews keep
  the zero-copy path.
- The `gpu` feature is on by default for `wlr-shot` and `wlr-peek` — bundle builds already
  shipped it through feature unification.

### Breaking

- `wlr-capture`: `DmabufFrame` carries a `planes` vector instead of one `fd`/`stride`/
  `offset`, and `DmabufImporter::import` returning `None` means the import failed. Shipped
  as a minor version: the crate is published to let the binaries be.

## 1.5.0 — 2026-07-21

### Added

- **`--app-id` / `--title`** — target a window by application id and/or title substring
  on `wlr-shot screenshot`/`record` and `wlr-peek mirror`/`watch`/`ocr`; an ambiguous filter
  lists the candidates. `wlr-shot screenshot --list-windows` prints the identifier, app
  id and title of each window.
- **Arch Linux (AUR)** — the suite is now packaged on the AUR: `wlr-utils` (built from
  source) and `wlr-utils-bin` (prebuilt binaries).

### Fixed

- **`wlr-draw` output hotplug** — the daemon no longer quits when an output is unplugged,
  DPMS-off, or reconfigured; it keeps the other overlays and rebuilds one when the output
  returns.
- **`wlr-draw` repaint pacing** — repaints follow frame callbacks instead of blocking on
  buffer swaps, so the daemon no longer freezes on the NVIDIA EGL driver.

### Changed

- **Ubuntu 22.04** — now ships a screenshots-only `.deb` (its PipeWire / FFmpeg are too
  old to build the recorder, so `wlr-shot record` is omitted); for recording there, use
  `cargo install wlr-utils` or the prebuilt tarball.
- Dependency refresh (`cargo upgrade --incompatible`).

## 1.4.0 — 2026-07-01

### Added

- **`wlr-draw` configurable keys** — bindings now live in `~/.config/wlr-draw/keys.toml`;
  the click-through key (Caps Lock by default) and every tool key can be rebound, so
  keyboards without a Caps Lock (e.g. HHKB) are no longer stuck (#2).
- **`wlr-draw save [path]`** — save the annotated screen to a chosen path from the command
  line (#3).
- **Screen-only compositors** — tools degrade gracefully on wlroots 0.19 / Sway 1.11:
  screen capture (screenshots, recording, loupe, `wlr-draw` freeze/save) works; window-only
  features stay off with a clear notice, and `wlr-switcher` reports the missing window
  capture and exits instead of showing an empty overlay (#1).
- **`doctor` on every tool** — the compositor report is now a `doctor` subcommand on
  `wlr-peek`/`wlr-shot`/`wlr-draw` and a `--doctor` flag on `wlr-chooser`/`wlr-switcher`
  (previously only `wlr-peek doctor`), so any single-tool install can produce it. It also
  reports the run environment — tool version, OS, compositor + version, install hint — so
  it doubles as the environment block a bug report needs.

### Fixed

- **`wlr-draw save`** — paths containing spaces are no longer truncated (#3).
- **`wlr-draw`** — a very short arrow no longer panics while sizing its head.
- **Debian / Ubuntu `.deb`** — now built per distro (Debian 12–sid, Ubuntu 24.04–26.04) so
  it links against that distro's FFmpeg / Leptonica; fixes `liblept.so.5` / `libavutil.so.58`
  load failures on Debian 13 (#1).
- **`wlr-peek doctor`** — verdicts reflect the two capture floors (screen 0.19/1.11,
  window 0.20/1.12).

### Security

- Control socket and single-instance locks stay in a private directory even when
  `$XDG_RUNTIME_DIR` is unset, instead of a predictable name in world-readable `/tmp`.

### Changed

- **`wlr-capture`** — the engine returns a typed `CaptureError` instead of `anyhow`
  (breaking for library users); each tool now owns its own translation catalog.
- **Docs** — added a compositor compatibility matrix and per-distro `.deb` install notes.
- Dependency refresh (egui 0.35, xkbcommon 0.9).

## 1.3.2 — 2026-06-26

### Added

- **`wlr-utils`** — a new bundle crate that installs the whole suite at once:
  `cargo install wlr-utils` provides `wlr-chooser`, `wlr-switcher`, `wlr-peek`, `wlr-shot`
  and `wlr-draw`. The prebuilt release now ships a **single archive** with every binary
  plus a one-line `wlr-utils-installer.sh`, and a **single `wlr-utils` `.deb`** (it
  `Replaces`/`Conflicts` the old per-tool packages), instead of one download per tool. Each
  tool stays its own crate for lighter, à-la-carte installs.

### Removed

- **`wlr-pip`** — the deprecated stub (superseded by `wlr-peek mirror`) is no longer built
  or published.

### Fixed

- **`wlr-peek doctor`** — on a screen-capture-only compositor (Sway 1.11 / wlroots 0.19)
  the report no longer claims a bare "screen capture: supported"; it now notes the tools
  also open the window source at start-up, so the effective floor stays Sway 1.12 /
  wlroots 0.20.
- **Docs** — aligned the **Install / Uninstall / Requirements** sections across every
  README (per-tool system dependencies, the unified bundle, and removing `wlr-draw`'s
  autostart entry).

## 1.3.1 — 2026-06-26

### Added

- **`wlr-draw`** — the tray menu gained a **Start on login** toggle that writes / removes
  an XDG autostart entry (`~/.config/autostart/wlr-draw.desktop`). The daemon also
  auto-registers itself on its first ever run (tracked by a sentinel under
  `$XDG_STATE_HOME`), so it starts with the session out of the box; unchecking it from the
  tray is then permanent — a later manual launch won't bring it back.

### Fixed

- **Compositor requirements** were documented wrong (`wlroots 0.18` / `Sway 1.10`). The
  base protocol plus output capture landed in **wlroots 0.19 / Sway 1.11**, and the
  foreign-toplevel source that *window* capture needs only in **wlroots 0.20 / Sway 1.12**
  — the suite's real floor. `COMPATIBILITY.md` is corrected, the
  `ext_foreign_toplevel_image_capture_source_manager_v1 missing` error now names the
  required version and points at `wlr-peek doctor`, and `doctor` reports **screen** and
  **window** capture as separate verdicts.
- **`wlr-draw`** — the daemon never called `i18n::init()` (unlike every other binary), so
  the tray menu and on-screen hints stayed English regardless of `$LANG`/`$LANGUAGE`. It
  now negotiates the desktop locale at startup. The autostart entry also carries its state
  as a `☑` / `☐` glyph in the label rather than a dbusmenu `toggle-type=checkmark`, which
  several SNI hosts (e.g. waybar) don't render reliably.

### Changed

- Replaced the unmaintained `khronos-egl` (last released ~5 years ago) with the
  API-compatible, actively maintained `edgefirst-egl` fork, which lets the EGL bindings
  track **`libloading` 0.9**. Refreshed the rest of the dependency tree (`image` 0.25.10,
  `resvg` 0.47, …) and dropped the pinned `rust-version` from the workspace manifest.
- **`wlr-capture`** — docs.rs now documents the full public API. It built with default
  features only, so the feature-gated modules (`capture`/`focus`/`overlay`/`mirror`/
  `video`) were missing; a `[package.metadata.docs.rs]` block enables them (with
  `doc(cfg)` feature badges). Documented every remaining public item (100 % coverage,
  enforced by `#![warn(missing_docs)]`). `audio` stays off on docs.rs — its `pipewire`
  dep fails to build there.

## 1.3.0 — 2026-06-25

### Added

- **`wlr-shot`** — a new screen-capture tool. `screenshot` captures an output (`-o`), a
  slurp-style logical region (`-g`, stitched across outputs), a window (`-w` /
  `--pick-window`), the whole layout (`--all`), the active window (`-a`) or focused
  output (`--current-output`) to PNG/JPEG/PPM (file, stdout, or `-c` clipboard), with an
  interactive **frozen region selector** (`-s`).
- **`wlr-shot record`** — record to **H.264** (`.mp4`/`.mkv`), animated **GIF/WebP**
  (downscaled; best on a region), with **system audio** as an AAC track (native
  PipeWire; `--no-audio`, `--audio-source`; an optional Pulse/ALSA fallback lives behind
  the off-by-default `audio-fallback` feature). Pluggable encoder
  (`--encoder auto|nvenc|vaapi|software`), constant-`--fps` capture, `--timelapse`,
  `--duration`/Ctrl-C.
- **`wlr-switcher`** — a live Alt-Tab / exposé that **focuses** the picked window; held
  modifier to switch, `--layout strip|grid|card`, live previews (`--live`).
- **`wlr-draw`** — a transparent on-screen annotation overlay (gromit-mpx-style): pen,
  rectangle, arrow, text, mask, eraser, **move** (right-drag or the move tool + arrow
  nudge), and dwell-to-snap circles/lines. Runs as a daemon driven by a key-bound
  control socket, with a colour palette, tray icon and systemd unit. Plus presenter
  tools: **spotlight** (hold Shift to dim the screen around a shape or the cursor),
  **freeze-frame** (`Space`), and **save** the annotated screen to PNG (`w`).
- **`wlr-peek`** — `watch` (change/idle monitor), `grep` (OCR then locate text),
  `region` (a native slurp replacement); and `mirror` now takes the suite's common
  source flags plus `--follow window`.
- **Compositor compatibility** — `wlr-peek doctor` reports the protocols the running
  compositor advertises; focus backends for Hyprland and niri join Sway; see
  [`COMPATIBILITY.md`](COMPATIBILITY.md).
- **`wlr-capture`** engine — a shared capture-session driver (`stream`), a frame-diff
  metric (`diff`), one-shot capture, a `Region` type with cropping/multi-output
  compositing, GPU dma-buf readback (`gl`), the `FrameSink` output seam, a wlroots
  clipboard (`data-control`), and a native-PipeWire `audio` module. Split into a lean
  always-on core plus optional features (`toolkit`, `video`, `audio`, `overlay`, …).
- **i18n** — the shared Fluent catalog is now complete in 13 languages (de, en, es, fr,
  it, ja, ko, nl, pl, pt-BR, ru, uk, zh-CN). The command line stays English.

### Changed

- **`wlr-chooser`** is now strictly the xdg-desktop-portal picker (prints the chosen
  source); the switcher modes moved to `wlr-switcher`. Both ship from one package and
  share the engine.
- The chooser/switcher overlay starts capturing before building itself, so thumbnails
  stream in sooner (`WLR_CHOOSER_TIMING=1` prints cold-start timing).
- Upgraded to the latest major dependencies (egui 0.34, glow 0.17, pipewire 0.10,
  ksni 0.3); the **minimum supported Rust version is now 1.92**.

## 1.2.0

### Added

- **`wlr-pip`**: a new companion binary — a floating, always-on-top live mirror
  (picture-in-picture) of a single window, sharing the same zero-copy GPU capture
  engine. Pick a window via `wlr-chooser` (run `wlr-pip` with no argument) or pass
  its identifier (`wlr-pip <id>`). It is an `xdg-toplevel` (pair with Sway
  `floating enable, sticky enable` for always-on-top across workspaces): drag to
  move, corner grip to resize (source aspect ratio kept), hover for collapse/close,
  `Esc` to quit. Collapsed to an icon badge, it pops back open when its window
  changes. One mirror per window (single-instance lock per identifier). Keyboard
  shortcuts: `Space` freeze/unfreeze, `c` collapse, `+`/`-` or wheel for opacity,
  `r` re-pick another window, `q`/`Esc` close.

### Changed

- The project is now a Cargo **workspace**: a shared `wlr-capture` library (the
  wlroots capture engine + the egui/EGL rendering & dma-buf-import toolkit, both
  extracted from the previous single crate) plus the `wlr-chooser` and `wlr-pip`
  binaries. No behaviour change for `wlr-chooser`.

## 1.1.0

### Added

- **Live thumbnails**: previews now refresh continuously (damage-driven), so the
  grid shows windows updating in real time, including on other workspaces.
- **GPU zero-copy capture** behind the `gpu` Cargo feature (on by default):
  dma-bufs are allocated via gbm and imported as GL textures (EGLImage), with no
  CPU read-back. Falls back to the CPU shm path automatically when unavailable.
  Build without it (no gbm/`libgbm` dependency) via `--no-default-features`.
- **`--switch`** window switcher: a live alt-tab / exposé that **focuses** the
  picked window (via `zwlr-foreign-toplevel-management-v1`) instead of printing.
  Two presentations via `--layout`: `full` (full-screen mission-control grid that
  dims the desktop, with an intro animation — default) or `compact` (the centred
  card). Identical windows are disambiguated by creation order so the right one
  is focused. Only one switcher opens at a time (re-pressing the keybind is a
  no-op, via a single-instance lock).

## 1.0.0

Initial release.
