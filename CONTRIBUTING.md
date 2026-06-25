# Contributing to wlr-utils

Thanks for your interest! Bug reports, translations, themes and patches are all
welcome.

## The workspace

`wlr-utils` is a Cargo workspace. One library powers a handful of binaries:

| Crate | Binaries | What it is |
|-------|----------|------------|
| `wlr-capture` | — | the shared engine: wlroots capture (`ext-image-copy-capture-v1`, dma-buf zero-copy + shm fallback) and the egui/EGL overlay toolkit |
| `wlr-chooser` | `wlr-chooser`, `wlr-switcher` | screen-share picker + Alt-Tab/exposé switcher |
| `wlr-shot` | `wlr-shot` | screenshots & recording |
| `wlr-peek` | `wlr-peek` | colour picker, loupe, mirror, OCR, grep, watch |
| `wlr-draw` | `wlr-draw` | on-screen annotation overlay |
| `wlr-pip` | `wlr-pip` | deprecated stub (use `wlr-peek mirror`) |

## Building & checks

Build the whole workspace, or a single tool with `-p`:

```sh
cargo build                       # everything
cargo build --release -p wlr-shot # just one tool
```

Before opening a pull request, make these clean (CI runs the same):

```sh
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test --workspace
```

The engine has feature combinations worth checking when you touch it, e.g.
`cargo clippy -p wlr-capture --no-default-features --features overlay` (and
`mirror`, `compose`, `focus`, `video`, `gpu`, `i18n`).

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

## Translations

The tools share **one** Fluent catalog (the `wlr_capture` domain), under
`crates/wlr-capture/i18n/<lang>/wlr_capture.ftl`. To add a language, copy
`crates/wlr-capture/i18n/en/wlr_capture.ftl`, translate the values — keep the
`{ $name }` placeables and the message keys — and add the file. The English
catalog is the source of truth and the per-message fallback; CJK renders via an
auto-detected CJK font. CLI `--help` text stays English by design.

## Themes

A theme is a `theme.toml` of colours (and optional fonts), shared by the
overlays. Add new palettes to `docs/themes/`; the keys are documented in
`crates/wlr-capture/src/theme.rs`.

## Commit messages & license

Conventional-commit style (`feat:`, `fix:`, `docs:` …) is appreciated. By
contributing, you agree that your contributions are dual-licensed under
Apache-2.0 and MIT, the same terms as the project.
