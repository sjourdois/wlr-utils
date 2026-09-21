//! wlr-switcher — window switcher / Alt-Tab / exposé for wlroots compositors.
//!
//! Picks a window from a live overlay and **focuses** it (via
//! `zwlr-foreign-toplevel-management-v1`). Bind it to a held modifier for a true
//! Alt-Tab: hold the modifier, `Tab`/`Shift+Tab` cycle, release to switch. Three
//! presentations via `--layout`; live previews are the differentiator.
//!
//! This binary *acts*: it focuses what it picked. For the sibling that *answers* —
//! naming the source on stdout and touching nothing — see `wlr-chooser`.

use crate::ui::{Live, Mode, Options, View};
use crate::{FilterArgs, HintRowArg, LayoutArg, OrderArg};
use crate::{acquire_switch_lock, run_overlay};
use crate::{i18n, tr};
use clap::{Parser, ValueEnum};
use std::time::Instant;
use wlr_capture::{CaptureError, wl};

/// Which tiles show a live preview (CLI mirror of [`Live`]).
#[derive(Clone, Copy, ValueEnum)]
enum LiveArg {
    None,
    Current,
    All,
}

impl From<LiveArg> for Live {
    fn from(v: LiveArg) -> Self {
        match v {
            LiveArg::None => Live::None,
            LiveArg::Current => Live::Current,
            LiveArg::All => Live::All,
        }
    }
}

/// Window switcher / Alt-Tab / exposé for wlroots: focuses the picked window.
#[derive(Parser)]
#[command(
    name = "wlr-switcher",
    version,
    about = "Window switcher / Alt-Tab / exposé for wlroots (focuses the picked window)"
)]
struct Cli {
    /// Capture through shared memory instead of the zero-copy dma-buf path.
    /// Use it if previews or captures come out broken on your driver; also
    /// settable with WLR_NO_GPU=1.
    #[arg(long)]
    no_gpu: bool,
    /// Presentation: `strip` (macOS-style row, default), `grid` (full-screen
    /// exposé) or `card` (centred rofi-like card).
    #[arg(long, value_enum, default_value = "strip")]
    layout: LayoutArg,
    /// Live previews: `none` (icons only), `current` (only the highlighted window)
    /// or `all` (default). Live capture is the differentiator.
    #[arg(long, value_enum, default_value_t = LiveArg::All)]
    live: LiveArg,
    /// Window order: `mru` (most recently focused first, if supported by the
    /// compositor; default) or `by-name`.
    #[arg(long, value_enum, default_value_t = OrderArg::Mru)]
    window_order: OrderArg,
    /// Hold-to-switch: confirm and close the moment the held launch modifier
    /// (Alt/Super) is released. Default: on for `strip`, off for `grid`/`card`.
    /// Bind it to a held modifier — e.g. `Mod1+Tab exec wlr-switcher` — for a
    /// true Alt-Tab. Use this to force it on for `grid`/`card`.
    #[arg(long)]
    hold: bool,
    /// Disable hold-to-switch: the overlay stays open after releasing the
    /// modifier — confirm with Enter or a click. Overrides the per-layout default.
    #[arg(long, conflicts_with = "hold")]
    no_hold: bool,
    /// Label each tile with the key that picks it, taken from a row of the physical
    /// keyboard: `home` (the resting row, default) or `top` (the row above the
    /// letters). The label is whatever the active layout prints on that key, so it
    /// always names the key to press. Off unless asked for; needs a layout with no
    /// filter field (`strip` or `grid`).
    #[arg(long, value_enum, value_name = "ROW", num_args = 0..=1, default_missing_value = "home")]
    hints: Option<HintRowArg>,
    /// Include windows with no app-id (system surfaces)
    #[arg(long)]
    include_system: bool,
    #[command(flatten)]
    filters: FilterArgs,
    /// Report which capture protocols the current compositor supports, then exit.
    #[arg(long)]
    doctor: bool,
}

pub fn main() {
    let t0 = Instant::now();
    let cli = Cli::parse();
    if cli.no_gpu {
        wlr_capture::wl::disable_gpu_globally();
    }
    i18n::init();

    if cli.doctor {
        if let Err(e) = wlr_capture::doctor::report("wlr-switcher", env!("CARGO_PKG_VERSION")) {
            eprintln!("wlr-switcher: {e}");
            std::process::exit(1);
        }
        return;
    }

    crate::reject_hints_on_card(cli.hints, cli.layout);

    // Single-instance guard: re-pressing the keybind while we're up is a no-op
    // rather than a stacked overlay (sway runs its bindings over our grab).
    let _lock = match acquire_switch_lock() {
        Some(lock) => lock,
        None => return,
    };

    let view = View::from(cli.layout);
    // Hold-to-switch defaults on for the strip (a true Alt-Tab) and off for the
    // exposé/card; --hold / --no-hold force either.
    let hold = if cli.hold {
        true
    } else if cli.no_hold {
        false
    } else {
        cli.layout == LayoutArg::Strip
    };
    let mut opts = Options {
        mode: Mode::Windows,
        show_system: cli.include_system,
        grid: None,
        view,
        hold,
        live: cli.live.into(),
        order: cli.window_order.into(),
        window_filters: cli.filters.into(),
        hints: cli.hints.map(Into::into),
    };

    // Pre-flight: wlr-switcher switches *windows*, which need the foreign-toplevel
    // capture source (wlroots >= 0.20 / Sway >= 1.12). On older compositors connect()
    // now succeeds for screen-only capture, but there are no windows to offer — so say
    // so clearly and exit, instead of showing an empty dimmed overlay (issue #1).
    match wl::Client::connect() {
        Ok(client) if !client.can_capture_windows() => {
            eprintln!("{}", tr!("capture-no-window"));
            std::process::exit(2);
        }
        Ok(client) => {
            // A --pid filter has to be settled here too: it rests on a compositor IPC,
            // and a filter that cannot be applied must not be applied silently.
            crate::require_window_pids(&mut opts.window_filters, client.toplevels());
            // Same reasoning for a filter that names no open window: the switcher shows
            // windows and nothing else, so it would come up empty.
            crate::reject_empty_window_filter(client.toplevels(), &opts.window_filters);
        }
        Err(e) => {
            eprintln!("{}", tr!("error", error = format!("{e:#}")));
            std::process::exit(2);
        }
    }

    match run_overlay(opts, t0) {
        Ok(Some(sel)) => {
            // Focus the picked window (outputs aren't focusable, so ignore them).
            if sel.is_window
                && let Err(e) = wl::activate_window(&sel.identifier, &sel.identity())
            {
                // A compositor with no activation protocol at all is a property of the
                // setup, not a bug in this run: say what is missing, like the pre-flight
                // does for window capture, rather than dumping a protocol name.
                let msg = match e {
                    CaptureError::ActivationUnsupported => tr!("focus-unsupported"),
                    e => tr!("error", error = format!("{e:#}")),
                };
                eprintln!("{msg}");
                std::process::exit(2);
            }
        }
        Ok(None) => std::process::exit(1), // cancelled
        Err(e) => {
            eprintln!("{}", tr!("error", error = format!("{e:#}")));
            std::process::exit(2);
        }
    }
}
