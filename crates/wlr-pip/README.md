# wlr-pip (deprecated)

[![crates.io](https://img.shields.io/crates/v/wlr-pip.svg)](https://crates.io/crates/wlr-pip)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](#license)

**`wlr-pip` has moved into [`wlr-peek`](../wlr-peek).**

The floating, always-on-top live mirror is now a `wlr-peek` subcommand:

```console
$ wlr-peek mirror <ID>             # window picture-in-picture (what wlr-pip did)
$ wlr-peek mirror -g "X,Y WxH"     # live magnifier of a screen region
```

This crate is kept only as a thin pointer (the name is published); the `wlr-pip`
binary prints this notice and exits non-zero. It will be removed in a future release.

## License

MIT OR Apache-2.0.
