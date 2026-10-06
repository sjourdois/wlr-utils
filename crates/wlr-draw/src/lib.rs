//! `wlr-draw` — draw and annotate live on screen on wlroots compositors.
//!
//! With no subcommand it runs the daemon: a transparent always-on-top overlay you draw
//! on (the `overlay` module). Every other invocation is a one-shot control message sent to
//! the running daemon over a Unix socket (the `ipc` module) — `toggle`, `clear`, `tool arrow`,
//! `color #00ff00`, … — so you bind them to compositor keys.

#[cfg(feature = "tray")]
mod autostart;
mod i18n;
mod ipc;
mod keymap;
mod model;
mod overlay;
mod proto;
#[cfg(feature = "tray")]
mod tray;

use crate::model::{Tool, parse_color};
use crate::proto::Cmd;
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "wlr-draw",
    version = wlr_capture::version!(),
    about = "Draw and annotate live on screen (wlroots / layer-shell)",
    long_about = "Run with no subcommand to start the overlay daemon. A wlroots client \
cannot grab a global hotkey, so further invocations drive the running daemon over a \
control socket — bind them to compositor keys (e.g. sway `bindsym $mod+p exec wlr-draw \
toggle`)."
)]
struct Cli {
    /// Accepted for consistency with the other tools: wlr-draw's freeze-frame and save
    /// always capture through shared memory, so it changes nothing here.
    #[arg(long, global = true)]
    no_gpu: bool,
    #[command(subcommand)]
    cmd: Option<Ctl>,
}

#[derive(Subcommand)]
enum Ctl {
    /// Toggle draw mode (grab input ↔ click-through)
    Toggle,
    /// Enter draw mode (grab input)
    On,
    /// Leave draw mode (click-through; annotations stay on screen)
    Off,
    /// Erase all annotations
    Clear,
    /// Undo the last action
    Undo,
    /// Redo the last undone action
    Redo,
    /// Hide / show the annotations without discarding them
    Visibility,
    /// Toggle the pen's snap-on-dwell (hold still mid-stroke to snap to a clean shape)
    Snap,
    /// Select a tool: pen, rect, mask, arrow, text, eraser, move
    Tool {
        /// Tool name
        name: String,
    },
    /// Set the stroke colour: a name (red, blue…) or #rrggbb[aa]
    Color {
        /// Colour name or hex
        value: String,
    },
    /// Set the stroke width in pixels
    Width {
        /// Width in logical pixels
        px: f32,
    },
    /// Save the annotated screen to a PNG (default: ~/Pictures/wlr-draw-<stamp>.png)
    Save {
        /// Destination path; omit for a timestamped file in your Pictures directory
        path: Option<String>,
    },
    /// Re-read the configuration, keeping the drawing
    Reload,
    /// Stop the running daemon
    Quit,
    /// Report which capture protocols the current compositor supports
    Doctor,
    /// Move the old theme.toml and keys.toml into ~/.config/wlr-utils/config.toml, then
    /// delete them
    MigrateConfig {
        /// `-` prints the new file instead, and touches nothing
        #[arg(value_parser = ["-"])]
        output: Option<String>,
    },
    /// Print the systemd --user unit:
    /// `wlr-draw print-unit > ~/.config/systemd/user/wlr-draw.service`
    PrintUnit,
}

/// The systemd --user unit, for an install that did not put it in place.
const UNIT: &str = include_str!("../contrib/wlr-draw.service");

pub fn main() -> anyhow::Result<()> {
    // Negotiate the UI language from the desktop locale (no-op without the `i18n`
    // feature). Like every other binary in the workspace — without it the tray menu and
    // on-screen hints stay English regardless of `$LANG`.
    crate::i18n::init();
    wlr_config::set_migrate_command("wlr-draw migrate-config");
    let cli = Cli::parse();
    if cli.no_gpu {
        wlr_capture::wl::disable_gpu_globally();
    }
    match cli.cmd {
        None => overlay::run(),
        // Doctor probes the compositor directly — it doesn't drive the daemon.
        Some(Ctl::Doctor) => {
            wlr_capture::doctor::report("wlr-draw", wlr_capture::version!()).map_err(Into::into)
        }
        Some(Ctl::MigrateConfig { output }) => {
            let output = wlr_config::migrate::Output::from_arg(output.as_deref());
            wlr_config::migrate::run(output).map_err(Into::into)
        }
        Some(Ctl::PrintUnit) => {
            print!("{UNIT}");
            Ok(())
        }
        Some(ctl) => ipc::send(&ctl_to_cmd(ctl)?),
    }
}

/// Map a CLI subcommand to a protocol command, validating tool/colour client-side so
/// errors surface before anything is sent.
fn ctl_to_cmd(ctl: Ctl) -> anyhow::Result<Cmd> {
    Ok(match ctl {
        Ctl::Toggle => Cmd::Toggle,
        Ctl::On => Cmd::On,
        Ctl::Off => Cmd::Off,
        Ctl::Clear => Cmd::Clear,
        Ctl::Undo => Cmd::Undo,
        Ctl::Redo => Cmd::Redo,
        Ctl::Visibility => Cmd::Visibility,
        Ctl::Snap => Cmd::Snap,
        Ctl::Reload => Cmd::Reload,
        Ctl::Quit => Cmd::Quit,
        Ctl::Tool { name } => Cmd::Tool(
            Tool::from_name(&name).ok_or_else(|| anyhow::anyhow!("unknown tool: {name}"))?,
        ),
        Ctl::Color { value } => Cmd::Color(
            parse_color(&value).ok_or_else(|| anyhow::anyhow!("unknown colour: {value}"))?,
        ),
        Ctl::Width { px } => Cmd::Width(px),
        // The daemon writes the file from its own directory: a relative path has to
        // mean the caller's.
        Ctl::Save { path } => Cmd::Save(path.as_deref().map(absolute).transpose()?),
        // Not daemon commands — handled directly in `main` before we get here.
        Ctl::Doctor | Ctl::MigrateConfig { .. } | Ctl::PrintUnit => {
            unreachable!("handled before ctl_to_cmd")
        }
    })
}

/// `path` resolved against this process's current directory.
fn absolute(path: &str) -> anyhow::Result<String> {
    std::path::absolute(path)?
        .into_os_string()
        .into_string()
        .map_err(|p| anyhow::anyhow!("not a UTF-8 path: {}", p.display()))
}

#[cfg(test)]
mod tests {
    use super::{Cli, Ctl, UNIT, absolute};
    use clap::{CommandFactory, Parser};

    #[test]
    fn print_unit_prints_the_shipped_unit() {
        Cli::command().debug_assert();
        let cli = Cli::try_parse_from(["wlr-draw", "print-unit"]).unwrap();
        assert!(matches!(cli.cmd, Some(Ctl::PrintUnit)));
        assert!(UNIT.lines().any(|l| l == "ExecStart=/usr/bin/env wlr-draw"));
    }

    #[test]
    fn a_relative_save_path_is_the_callers() {
        let cwd = std::env::current_dir().unwrap();
        assert_eq!(
            absolute("shot.png").unwrap(),
            cwd.join("shot.png").to_str().unwrap()
        );
        assert_eq!(absolute("/tmp/shot.png").unwrap(), "/tmp/shot.png");
    }
}
