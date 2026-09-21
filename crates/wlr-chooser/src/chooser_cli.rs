//! wlr-chooser — graphical window & screen picker for wlroots screencast portals.
//!
//! Output contract (stdout) expected by xdg-desktop-portal-wlr:
//! `Window: <foreign-toplevel-identifier>` or `Monitor: <output-name>`.
//! On cancel: no output, non-zero exit. `--format json` answers the same question in a
//! form a script can act on.
//!
//! This binary *answers*: it names the source and never touches it. For the sibling
//! that *acts* — focusing the window it picked — see `wlr-switcher`.

use crate::ui::{self, Live, Mode, Options, View};
use crate::{FilterArgs, HintRowArg, LayoutArg, OrderArg, parse_grid, run_overlay};
use crate::{i18n, tr};
use clap::{Parser, ValueEnum};
use std::time::Instant;
use wlr_capture::focus;

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
    /// Presentation: `card` (centred rofi-like card with tabs + search, default),
    /// `grid` (full-screen exposé) or `strip` (macOS-style row of tiles). The
    /// portal runs the chooser with no argument, so it always gets the card.
    #[arg(long, value_enum, default_value = "card")]
    layout: LayoutArg,
    /// Label each tile with the key that picks it, taken from a row of the physical
    /// keyboard: `home` (the resting row, default) or `top` (the row above the
    /// letters). The label is whatever the active layout prints on that key, so it
    /// always names the key to press. Off unless asked for; needs `--layout grid`
    /// or `--layout strip`, which have no filter field.
    #[arg(long, value_enum, value_name = "ROW", num_args = 0..=1, default_missing_value = "home")]
    hints: Option<HintRowArg>,
    /// How the picked source is written on stdout: `portal` (default) or `json`.
    #[arg(long, value_enum, value_name = "FORMAT", default_value = "portal")]
    format: FormatArg,
    /// Show a fixed COLSxROWS grid of thumbnails (e.g. 4x3). Sizes the card, so it
    /// needs the default `--layout card`.
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

/// How the picked source is written on stdout.
///
/// A value rather than a flag: the portal contract gains a name instead of being the
/// nameless default, and a format a script asks for next is one more value, not one
/// more flag. Each value is a contract, so there are only the two that earn one.
#[derive(Clone, Copy, ValueEnum)]
enum FormatArg {
    /// `Window: <identifier>` / `Monitor: <name>`, the line xdg-desktop-portal-wlr
    /// reads. The default: the portal runs the chooser with no argument.
    Portal,
    /// One JSON object on one line, naming what a script can act on.
    Json,
}

/// The process behind the picked window, where the compositor names one.
///
/// No Wayland protocol carries a pid, so this is compositor IPC or nothing — the same
/// source `--pid` filters on, queried only for a `--format json` run that picked a
/// window.
///
/// Where nothing can answer, the key is simply absent from the object. A `--pid`
/// *filter* that cannot be applied has to refuse — an unapplied filter would offer
/// every window where one process was asked for — but a field that cannot be read is
/// merely one field fewer, and the script sees its absence in the answer.
fn window_pid(sel: &ui::Selection) -> Option<u32> {
    if !sel.is_window {
        return None;
    }
    focus::detect()
        .and_then(|b| b.window_pids())
        .and_then(|pids| pids.get(&sel.identifier).copied())
}

