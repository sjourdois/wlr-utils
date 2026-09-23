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
use crate::{acquire_switch_lock, daemon, run_overlay, shell};
use crate::{i18n, tr};
use clap::{Parser, ValueEnum};
use std::io::IsTerminal;
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
    /// (Alt/Super) is released. Default: on for `strip`, off for `grid`/`card`,
    /// and off when run from a terminal, where no modifier is held. Bind it to a
    /// held modifier — e.g. `Mod1+Tab exec wlr-switcher` — for a true Alt-Tab.
    /// Use this to force it on for `grid`/`card`.
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
    /// Show the overlay in this process, even if a `wlr-overlayd` daemon is running —
    /// and without the notice that says none is.
    #[arg(long)]
    no_daemon: bool,
}

/// What a run came to.
enum Ran {
    /// The overlay was answered: a window picked, and focused if it could be.
    Switched,
    /// The user backed out of the overlay.
    Cancelled,
}

pub fn main() {
    let t0 = Instant::now();
    // Kept before clap eats them: what a daemon is handed is the invocation itself,
    // so `wlr-switcher --layout grid` means the same thing whether the daemon shows
    // it or this process does.
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let mut cli = Cli::parse();
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

    // A terminal holds no launch modifier, and hold-to-switch reads that as a tap
    // released before the overlay got the keyboard: it would switch before showing
    // anything. Settled here, where the terminal is, and handed on to a daemon like
    // any flag.
    if !cli.hold && !cli.no_hold && std::io::stdin().is_terminal() {
        cli.no_hold = true;
        args.push("--no-hold".into());
    }
    // Whatever gave hold-to-switch to a run someone is watching, say what it will do
    // with no modifier held.
    if hold(&cli) && std::io::stderr().is_terminal() {
        eprintln!("{}", tr!("hold-no-modifier"));
    }

    // Hand the run to the daemon if the user is running one. None of this starts
    // one: with nothing listening the invocation shows the overlay itself, below,
    // exactly as it always has — and says so, because that is the slow path and the
    // reason it is slow is not otherwise visible.
    if !cli.no_daemon {
        if !daemon_can_serve(&cli, &args) {
            eprintln!("{}", tr!("daemon-bypassed"));
        } else {
            match daemon::request(daemon::Tool::Switch, &args) {
                // An overlay is already up. That is what pressing the keybinding
                // twice has always been: a no-op, not a second overlay.
                Some(daemon::Reply::Busy) => return,
                Some(reply) => {
                    if let daemon::Reply::Err(reason) = &reply {
                        eprintln!("{reason}");
                    }
                    std::process::exit(reply.exit_code());
                }
                None => eprintln!("{}", tr!("daemon-not-running")),
            }
        }
    }

    // Single-instance guard: re-pressing the keybind while we're up is a no-op
    // rather than a stacked overlay (sway runs its bindings over our grab).
    let _lock = match acquire_switch_lock() {
        Some(lock) => lock,
        None => return,
    };

    match run(cli, t0, None) {
        Ok(Ran::Switched) => {}
        Ok(Ran::Cancelled) => std::process::exit(1),
        Err(reason) => crate::exit_with(reason),
    }
}

/// Whether the run switches on the modifier's release: by default for the strip (a
/// true Alt-Tab) and not for the exposé/card; --hold / --no-hold force either.
fn hold(cli: &Cli) -> bool {
    if cli.hold {
        true
    } else if cli.no_hold {
        false
    } else {
        cli.layout == LayoutArg::Strip
    }
}

/// Whether this invocation is one a daemon could take on.
///
/// `--no-gpu` (and `WLR_NO_GPU`) turns off the zero-copy path for the whole process,
/// including the EGL context the daemon built at startup: a daemon started without it
/// cannot honour it, and honouring it halfway would be worse than being slow. Such a
/// run shows its own overlay, as does one whose arguments will not survive the wire.
fn daemon_can_serve(cli: &Cli, args: &[String]) -> bool {
    !cli.no_gpu && std::env::var_os("WLR_NO_GPU").is_none() && daemon::can_encode(args)
}

