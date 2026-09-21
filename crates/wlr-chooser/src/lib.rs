//! Shared internals for the two front-ends built on the same capture-fed overlay:
//! `wlr-chooser` (the xdg-desktop-portal-wlr picker) and `wlr-switcher` (the
//! window switcher / Alt-Tab / exposé). Both bind the [`ui`] egui app to the
//! [`shell`] layer-shell host; the binaries differ only in their CLI and in what
//! they do with the picked source (print a token vs. focus the window).

pub mod chooser_cli;
pub mod daemon;
pub mod hints;
mod i18n;
pub mod overlayd;
pub mod shell;
pub mod switcher_cli;
pub mod ui;

use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex, mpsc};
use std::time::Instant;
use wlr_capture::theme;

/// Window order (CLI mirror of [`ui::Order`]), shared by both front-ends.
#[derive(Clone, Copy, clap::ValueEnum)]
pub(crate) enum OrderArg {
    ByName,
    Mru,
}

/// Presentation (CLI mirror of [`ui::View`]), shared by both front-ends so `--layout`
/// means the same thing wherever it is accepted.
#[derive(Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub(crate) enum LayoutArg {
    /// macOS-style single row of tiles.
    Strip,
    /// Full-screen mission-control exposé grid.
    Grid,
    /// Centred rofi-like card with tabs + search.
    Card,
}

impl From<LayoutArg> for ui::View {
    fn from(v: LayoutArg) -> Self {
        match v {
            LayoutArg::Strip => ui::View::Strip,
            LayoutArg::Grid => ui::View::Grid,
            LayoutArg::Card => ui::View::Card,
        }
    }
}

/// Which physical keyboard row the tile hints come from (CLI mirror of
/// [`hints::HintRow`]), shared by both front-ends.
#[derive(Clone, Copy, clap::ValueEnum)]
pub(crate) enum HintRowArg {
    /// The home row — `asdfghjkl` on QWERTY.
    Home,
    /// The row above the letters — `1234567890` on QWERTY.
    Top,
}

impl From<HintRowArg> for hints::HintRow {
    fn from(v: HintRowArg) -> Self {
        match v {
            HintRowArg::Home => hints::HintRow::Home,
            HintRowArg::Top => hints::HintRow::Top,
        }
    }
}

/// Say so and exit when tile hints were asked for on the one presentation that cannot
/// carry them.
///
/// The card has a filter field the user types into, where a letter is a letter. Saying
/// so beats accepting a flag that would then do nothing.
pub(crate) fn reject_hints_on_card(
    hints: Option<HintRowArg>,
    layout: LayoutArg,
) -> Result<(), String> {
    if hints.is_some() && layout == LayoutArg::Card {
        return Err(crate::tr!("hints-need-keyboard"));
    }
    Ok(())
}

/// Report a pre-flight refusal the way a one-shot run does: on stderr, exit 2.
///
/// The refusals answer rather than exit because the switcher's daemon has to send
/// them back to the client that asked; a binary running the overlay itself turns them
/// back into an exit status here.
pub(crate) fn exit_with(reason: String) -> ! {
    eprintln!("{reason}");
    std::process::exit(2);
}

impl From<OrderArg> for ui::Order {
    fn from(v: OrderArg) -> Self {
        match v {
            OrderArg::ByName => ui::Order::ByName,
            OrderArg::Mru => ui::Order::Mru,
        }
    }
}

/// The window filter flags, shared by both front-ends so `--app-id`, `--title` and
/// `--pid` mean the same thing wherever they are accepted.
#[derive(clap::Args)]
pub(crate) struct FilterArgs {
    /// Offer only windows with this application id (exact, case-insensitive) — e.g.
    /// `firefox`. Repeatable: a window matching any of the ids is kept. Windows left
    /// out are never captured.
    #[arg(long, value_name = "APP_ID")]
    app_id: Vec<String>,
    /// Offer only windows whose title contains this text (case-insensitive).
    /// Repeatable: a window matching any of the texts is kept. Used together with
    /// --app-id, a window must match both.
    #[arg(long, value_name = "TEXT")]
    title: Vec<String>,
    /// Offer only windows belonging to this process. Every window of the process is
    /// kept. Repeatable: a window belonging to any of the pids is kept. Used together
    /// with --app-id or --title, a window must match both. Needs a compositor that
    /// names the process behind a window (sway, Hyprland, niri).
    #[arg(long, value_name = "PID")]
    pid: Vec<u32>,
}

impl From<FilterArgs> for ui::WindowFilters {
    fn from(a: FilterArgs) -> Self {
        Self::new(a.app_id, a.title, a.pid)
    }
}

/// Read the process behind each open window for a `--pid` filter, and say so and exit
/// when nothing can answer.
///
/// No Wayland protocol carries a pid, so this rests on a compositor IPC that not every
/// compositor has. Ignoring the flag there would be the worst outcome: the overlay
/// would come up holding every window where the user asked for one process's.
pub(crate) fn require_window_pids(
    filters: &mut ui::WindowFilters,
    toplevels: &[wlr_capture::wl::Toplevel],
) -> Result<(), String> {
    if filters.refresh_pids(toplevels) {
        return Ok(());
    }
    Err(crate::tr!("pid-unsupported"))
}