/// The picked source as `--format json` writes it: one object on one line.
///
/// The portal's `Window: <identifier>` line names the source in the only terms the
/// portal needs, and that identifier is opaque to everything else — no compositor IPC
/// and no tool in this suite takes one. So a script gets what it can act on instead:
/// the app id and title `wlr-shot --app-id` / `--title` take, the pid it can signal,
/// the output name a screen is addressed by. `type` says which set is present, and
/// `pid` is absent rather than null when the compositor names none.
///
/// The field names are a contract: they may gain company, never change meaning. Their
/// order in the object is not — JSON objects are unordered, and a reader addresses a
/// field by name.
fn json(sel: &ui::Selection, pid: Option<u32>) -> String {
    let mut o = serde_json::Map::new();
    if sel.is_window {
        o.insert("type".into(), "window".into());
        o.insert("identifier".into(), sel.identifier.clone().into());
        o.insert("app-id".into(), sel.app_id.clone().into());
        o.insert("title".into(), sel.title.clone().into());
        if let Some(pid) = pid {
            o.insert("pid".into(), pid.into());
        }
    } else {
        o.insert("type".into(), "screen".into());
        // The output name is what addresses a screen everywhere else (`wlr-shot -o`,
        // `swaymsg output`), and it is the only thing a screen has to give.
        o.insert("name".into(), sel.output.clone().into());
    }
    serde_json::Value::Object(o).to_string()
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

    crate::reject_hints_on_card(cli.hints, cli.layout);
    // --grid sizes the card and nothing else: the exposé lays itself out to fill the
    // screen, the strip is one row by definition. A flag with no effect would look
    // applied.
    if cli.grid.is_some() && cli.layout != LayoutArg::Card {
        eprintln!("{}", tr!("grid-needs-card"));
        std::process::exit(2);
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
        view: View::from(cli.layout),
        hold: false,
        live: Live::All,
        order: cli.window_order.into(),
        window_filters,
        hints: cli.hints.map(Into::into),
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
        Ok(Some(sel)) => println!(
            "{}",
            match cli.format {
                FormatArg::Portal => sel.token.clone(),
                FormatArg::Json => json(&sel, window_pid(&sel)),
            }
        ),
        Ok(None) => std::process::exit(1), // cancelled
        Err(e) => {
            eprintln!("{}", tr!("error", error = format!("{e:#}")));
            std::process::exit(2);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The picked window, as the overlay hands it back.
    fn window(identifier: &str, app_id: &str, title: &str) -> ui::Selection {
        ui::Selection {
            token: format!("Window: {identifier}"),
            is_window: true,
            identifier: identifier.into(),
            app_id: app_id.into(),
            title: title.into(),
            output: String::new(),
            dup_index: 0,
        }
    }

    /// The picked screen, as the overlay hands it back.
    fn screen(name: &str) -> ui::Selection {
        ui::Selection {
            token: format!("Monitor: {name}"),
            is_window: false,
            identifier: String::new(),
            app_id: String::new(),
            title: String::new(),
            output: name.into(),
            dup_index: 0,
        }
    }

    #[test]
    fn the_json_line_names_what_a_script_can_act_on() {
        // The app id and title are what `wlr-shot --app-id` / `--title` take, the pid
        // is what a script can signal; the identifier alone would be opaque to all of
        // them.
        assert_eq!(
            json(&window("ext-7", "foot", "vim"), Some(4242)),
            r#"{"app-id":"foot","identifier":"ext-7","pid":4242,"title":"vim","type":"window"}"#
        );
        // A compositor that names no process leaves the key out, so a script can tell
        // an unknown pid from one it could read.
        assert_eq!(
            json(&window("ext-7", "foot", "vim"), None),
            r#"{"app-id":"foot","identifier":"ext-7","title":"vim","type":"window"}"#
        );
        // A screen is addressed by its output name and has nothing else to give.
        assert_eq!(
            json(&screen("DP-4"), None),
            r#"{"name":"DP-4","type":"screen"}"#
        );
    }

    #[test]
    fn a_json_line_survives_whatever_an_application_puts_in_its_title() {
        // Titles are the application's to write. One object on one line, whatever it
        // holds — which is the whole reason this is JSON and not a field separator.
        let out = json(&window("ext-1", "firefox", "a\"b\nc\\d"), None);
        assert_eq!(out.lines().count(), 1);
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&out).unwrap()["title"],
            "a\"b\nc\\d"
        );
    }

    #[test]
    fn the_portal_line_is_what_a_plain_run_still_prints() {
        // xdg-desktop-portal-wlr runs the chooser with no argument and reads this; a
        // new flag must not move it.
        assert_eq!(window("ext-7", "foot", "vim").token, "Window: ext-7");
        assert_eq!(screen("DP-4").token, "Monitor: DP-4");
    }
}
