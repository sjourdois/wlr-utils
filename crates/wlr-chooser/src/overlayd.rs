//! `wlr-overlayd` — the shared overlay daemon behind `wlr-switcher` and `wlr-chooser`.
//!
//! It holds what an overlay costs to build and throws away: the Wayland connection,
//! the EGL context with its compiled shaders, the glyph atlas. An invocation of either
//! front-end hands it the run over a control socket ([`crate::daemon`]) and gets back
//! what to print and what to exit on; the overlay is on screen in about ten
//! milliseconds instead of ninety.
//!
//! One daemon serves both because they *are* one overlay — the same egui app on the
//! same engine, differing only in what they do with the pick. Two daemons would warm
//! two contexts for it.
//!
//! It shows one overlay at a time, and nothing of it is on screen in between: no
//! surface, no keyboard held, no capture running.

use crate::daemon::{self, Reply, Tool};
use crate::i18n;
use crate::shell::Host;
use clap::Parser;
use std::time::Instant;

/// The overlay daemon for wlr-switcher and wlr-chooser.
#[derive(Parser)]
#[command(
    name = "wlr-overlayd",
    version = wlr_capture::version!(),
    about = "Overlay daemon for wlr-switcher and wlr-chooser (instant overlays)",
    long_about = "Runs in the foreground holding the Wayland connection and the GPU \
context an overlay would otherwise build from scratch every time — some ninety \
milliseconds, paid once here instead of at every overlay.\n\n\
Start it with your session and nothing else changes: `wlr-switcher` and `wlr-chooser` \
find it on their own, and work exactly as before when it is not running.\n\n\
    sway:             exec wlr-overlayd\n\
    Hyprland (.lua):  hl.on(\"hyprland.start\", function () hl.exec_cmd(\"wlr-overlayd\") end)\n\
    Hyprland (.conf): exec-once = wlr-overlayd\n\
    niri:             spawn-at-startup \"wlr-overlayd\"\n\n\
A systemd --user unit comes with it: `wlr-overlayd --print-unit` prints it. Nothing is \
captured while the daemon waits. To show an overlay in its own process anyway, pass --no-daemon \
to wlr-switcher or wlr-chooser."
)]
struct Cli {
    /// Capture through shared memory instead of the zero-copy dma-buf path, for
    /// every overlay this daemon shows. Use it if previews come out broken on your
    /// driver; also settable with WLR_NO_GPU=1.
    #[arg(long)]
    no_gpu: bool,
    /// Stop the running daemon, then exit.
    #[arg(long, conflicts_with = "no_gpu")]
    quit: bool,
    /// Print the systemd --user unit, then exit:
    /// `wlr-overlayd --print-unit > ~/.config/systemd/user/wlr-overlayd.service`.
    #[arg(long, conflicts_with_all = ["no_gpu", "quit"])]
    print_unit: bool,
}

/// The systemd --user unit, for an install that did not put it in place.
const UNIT: &str = include_str!("../contrib/wlr-overlayd.service");

pub fn main() {
    let t0 = Instant::now();
    let cli = Cli::parse();
    if cli.print_unit {
        print!("{UNIT}");
        return;
    }
    i18n::init();
    wlr_config::set_migrate_command("wlr-chooser --migrate-config");

    if cli.quit {
        if let Err(e) = daemon::quit() {
            eprintln!("{e:#}");
            std::process::exit(2);
        }
        return;
    }

    if cli.no_gpu {
        wlr_capture::wl::disable_gpu_globally();
    }

    if let Err(e) = daemon::run(t0, serve) {
        // No tool-name prefix: the messages name the daemon where it matters, and
        // this goes to whatever the session autostart points its output at — a
        // journal already headed by the unit, more often than not.
        eprintln!("{e:#}");
        std::process::exit(2);
    }
}

/// Serve one request: run it as the front-end that asked.
fn serve(host: &mut Host, tool: Tool, args: Vec<String>) -> Reply {
    match tool {
        Tool::Switch => crate::switcher_cli::serve(host, args),
        Tool::Choose => crate::chooser_cli::serve(host, args),
    }
}

#[cfg(test)]
mod tests {
    use super::{Cli, UNIT};
    use clap::{CommandFactory, Parser};

    #[test]
    fn print_unit_prints_the_shipped_unit() {
        Cli::command().debug_assert();
        assert!(
            Cli::try_parse_from(["wlr-overlayd", "--print-unit"])
                .unwrap()
                .print_unit
        );
        assert!(Cli::try_parse_from(["wlr-overlayd", "--print-unit", "--quit"]).is_err());
        assert!(
            UNIT.lines()
                .any(|l| l == "ExecStart=/usr/bin/env wlr-overlayd")
        );
    }
}
