# wlr-config

[![CI](https://github.com/sjourdois/wlr-utils/actions/workflows/ci.yml/badge.svg)](https://github.com/sjourdois/wlr-utils/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/wlr-config.svg)](https://crates.io/crates/wlr-config)
[![docs.rs](https://docs.rs/wlr-config/badge.svg)](https://docs.rs/wlr-config)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](#license)

The configuration of the [wlr-utils](https://github.com/sjourdois/wlr-utils) tools: one
`config.toml` for all of them, its themes, and the move from the files that came before.

- `load()` finds the file and reads it, resolving `[theme]` against the installed themes.
  Each tool then takes its own section with `Config::section`.
- Nothing fails: a setting that cannot be read keeps its default, and why becomes a
  localised warning, which the tool prints with `Config::report`.
- `migrate::run` writes `config.toml` from the old `wlr-chooser/theme.toml` and
  `wlr-draw/keys.toml`, comments included, then deletes them.

The file it reads, and every setting, are described in the
[main README](https://github.com/sjourdois/wlr-utils#configuration) and the
[example](https://github.com/sjourdois/wlr-utils/blob/main/docs/config.toml).

## License

Licensed under either of [Apache-2.0](../../LICENSE-APACHE) or
[MIT](../../LICENSE-MIT) at your option.
