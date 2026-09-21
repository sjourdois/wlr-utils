# wlr-chooser

[![CI](https://github.com/sjourdois/wlr-utils/actions/workflows/ci.yml/badge.svg)](https://github.com/sjourdois/wlr-utils/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/wlr-chooser.svg)](https://crates.io/crates/wlr-chooser)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](#license)

Part of the [wlr-utils](https://github.com/sjourdois/wlr-utils) workspace.

A graphical window & screen picker for **wlroots** screencast portals
(`xdg-desktop-portal-wlr`) — a rofi-like overlay with **live thumbnails**.

<p align="center">
  <img src="https://raw.githubusercontent.com/sjourdois/wlr-utils/main/docs/assets/wlr-chooser/picker.png"
       alt="wlr-chooser: a grid of live window and screen thumbnails to pick a screen-share source" width="820">
</p>

The same crate ships **`wlr-switcher`**, a live Alt-Tab / exposé window switcher.
The strip cycles with Tab; the exposé **reveals windows from other workspaces**:

<p align="center">
  <img src="https://raw.githubusercontent.com/sjourdois/wlr-utils/main/docs/assets/wlr-switcher/altab.gif"
       alt="wlr-switcher: a macOS-style strip of live window previews, cycling with Tab" width="410">
  <img src="https://raw.githubusercontent.com/sjourdois/wlr-utils/main/docs/assets/wlr-switcher/expose.gif"
       alt="wlr-switcher: the full-screen exposé grid revealing windows from every workspace" width="410">
</p>

When an application requests screen sharing (e.g. Firefox `getDisplayMedia`, a
video call), the wlroots portal asks an external *chooser* which source to share.
`wlr-chooser` replaces the text-only chooser with a grid of live previews — pick a
window or a monitor with a click.

It also ships **`wlr-overlayd`**, an optional daemon that holds the GPU context both
tools would otherwise build from scratch, so the overlay appears in milliseconds
instead of ninety — see [Instant overlays](#instant-overlays--wlr-overlayd).

<p align="center"><sub>📖 See every tool in action on the <a href="https://sjourdois.github.io/wlr-utils/">showcase</a>.</sub></p>

## Why

- **Real overlay**, like rofi: a `wlr-layer-shell` surface that grabs the keyboard,
  dims the desktop behind a centred card, and cancels on click-outside or Escape.
- **Captures any window** — including ones on other workspaces/outputs — via the
  compositor's native toplevel capture (`ext-image-copy-capture-v1`), not
  screen-region grabs. Off-screen windows are real previews, not icons.
- **Live thumbnails that actually move**: previews refresh in real time, and on
  the GPU path (default) the dma-buf is imported straight as a texture — no
  read-back, near-zero CPU. Falls back to CPU shm where the GPU path isn't usable.
- **Doubles as a window switcher**: the `wlr-switcher` binary picks a window to focus it.
- **Native Wayland** (no XWayland), built in Rust with [egui]; opens near-instantly.
- **Themeable** (8 ready palettes incl. Catppuccin), **localised** (13 languages,
  with CJK font fallback), and a configurable thumbnail grid.

## Requirements

- A compositor speaking the wlroots protocols, with `wlr-layer-shell` and a capture protocol.
  Screen sources need `ext-image-copy-capture-v1` with the **output** source
  (**Sway ≥ 1.11 / wlroots ≥ 0.19**), or `wlr-screencopy`; live **window** thumbnails
  and **`wlr-switcher`** need the **foreign-toplevel** source +
  `ext-foreign-toplevel-list-v1` (**Sway ≥ 1.12 / wlroots ≥ 0.20**), which
  `wlr-screencopy` does not stand in for. Without it, `wlr-switcher` reports the missing
  window capture and exits. Run `wlr-chooser --doctor` (or `wlr-switcher --doctor`) to
  check your own; see [COMPATIBILITY.md](../../COMPATIBILITY.md).
- `xdg-desktop-portal-wlr` ≥ 0.8 (for the screencast chooser use).
- For the **GPU path** (default): a working EGL/GLES driver and `libgbm`
  (ships with Mesa). It falls back to CPU automatically if unavailable.
- For **`wlr-switcher`**: `zwlr-foreign-toplevel-management-v1`, or COSMIC's
  `cosmic-toplevel-management` where that is missing.

## Install

> **Want the whole suite?** Install the bundle instead — `cargo install wlr-utils` gets
> every tool (`wlr-chooser`, `wlr-switcher`, `wlr-peek`, `wlr-shot`, `wlr-draw`) in one
> go. The single-tool install below is the lighter, à-la-carte option.

```sh
cargo install wlr-chooser        # installs both wlr-chooser and wlr-switcher
```

Or build just these binaries from the [wlr-utils](../../README.md) workspace:

```sh
cargo build --release -p wlr-chooser
```

The `gpu` feature (on by default) enables zero-copy dma-buf capture and needs `libgbm-dev`
at build time (`libgbm` at runtime, from Mesa). `--no-default-features` builds a pure-CPU
binary with no gbm dependency; `--no-gpu` (or `WLR_NO_GPU=1`) switches the same build to
shared memory at runtime, and previews fall back to it on their own if an import fails. The whole suite also ships as a single `wlr-utils` `.deb`
on every [release](https://github.com/sjourdois/wlr-utils/releases/latest).

## Uninstall

The crate ships two binaries — `wlr-chooser` and `wlr-switcher`. Remove both the way you
installed them:

```sh
cargo uninstall wlr-chooser                       # crates.io install (~/.cargo/bin)
rm -f ~/.local/bin/wlr-chooser ~/.local/bin/wlr-switcher   # manual `install` from source
sudo apt remove wlr-chooser                        # the .deb package
```

## Set up the portal

Point the screencast chooser at the binary:

```ini
# ~/.config/xdg-desktop-portal-wlr/config
[screencast]
chooser_type=simple
chooser_cmd=wlr-chooser
```

Then restart the portal: `systemctl --user restart xdg-desktop-portal-wlr`.

Now any screen-share prompt opens `wlr-chooser` as a dimmed modal overlay on the
focused output. You can pass options in `chooser_cmd`, e.g.
`chooser_cmd=wlr-chooser --windows --grid 4x3`.

## Options

```
-w, --windows          Show only windows
-o, --outputs          Show only screens          (alias: --screens)
    --both             Show both (default)
    --include-system   Include windows with no app-id (system surfaces)
    --app-id APP_ID    Show only windows with that app-id (repeatable)
    --title TEXT       Show only windows whose title contains TEXT (repeatable)
    --pid PID          Show only windows of that process (repeatable)
    --layout card|grid|strip
                       Presentation: centred card (default), full-screen exposé
                       or macOS-style row
    --hints [home|top] Label each tile with the key that picks it
    --format portal|json
                       How the pick is written on stdout (default: portal)
    --grid COLSxROWS   Fixed grid of that many thumbnails (e.g. 4x3)
    --window-order by-name|mru
                       Order windows by name (default) or most recently focused
                       first, if supported by the compositor
    --no-gpu           Capture through shared memory instead of dma-buf
    --doctor           Report the compositor's capture protocols, then exit
-h, --help             Print help
-V, --version          Print version
```

In the overlay: type to filter, arrows to move, Enter/click to pick, Escape or
click-outside to cancel, and the tab bar switches All / Windows / Screens.

The three presentations of `--layout` are the ones `wlr-switcher` offers, with the
card as the default here: the portal runs the chooser with no argument, so that path
is unchanged. `--grid COLSxROWS` sizes the card and needs it; the exposé and the strip
lay themselves out.

`--app-id`, `--title` and `--pid` pick which windows are offered at all. The
app-id is matched exactly, the title as a substring, both ignoring case — the same
comparison `wlr-shot --app-id` / `--title` make. `--pid` keeps every window of the
process. Any flag can be repeated to widen its set; give several kinds and a
window has to match each. Windows left out are never captured, so a narrow list
costs less than a full one. Screens are not filtered, so only `--windows` can end
up with nothing to show: the picker then says so and exits.

`--pid` needs a compositor that names the process behind a window, which Wayland
itself does not: Sway, Hyprland and niri do, through their IPC. Elsewhere `--pid`
says so and exits rather than offering every window.

### Pick a tile with one key — `--hints`

`--hints` labels every tile with the key that picks it. Pressing that key picks the
tile straight away, like a click on it.

The label is a **physical key**, shown as the character your active keyboard layout
prints on it. The home row reads `asdfghjkl` on QWERTY and `qsdfghjkl` on AZERTY; the
key under the finger is the same one. `--hints top` takes the row above the letters
instead — `1234567890` on QWERTY, `&é"'(-è_çà` on AZERTY. The home row is the default:
it prints a letter on almost every layout, where the row above often prints
punctuation.

Nine tiles carry a hint from the home row, ten from the top row. Beyond that the tiles
have none, and are picked with Tab, the arrows or the mouse as before. A key the
layout prints nothing on is passed over.

Hints need a presentation with no filter field, where a letter is a shortcut and not
text: `--layout grid` or `--layout strip`. Asked for on the card, the run says so and
exits.

```sh
wlr-chooser -w --app-id foot --layout grid --hints --format json
```

That is an exposé of your terminals alone, one keystroke each, and the answer on
stdout for a script to act on.

### Output — `--format`

`portal` (the default) writes the line `xdg-desktop-portal-wlr` reads:

```text
Window: <foreign-toplevel-identifier>
Monitor: <output-name>
```

`json` writes one object on one line instead, naming what a script can act on — that
identifier means nothing to any other tool:

```json
{"app-id":"foot","identifier":"7e2cbb…","pid":1312558,"title":"vim","type":"window"}
{"name":"DP-4","type":"screen"}
```

`type` says which fields are present. A window carries its `identifier`, `app-id` and
`title` — the terms `wlr-shot --app-id` / `--title` take — plus its `pid` where the
compositor names one (Sway, Hyprland, niri); elsewhere the key is absent. A screen
carries the output `name` that addresses it everywhere else. The field names are a
contract: they may gain company, never change meaning.

Either way, cancelling writes nothing and exits non-zero, so a script tells a pick
from a cancel by the exit code.

> **Looking for an Alt-Tab / window switcher?** That is a separate binary,
> **`wlr-switcher`** (shipped alongside this one) — see [its section](#window-switcher--wlr-switcher) below.

## Window switcher — `wlr-switcher`

The same crate ships a second binary, **`wlr-switcher`**: a live Alt-Tab / exposé
that **focuses** the picked window (via `zwlr-foreign-toplevel-management-v1`, or
`cosmic-toplevel-management` on COSMIC) instead of printing to stdout. It reuses
this engine, so previews are **live** — even for windows on other workspaces —
which is what sets it apart from a plain Cmd-Tab.

The two split the work cleanly: **the chooser answers, the switcher acts.** Both offer
the same presentations, the same window filters and the same `--hints`; pick the
chooser when a script decides what happens next, the switcher when the answer is
"focus it".

Three presentations via `--layout`:

- `strip` (default) — a macOS-style single row of tiles, the highlighted window's
  name above the row;
- `grid` — a full-screen, mission-control exposé;
- `card` — the centred rofi-like card.

Each tile shows a live preview with the app icon as a badge; tune it with
`--live none|current|all` (default `all`): `current` previews only the highlighted
window, `none` shows app icons only.

`--hints [home|top]` labels the tiles with the key that picks them, as it does in the
chooser (see *Pick a tile with one key* above) — one keystroke to switch instead of a
run of Tabs. It needs `strip` or `grid`, which have no filter field.

```
bindsym Mod1+Tab exec wlr-switcher --hints
```

`--app-id`, `--title` and `--pid` restrict the switcher to a subset of the open
windows, with the same meaning as in `wlr-chooser` above:

```
bindsym Mod1+grave exec wlr-switcher --app-id foot --app-id firefox
bindsym $mod+n exec wlr-switcher --title notes
bindsym $mod+e exec wlr-switcher --pid $(pidof -s emacs)
```

The windows they leave out are never captured. If no open window matches,
`wlr-switcher` says so and exits rather than opening an empty overlay.

Windows are listed most recently focused first where the compositor reports it
(Sway, Hyprland, niri), by name otherwise; `--window-order by-name` always orders
them by name.

Whatever the order, the overlay opens with the **first window that is not the one
you are on** highlighted, so releasing the modifier straight away always switches
somewhere. Which window that is comes from `zwlr-foreign-toplevel-management-v1`
(the `activated` state) rather than from a compositor IPC. cosmic-comp does not
expose that protocol, so the overlay opens on the first tile there.

### True Alt-Tab (hold-to-switch)

Bind `wlr-switcher` to a **held** modifier and it behaves like a classic Alt-Tab:

```
bindsym Mod1+Tab exec wlr-switcher                 # hold Alt, Tab cycles, release switches
bindsym $mod+Tab exec wlr-switcher --layout grid   # full-screen exposé
```

- The overlay appears while the modifier (Alt **or** Super) is held.
- **`Tab`** moves to the next window, **`Shift+Tab`** to the previous one.
- **Releasing the modifier** confirms the highlighted window and switches to it.
- Mouse click and `Esc` (cancel) still work.

Hold-to-switch is **on by default for `strip`** and **off for `grid`/`card`**;
force it either way with `--hold` / `--no-hold`. With it off, the overlay stays
open after release — confirm with Enter or a click. Only one switcher opens at a
time (re-pressing the keybind is a no-op).

With hold-to-switch on, a modifier that is no longer held when the overlay gets the
keyboard counts as released: a quick tap switches straight away, without showing
the overlay. Bind a strip to a key with no modifier (or run it from a terminal) with
`--no-hold`, or it switches as soon as it opens.

## Instant overlays — `wlr-overlayd`

Most of the delay before an overlay appears is initialisation the run then throws away:
the EGL context and its compiled shaders, the font set, the Wayland connection. On an
NVIDIA driver at 2560×1440 that is 93 to 109 ms, of which the EGL setup alone is about 70.

**`wlr-overlayd`** pays it once, at login, and keeps it. With one running, `wlr-switcher`
and `wlr-chooser` hand it the run and the overlay is up in roughly ten milliseconds.

### What you have to do

Start it with your session. That is the whole setup: **your keybindings do not change**,
and neither does the portal. Both tools find the daemon on their own, and behave exactly
as before when there is none.

One line in your compositor's config:

```
exec wlr-overlayd                 # sway
exec-once = wlr-overlayd          # Hyprland
spawn-at-startup "wlr-overlayd"   # niri
```

Or, if you would rather have it in the journal and restarted with the session, the
provided systemd `--user` unit ([`contrib/wlr-overlayd.service`](contrib/wlr-overlayd.service)):

```sh
install -Dm644 contrib/wlr-overlayd.service ~/.config/systemd/user/wlr-overlayd.service
systemctl --user enable --now wlr-overlayd.service
```

It is bound to `graphical-session.target`, so it comes up with the Wayland session and
goes down with it — this needs a session that populates that target, which uwsm does. The
unit calls `wlr-overlayd` by name; if yours lives somewhere the user manager's `PATH` does
not cover, write the full path in `ExecStart`. **Use one mechanism, not both.**

To check it took, run `wlr-switcher` from a terminal: silence means the daemon served it,
and a line on stderr says why it did not.

### Stopping, restarting, logs

```sh
wlr-overlayd --quit                          # stop the running daemon
systemctl --user restart wlr-overlayd        # after installing a new build
journalctl --user -t wlr-overlayd -f         # its output, however it was started
```

Filtering the journal by the **binary name** rather than the unit works whichever way you
started it. Set `WLR_CHOOSER_TIMING=1` in its environment and it prints, for every overlay
it shows, where the milliseconds went.

### What it does and does not hold

One daemon serves both front-ends because they *are* one overlay: the same egui app on the
same engine, differing only in what they do with the pick. Two would warm two GPU contexts
for it.

**It captures nothing while it idles.** The capture thread is spawned for each overlay and
dies with it — between two, the daemon holds a Wayland connection and a GPU context, and
reads no window contents. What an overlay put on the GPU is freed when it closes, the
imported window buffers included, so the daemon holds on to no window it is no longer
showing. Idle it sits on a `poll()` and costs no CPU; of its resident memory, most is
library pages shared with everything else drawing on screen, and about 40 MB is the warm
GPU context itself — which is the whole point of it.

Each overlay still builds its own layer surface, so it opens on the screen you are working
on, and screens can be plugged or unplugged under an idle daemon.

It shows **one overlay at a time**, and tells a second caller so at once. For
`wlr-switcher` that is the no-op pressing the keybinding twice has always been;
`wlr-chooser` shows its own overlay instead, so a portal waiting for a screen-share picker
is never left with no answer.

### Running without it

Nothing starts a daemon for you, and nothing depends on one: with none listening, both
tools show the overlay themselves exactly as they always have, and say so on stderr — you
would otherwise have no way of knowing you were paying the full startup. If that is a
deliberate choice, `--no-daemon` makes it explicit and silences the notice:

```
bindsym Mod1+Tab exec wlr-switcher --no-daemon
```

| | |
|---|---|
| `wlr-overlayd` | run the daemon in the foreground (what your autostart runs) |
| `wlr-overlayd --quit` | stop the running daemon |
| `wlr-overlayd --no-gpu` | serve every overlay through shared memory instead of dma-buf |
| `--no-daemon` | on either tool: show the overlay in this process, daemon or not |

A run that asks for `--no-gpu`, or that has `WLR_NO_GPU` set, never goes through the
daemon: it changes what the whole process does, and a daemon started without it cannot
honour it. Such a run shows its own overlay, at the usual cold-start cost — start the
daemon itself with `--no-gpu` if that is what your driver needs.

### The protocol

The daemon listens on `$XDG_RUNTIME_DIR/wlr-overlayd.sock`, one line of text per request,
like `wlr-draw`'s control socket — `switch` or `choose` (with the invocation's arguments
after it, separated by `\x1f`), `ping` or `quit`. It answers `ok`, `ok <stdout line>`,
`cancel`, `busy` or `err <reason>`, which the client turns back into its own output and
exit status. Exit statuses are unchanged: `0` for a run that did what it was asked, `1`
for a cancel, `2` for a failure — and `wlr-chooser` writes the same stdout line wherever
the overlay was shown.

Messages an overlay writes to stderr — a compositor that cannot focus a window, a `--pid`
filter it cannot apply — reach the client that asked for it. Anything the capture thread
has to say goes to the daemon's own output.

## Output contract

`wlr-chooser` writes the selected source to stdout and exits `0`:

```text
Window: <foreign-toplevel-identifier>
Monitor: <output-name>
```

On cancel it writes nothing and exits non-zero.

## Theming

Colours and fonts come from `~/.config/wlr-chooser/theme.toml`
(`$XDG_CONFIG_HOME` is honoured) with sensible dark defaults. Colour keys are
`#rrggbb` / `#rrggbbaa`:

```toml
accent        = "#89b4fa"
screen-accent = "#74c7ec"   # outline for screens
window-accent = "#cba6f7"   # outline for windows
backdrop      = "#11111baa" # dimmed overlay

font      = "JetBrains Mono" # UI font family (via fontconfig)
# font-path = "/path/to/Font.ttf"
# cjk-font = "Noto Sans CJK JP"
font-size = 15.0
```

Screens are outlined in `screen-accent`, windows in `window-accent`, so the two
can't be confused. Ready-made themes live in [`docs/themes/`](../../docs/themes/):
Catppuccin (Mocha, Macchiato, Frappé, Latte), Nord, Gruvbox, Dracula, Tokyo Night.
Symlink one so it tracks updates:

```sh
mkdir -p ~/.config/wlr-chooser
ln -sf "$PWD/docs/themes/catppuccin-mocha.toml" ~/.config/wlr-chooser/theme.toml
```

## Localisation

The UI ships in 13 languages (English, French, German, Spanish, Italian,
Brazilian Portuguese, Dutch, Polish, Russian, Ukrainian, Japanese, Korean,
Simplified Chinese), translated with [Fluent](https://projectfluent.org/). It
**follows your desktop locale** (`LANG` / `LC_*`) and falls back to English when no
catalog matches. Override it any time with `LANGUAGE`:

```sh
LANGUAGE=ja wlr-chooser
```

Rendering CJK text needs a CJK font installed (e.g. Noto Sans CJK); one is
auto-detected. New locales are welcome — copy
`crates/wlr-chooser/i18n/en/wlr_chooser.ftl`.

## Contributing

Bug reports, translations and patches welcome — see
[CONTRIBUTING.md](../../CONTRIBUTING.md). Please keep `cargo fmt`, `cargo clippy` and
`cargo test` clean.

## License

Licensed under either of [Apache-2.0](../../LICENSE-APACHE) or
[MIT](../../LICENSE-MIT) at your option.

[egui]: https://github.com/emilk/egui
