# Contributing to wlr-utils

Thanks for your interest! Bug reports, translations, themes and patches are all
welcome.

## The workspace

`wlr-utils` is a Cargo workspace. One library powers a handful of binaries:

| Crate | Binaries | What it is |
|-------|----------|------------|
| `wlr-capture` | — | the shared engine: wlroots capture (`ext-image-copy-capture-v1`, or `wlr-screencopy` for screens, dma-buf zero-copy + shm fallback) and the egui/EGL overlay toolkit |
| `wlr-config` | — | `config.toml`: finding and reading it, the themes, the migration from the old files |
| `wlr-i18n` | — | shared Fluent localisation plumbing; each tool builds its own catalog on it |
| `wlr-chooser` | `wlr-chooser`, `wlr-switcher`, `wlr-overlayd` | screen-share picker + Alt-Tab/exposé switcher, and the daemon that keeps their overlay warm |
| `wlr-shot` | `wlr-shot` | screenshots & recording |
| `wlr-peek` | `wlr-peek` | colour picker, loupe, mirror, OCR, grep, watch |
| `wlr-draw` | `wlr-draw` | on-screen annotation overlay |
| `wlr-utils` | `wlr-chooser`, `wlr-switcher`, `wlr-overlayd`, `wlr-peek`, `wlr-shot`, `wlr-draw` | bundle crate re-exporting every tool's binary (`cargo install wlr-utils`); kept out of `default-members` so a plain build doesn't clash on duplicate binary names |

## Building & checks

Build the default set (every tool, **not** the bundle), or a single tool with `-p`:

```sh
cargo build                       # all tools (default-members)
cargo build --release -p wlr-shot # just one tool
cargo build -p wlr-utils          # the bundle (re-exports the same binaries)
```

> Avoid `--workspace` for builds: it pulls in the `wlr-utils` bundle alongside the
> individual crates, and Cargo warns about the duplicate binary names. The default set
> excludes the bundle, so plain `cargo build` / `cargo test` stay clean; check the bundle
> on its own with `-p wlr-utils`.

Before opening a pull request, make these clean (CI runs the same):

```sh
cargo check --locked              # Cargo.lock is up to date
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo build -p wlr-utils          # the bundle isn't in the default set
```

The engine has feature combinations worth checking when you touch it, e.g.
`cargo clippy -p wlr-capture --no-default-features --features overlay` (and
`mirror`, `compose`, `focus`, `pointer`, `keys`, `video`, `audio`, `audio-fallback`,
`gpu`).

## Testing the overlays without disturbing your screen

The interactive tools are layer-shell overlays, so they cover your screen. To
iterate (and to regenerate the screenshots/videos), use the generator in
[`tools/screenshots`](tools/screenshots): it spins up an **isolated, headless
nested sway**, drives the tool with a synthetic pointer + keyboard and captures
the result — all without touching your real session.

```sh
cd tools/screenshots
./capture.sh draw          # build + run a single scene
./capture.sh               # regenerate every asset
```

See `tools/screenshots/README.md` for how it works.

## The overlay daemon

How `wlr-overlayd` serves `wlr-switcher` and `wlr-chooser`.

### What it holds

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

## Translations

Each tool crate owns **its own** Fluent catalog under
`crates/<crate>/i18n/<lang>/<domain>.ftl` (domains `wlr_chooser`, `wlr_peek`,
`wlr_shot`, `wlr_draw`); the shared loader plumbing lives in the `wlr-i18n` crate,
and `wlr-capture` (the engine) carries no UI strings. To add a language to a tool,
copy its `en` catalog (e.g. `crates/wlr-draw/i18n/en/wlr_draw.ftl`), translate the
values — keep the `{ $name }` placeables and the message keys — and add the file.
The English catalog is the source of truth and the per-message fallback; CJK
renders via an auto-detected CJK font. CLI `--help` text stays English by design.

## Themes

A theme is a file of colours (and optional fonts) shared by the overlays: the keys of
the `[theme]` section, without its header. Add new palettes to `docs/themes/`; the keys
are listed in [`docs/config.toml`](docs/config.toml).

## Commit messages & license

Conventional-commit style (`feat:`, `fix:`, `docs:` …) is appreciated. By
contributing, you agree that your contributions are dual-licensed under
Apache-2.0 and MIT, the same terms as the project.

## Releasing

Cutting a release (the whole workspace versions as one block) follows a checklist
that catches the docs and CI files which drift after a structural change — see
[`RELEASING.md`](RELEASING.md).