/// One switcher run, from the pre-flight to the focus change it was for.
///
/// `host` is the daemon's warm host, or `None` to build one for this run alone.
/// Refusals come back as a message rather than exiting the process: the daemon has to
/// send them to the client that asked instead of dying on them.
fn run(cli: Cli, t0: Instant, host: Option<&mut shell::Host>) -> Result<Ran, String> {
    crate::reject_hints_on_card(cli.hints, cli.layout)?;

    let view = View::from(cli.layout);
    let hold = hold(&cli);
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
    preflight(&mut opts)?;

    let picked = match host {
        Some(host) => crate::run_overlay_on(host, opts, t0),
        None => run_overlay(opts, t0),
    }
    .map_err(|e| tr!("error", error = format!("{e:#}")))?;

    let Some(sel) = picked else {
        return Ok(Ran::Cancelled);
    };
    // Focus the picked window (outputs aren't focusable, so ignore them).
    if sel.is_window
        && let Err(e) = wl::activate_window(&sel.identifier, &sel.identity())
    {
        // A compositor with no activation protocol at all is a property of the
        // setup, not a bug in this run: say what is missing, like the pre-flight
        // does for window capture, rather than dumping a protocol name.
        return Err(match e {
            CaptureError::ActivationUnsupported => tr!("focus-unsupported"),
            e => tr!("error", error = format!("{e:#}")),
        });
    }
    Ok(Ran::Switched)
}

/// Settle, before the overlay, everything that decides whether it has anything to
/// show at all.
///
/// wlr-switcher switches *windows*, which need the foreign-toplevel capture source
/// (wlroots >= 0.20 / Sway >= 1.12). On older compositors connect() still succeeds for
/// screen-only capture, but there are no windows to offer — so say so clearly instead
/// of showing an empty dimmed overlay (issue #1).
fn preflight(opts: &mut Options) -> Result<(), String> {
    match wl::Client::connect() {
        Ok(client) if !client.can_capture_windows() => Err(tr!("capture-no-window")),
        Ok(client) => {
            // A --pid filter has to be settled here too: it rests on a compositor IPC,
            // and a filter that cannot be applied must not be applied silently.
            crate::require_window_pids(&mut opts.window_filters, client.toplevels())?;
            // Same reasoning for a filter that names no open window: the switcher shows
            // windows and nothing else, so it would come up empty.
            crate::reject_empty_window_filter(client.toplevels(), &opts.window_filters)
        }
        Err(e) => Err(tr!("error", error = format!("{e:#}"))),
    }
}

/// Serve one `switch` request: the daemon's side of a `wlr-switcher` invocation.
///
/// The arguments are parsed with the very same parser the client used, so a daemon
/// run and a direct run differ in nothing but what they had to build first.
pub(crate) fn serve(host: &mut shell::Host, args: Vec<String>) -> daemon::Reply {
    // This overlay's clock starts here — a few hundred microseconds after the
    // invocation's own, which is all the client spent reaching us.
    let t0 = Instant::now();
    let cli = match Cli::try_parse_from(std::iter::once("wlr-switcher".to_string()).chain(args)) {
        Ok(cli) => cli,
        Err(e) => return daemon::Reply::Err(e.render().to_string()),
    };
    // Runs that are not a daemon's to make. A client settles this before asking (see
    // `daemon_can_serve`), so only a hand-sent line reaches here.
    if cli.no_daemon || cli.no_gpu || cli.doctor {
        return daemon::Reply::Err(tr!("daemon-cannot-serve"));
    }
    // The same single-instance guard a one-shot run takes, and for the same reason:
    // it also keeps a `--no-daemon` run from stacking an overlay on the daemon's.
    let Some(_lock) = acquire_switch_lock() else {
        return daemon::Reply::Busy;
    };
    match run(cli, t0, Some(host)) {
        // The switcher acts on the pick rather than naming it, so there is nothing
        // for the client to print.
        Ok(Ran::Switched) => daemon::Reply::Done(String::new()),
        Ok(Ran::Cancelled) => daemon::Reply::Cancelled,
        Err(reason) => daemon::Reply::Err(reason),
    }
}