/// Say so and exit when the window filter matches none of the open windows. The caller
/// has nothing but windows to offer, so an empty list is an empty overlay — the same
/// treatment a compositor that cannot capture windows at all gets.
pub(crate) fn reject_empty_window_filter(
    toplevels: &[wlr_capture::wl::Toplevel],
    filters: &ui::WindowFilters,
) -> Result<(), String> {
    if filters.is_empty()
        || toplevels
            .iter()
            .any(|w| filters.admits(ui::Candidate::from(w)))
    {
        return Ok(());
    }
    Err(crate::tr!("filter-no-match", filter = filters.describe()))
}

/// Parse a `COLSxROWS` grid spec (e.g. `4x3`).
pub fn parse_grid(s: &str) -> Result<(u32, u32), String> {
    let (c, r) = s
        .split_once(['x', 'X', '×'])
        .ok_or("expected COLSxROWS, e.g. 4x3")?;
    let n = |v: &str, what: &str| {
        v.trim()
            .parse::<u32>()
            .ok()
            .filter(|&n| n >= 1)
            .ok_or(format!("{what} must be a positive integer"))
    };
    Ok((n(c, "columns")?, n(r, "rows")?))
}

/// Acquire the single-instance advisory lock for the interactive switcher.
/// Returns the held lock file (keep it alive), or `None` if another instance owns
/// it — sway processes its own keybinding even over our exclusive keyboard grab,
/// so re-pressing the bind would otherwise stack overlays.
pub fn acquire_switch_lock() -> Option<std::fs::File> {
    use rustix::fs::{FlockOperation, flock};
    let dir = wlr_capture::paths::runtime_dir();
    let f = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(false)
        .open(dir.join("wlr-switcher.lock"))
        .ok()?;
    flock(&f, FlockOperation::NonBlockingLockExclusive).ok()?;
    Some(f)
}

/// Spawn the capture thread, build the overlay for `opts`, run it to completion on a
/// host of its own, and return the picked source (if any). `t0` is the process start,
/// for cold-start timing (see [`shell::tlog`]).
pub fn run_overlay(opts: ui::Options, t0: Instant) -> anyhow::Result<Option<ui::Selection>> {
    // The capture thread first, the host second: connecting, enumerating and opening
    // sessions is the long pole for the thumbnails, and the host's own connection
    // takes a millisecond it may as well spend in parallel.
    let (app, out) = build_overlay(opts, t0);
    let mut host = shell::Host::new()?;
    shell::tlog(t0, "ui ready, entering overlay");
    host.show(app, t0)?;
    Ok(out.lock().unwrap().take())
}

/// Show one overlay on a host that is already up — the switcher's daemon reusing its
/// warm Wayland connection and EGL context. `t0` is the start of *this* run.
pub fn run_overlay_on(
    host: &mut shell::Host,
    opts: ui::Options,
    t0: Instant,
) -> anyhow::Result<Option<ui::Selection>> {
    let (app, out) = build_overlay(opts, t0);
    shell::tlog(t0, "ui ready, entering overlay");
    host.show(app, t0)?;
    Ok(out.lock().unwrap().take())
}

/// Spawn the capture thread and build the overlay it feeds, with the slot the picked
/// source lands in.
fn build_overlay(mut opts: ui::Options, t0: Instant) -> (ui::App, ui::Outcome) {
    // Before the overlay is up: once it holds the keyboard, no window has focus.
    let (order, focused) = opts.order.resolve();
    // The capture thread owns the window filter: it decides which windows exist at all,
    // and the UI only ever sees the ones it kept.
    let filters = std::mem::take(&mut opts.window_filters);

    // Start capturing first thing: the thread owns the non-Send Wayland client and
    // must connect, enumerate, and open sessions before any thumbnail appears.
    let (tx, rx) = mpsc::channel();
    // Raised by the UI when a dma-buf import fails; the capture thread then
    // reallocates in shm.
    let gpu_failed = Arc::new(AtomicBool::new(false));
    let flag = gpu_failed.clone();
    std::thread::spawn(move || ui::capture_thread(tx, flag, order, filters));
    shell::tlog(t0, "capture-thread spawned");

    let out: ui::Outcome = Arc::new(Mutex::new(None));
    let theme = theme::Theme::load();
    let app = ui::App::new(rx, out.clone(), opts, focused, theme, gpu_failed);
    (app, out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_grid_reads_cols_and_rows() {
        assert_eq!(parse_grid("4x3"), Ok((4, 3)));
        assert_eq!(parse_grid("10X2"), Ok((10, 2)));
        assert_eq!(parse_grid("2×5"), Ok((2, 5)));
        // Surrounding whitespace on either factor is tolerated.
        assert_eq!(parse_grid(" 4 x 3 "), Ok((4, 3)));
    }

    #[test]
    fn parse_grid_rejects_bad_specs() {
        assert!(parse_grid("4").unwrap_err().contains("COLSxROWS"));
        assert!(parse_grid("0x3").unwrap_err().contains("columns"));
        assert!(parse_grid("4x0").unwrap_err().contains("rows"));
        assert!(parse_grid("axb").unwrap_err().contains("columns"));
        assert!(parse_grid("").is_err());
    }
}
