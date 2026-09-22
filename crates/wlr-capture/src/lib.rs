//! `wlr-capture` — the reusable bricks behind the wlr-utils tools.
//!
//! Always available (headless-friendly, no egui/EGL display deps):
//! - [`wl`]: native Wayland client that enumerates foreign toplevels + outputs and
//!   captures them (full-resolution, zero-copy GPU dma-buf path) via
//!   `ext-image-copy-capture`.
//! - [`clipboard`]: put a captured blob on the wlroots clipboard (`data-control`).
//! - [`gl`]: EGL/GL dma-buf import + headless readback ([`gl::GpuReadback`]).
//! - [`sink`]: the [`sink::FrameSink`] seam shared by screenshot/record/timelapse,
//!   with GPU dma-buf readback under the default path.
//! - [`stream`]: the shared live-capture session driver (arm/poll/reopen/give-up),
//!   used by the mirror, recorder and change monitor.
//! - [`diff`]: a frame-difference metric for change detection.
//!
//! Behind the `compose` feature (resamples with `image`):
//! - [`capture`]: resolve a source (output/window/region) to a [`wl::CapturedImage`],
//!   compositing across mixed-scale outputs. Shared by `wlr-shot` and `wlr-peek`.
//!
//! Behind the `video` feature (links system FFmpeg via `ffmpeg-next`, headless):
//! - [`video`]: a [`sink::FrameSink`] that encodes a capture stream to a file with a
//!   pluggable hardware/software backend (NVENC / VAAPI / libx264).
//!
//! Behind the `pointer` feature (needs a windowing host, sctk):
//! - [`pointer`](mod@pointer): a seat's pointer and the cursor image it sets — the `wl_pointer`
//!   plus its `cursor-shape-v1` device.
//!
//! Behind the `input` feature (needs egui's key names):
//! - [`keys`]: keyboard pieces an overlay can share rather than spell out — today, a
//!   keystroke in the form a user can write down.
//!
//! Behind the `focus` feature (compositor IPC, pulls `serde_json`):
//! - [`focus`]: "the active window" / "the current output" via the compositor's own
//!   IPC (Sway today). Wayland gives no portable way to query focus.
//!
//! This crate carries **no localised UI strings**: it exposes an API and typed
//! [`CaptureError`]s, and any on-screen text (e.g. the overlay hints, the mirror labels)
//! is passed in by the caller. Each tool crate owns its own catalog via `wlr-i18n`.
//!
//! Behind the `toolkit` feature (on by default) — the egui/EGL overlay toolkit:
//! - [`render`]: egui → `egui_glow` rendering on an EGL context bound to a surface.
//! - `icons` / `theme`: shared overlay UI helpers.
//!
//! Consumers (`wlr-chooser`, `wlr-pip`, …) build their own windowing host on top
//! and reuse this engine for the heavy lifting; a future headless recorder can use
//! the capture engine + readback without pulling in the toolkit.

// `doc_cfg` renders the "Available on crate feature X" badges on docs.rs (nightly-only,
// so guarded by the `docsrs` cfg that docs.rs sets via rustdoc-args).
#![cfg_attr(docsrs, feature(doc_cfg))]
// Keep the public API fully documented (the whole surface ships on docs.rs).
#![warn(missing_docs)]

// Re-exported so consumers can own a single `Connection` and pass it to several
// overlays in one process (e.g. the region selector then a live mirror), sharing one
// `EGLDisplay` instead of opening a second one. See `overlay::select_region_on`.
pub use wayland_client::Connection;

/// The version of the wlr-utils git checkout this crate was built from: the release
/// number on a release tag (`1.9.0`), `git describe` on any other commit
/// (`1.9.0-17-g7a434c1`), or the crate version plus the commit when no tag is reachable
/// (`1.9.0-g7a434c1`). `None` outside such a checkout (crates.io, a distro tarball) or
/// without git. Read it through [`version!`].
pub const GIT_VERSION: Option<&str> = option_env!("WLR_GIT_VERSION");

/// The version a tool reports (`--version`, `doctor`), as a `&'static str`:
/// [`GIT_VERSION`] when known, otherwise the calling crate's `CARGO_PKG_VERSION`.
#[macro_export]
macro_rules! version {
    () => {
        match $crate::GIT_VERSION {
            Some(version) => version,
            None => env!("CARGO_PKG_VERSION"),
        }
    };
}

pub mod clipboard;
mod cosmic_activate;
mod cosmic_protocol;
pub mod diff;
pub mod doctor;
pub mod error;
pub mod gl;
pub use error::CaptureError;
pub mod paths;
pub mod sink;
pub mod stream;
pub mod wl;

#[cfg(feature = "compose")]
#[cfg_attr(docsrs, doc(cfg(feature = "compose")))]
pub mod capture;

#[cfg(feature = "focus")]
#[cfg_attr(docsrs, doc(cfg(feature = "focus")))]
pub mod focus;

#[cfg(feature = "input")]
#[cfg_attr(docsrs, doc(cfg(feature = "input")))]
pub mod keys;

#[cfg(feature = "pointer")]
#[cfg_attr(docsrs, doc(cfg(feature = "pointer")))]
pub mod pointer;

#[cfg(feature = "overlay")]
#[cfg_attr(docsrs, doc(cfg(feature = "overlay")))]
pub mod overlay;

#[cfg(feature = "mirror")]
#[cfg_attr(docsrs, doc(cfg(feature = "mirror")))]
pub mod mirror;

#[cfg(feature = "video")]
#[cfg_attr(docsrs, doc(cfg(feature = "video")))]
pub mod video;

#[cfg(feature = "audio")]
#[cfg_attr(docsrs, doc(cfg(feature = "audio")))]
pub mod audio;

#[cfg(feature = "toolkit")]
#[cfg_attr(docsrs, doc(cfg(feature = "toolkit")))]
pub mod icons;
#[cfg(feature = "toolkit")]
#[cfg_attr(docsrs, doc(cfg(feature = "toolkit")))]
pub mod render;
#[cfg(feature = "toolkit")]
#[cfg_attr(docsrs, doc(cfg(feature = "toolkit")))]
pub mod theme;
