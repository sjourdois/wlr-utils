//! wlr-chooser — graphical window & screen picker for wlroots screencast portals.
//!
//! Output contract (stdout) expected by xdg-desktop-portal-wlr:
//! `Window: <foreign-toplevel-identifier>` or `Monitor: <output-name>`.
//! On cancel: no output, non-zero exit.
//!
//! For an interactive window switcher / Alt-Tab / exposé, see the sibling
//! `wlr-switcher` binary.

use crate::ui::{self, Live, Mode, Options, View};
use crate::{FilterArgs, OrderArg, parse_grid, run_overlay};
use crate::{i18n, tr};
use clap::Parser;
use std::time::Instant;

/// Graphical window & screen picker for xdg-desktop-portal-wlr.
///
/// Prints the chosen source to stdout (`Window: <id>` / `Monitor: <name>`); exits
/// non-zero if cancelled.
#[derive(Parser)]
#[command(name = "wlr-chooser", version, about)]
struct Cli {
    /// Capture through shared memory instead of the zero-copy dma-buf path.
    /// Use it if previews or captures come out broken on your driver; also
    /// settable with WLR_NO_GPU=1.
    #[arg(long)]
    no_gpu: bool,
    /// Show only windows
    #[arg(short = 'w', long, group = "what")]
    windows: bool,
    /// Show only screens
    #[arg(short = 'o', long, visible_alias = "screens", group = "what")]
    outputs: bool,
    /// Show both windows and screens (default)
    #[arg(long, group = "what")]
    both: bool,
    /// Include windows with no app-id (system surfaces)
    #[arg(long)]
    include_system: bool,
    #[command(flatten)]
    filters: FilterArgs,
    /// Show a fixed COLSxROWS grid of thumbnails (e.g. 4x3)
    #[arg(long, value_name = "COLSxROWS", value_parser = parse_grid)]
    grid: Option<(u32, u32)>,
    /// Order windows `by-name` (default) or `mru` (most recently focused first, if
    /// supported by the compositor)
    #[arg(long, value_enum, default_value_t = OrderArg::ByName)]
    window_order: OrderArg,
    /// Headless capture benchmark: run the capture loop for SECS seconds and
    /// print per-source frame/change stats to stderr (debug; no overlay).
    #[arg(long, value_name = "SECS", hide = true)]
    bench_capture: Option<u64>,
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
        if let Err(e) = wlr_capture::doctor::report("wlr-chooser", env!("CARGO_PKG_VERSION")) {
            eprintln!("wlr-chooser: {e}");
            std::process::exit(1);
        }
        return;
    }

    let window_filters: ui::WindowFilters = cli.filters.into();

    if let Some(secs) = cli.bench_capture {
        ui::bench_capture(secs, window_filters);
        return;
    }

    let _ = cli.both; // default; accepted for symmetry with -w/-o
    let mode = if cli.windows {
        Mode::Windows
    } else if cli.outputs {
        Mode::Outputs
    } else {
        Mode::All
    };
    let mut opts = Options {
        mode,
        show_system: cli.include_system,
        grid: cli.grid,
        view: View::Card,
        hold: false,
        live: Live::All,
        order: cli.window_order.into(),
        window_filters,
    };

    // Two reasons to connect before the overlay. A --pid filter needs the compositor to
    // name the process behind each window, whatever the mode, and must say so rather
    // than come up unapplied. And screens ignore the window filter, so only a
    // windows-only run can be emptied by it. Anything else opens the overlay straight
    // away, paying no extra connection.
    if opts.window_filters.needs_pids()
        || (mode == Mode::Windows && !opts.window_filters.is_empty())
    {
        match wlr_capture::wl::Client::connect() {
            Ok(client) => {
                crate::require_window_pids(&mut opts.window_filters, client.toplevels());
                if mode == Mode::Windows {
                    crate::reject_empty_window_filter(client.toplevels(), &opts.window_filters);
                }
            }
            Err(e) => {
                eprintln!("{}", tr!("error", error = format!("{e:#}")));
                std::process::exit(2);
            }
        }
    }

    match run_overlay(opts, t0) {
        Ok(Some(sel)) => {
            // Portal contract: print the chosen source.
            println!("{}", sel.token);
        }
        Ok(None) => std::process::exit(1), // cancelled
        Err(e) => {
            eprintln!("{}", tr!("error", error = format!("{e:#}")));
            std::process::exit(2);
        }
    }
}
