//! The overlay daemon's control socket: how a `wlr-switcher` or `wlr-chooser`
//! invocation hands its run to `wlr-overlayd`.
//!
//! An overlay costs about ninety milliseconds to put on screen, and nearly all of it
//! is initialisation the run throws away: the EGL context and its shaders, the font
//! set, the Wayland connection. A daemon pays that once, at startup, and keeps the
//! host ([`crate::shell::Host`]) alive between overlays; each invocation then only
//! builds its surface and paints a frame, which is about ten.
//!
//! Both front-ends are the same overlay over the same engine, so one daemon serves
//! both and warms one context for the two of them. Nothing starts it on its own — it
//! is `wlr-overlayd`, from a session autostart. When one is listening, an invocation
//! hands it the run and reports its answer ([`request`]); when none is, the
//! invocation shows the overlay itself, exactly as it always has.
//!
//! **Capture is not kept warm.** The capture thread is spawned per overlay and dies
//! with it, so an idle daemon reads no window contents: it holds a Wayland connection
//! and a GPU context, nothing else.
//!
//! The protocol is one line of text, like `wlr-draw`'s: `switch` or `choose` followed
//! by the invocation's arguments, `step next|prev` followed by them, `ping`, or `quit`.
//! The daemon answers one line — `ok`, `ok <stdout line>`, `cancel`, `busy` or
//! `err <reason>` — which the client turns back into its own output and exit status.
//!
//! `step` is `wlr-switcher --cycle`: with a switcher overlay up it moves its highlight
//! and answers `ok` at once, and with none it is a `switch`. It is how a compositor
//! that keeps its own bindings while the overlay holds the keyboard — one without
//! `keyboard-shortcuts-inhibit` — still cycles: the binding runs again, and reaches
//! the overlay through here.

use crate::shell::{Host, tlog};
use rustix::event::{EventfdFlags, eventfd};
use std::io::{BufRead, BufReader, Read, Write};
use std::os::fd::{AsFd, BorrowedFd, OwnedFd};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Separates the arguments inside a `show` line.
///
/// A window title is the application's to write and may hold anything a shell would
/// quote — spaces, quotes, backslashes — so `--title` arguments cannot be split on
/// whitespace. ASCII unit separator is the one byte a command line will not contain;
/// [`request`] hands the run back to the client rather than encode one that does.
const SEP: char = '\x1f';

/// How long the daemon waits for a client to finish sending its request. A client
/// writes its line and waits for the answer, so this only ever fires on one that
/// connected and went away — which must not take the daemon down with it.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(2);

/// Path of the per-user control socket.
pub fn socket_path() -> PathBuf {
    wlr_capture::paths::runtime_dir().join("wlr-overlayd.sock")
}

/// Which front-end a run belongs to. They share the overlay and differ in what they
/// do with the pick: one focuses the window, the other names it on stdout.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Tool {
    /// `wlr-switcher`: focus the picked window.
    Switch,
    /// `wlr-chooser`: write the picked source on stdout.
    Choose,
}

impl Tool {
    /// The verb that names this front-end on the wire.
    fn verb(self) -> &'static str {
        match self {
            Tool::Switch => "switch",
            Tool::Choose => "choose",
        }
    }
}

/// Which way a `step` moves the switcher's highlight.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Step {
    Next,
    Prev,
}

impl Step {
    /// The word that names this direction on the wire.
    fn word(self) -> &'static str {
        match self {
            Step::Next => "next",
            Step::Prev => "prev",
        }
    }

    /// How far it moves the highlight.
    fn delta(self) -> isize {
        match self {
            Step::Next => 1,
            Step::Prev => -1,
        }
    }
}

/// What a client asked for.
enum Request {
    /// Show the overlay for these arguments, as this front-end.
    Run(Tool, Vec<String>),
    /// Move the switcher overlay's highlight, or show one for these arguments if none
    /// is up.
    Step(Step, Vec<String>),
    /// Is anyone there?
    Ping,
    /// Stop the daemon.
    Quit,
}

impl Request {
    fn parse(line: &str) -> Result<Request, String> {
        let line = line.trim_end_matches(['\r', '\n']);
        let mut fields = line.split(SEP);
        let verb = fields.next().unwrap_or("").trim();
        let args = |fields: std::str::Split<'_, char>| fields.map(str::to_string).collect();
        Ok(match verb {
            "switch" => Request::Run(Tool::Switch, args(fields)),
            "choose" => Request::Run(Tool::Choose, args(fields)),
            "step" => {
                let step = match fields.next().map(str::trim) {
                    Some("next") => Step::Next,
                    Some("prev") => Step::Prev,
                    _ => return Err("step: expected next or prev".into()),
                };
                Request::Step(step, args(fields))
            }
            "ping" => Request::Ping,
            "quit" | "exit" => Request::Quit,
            "" => return Err("empty command".into()),
            other => return Err(format!("unknown command: {other}")),
        })
    }
}

/// What the daemon answers with, and what the client makes of it.
#[derive(Debug, PartialEq)]
pub enum Reply {
    /// The run did what it was asked. The string is what the client must write on
    /// stdout: the chooser's answer, and empty for the switcher, which acts on the
    /// pick instead of naming it.
    Done(String),
    /// The user backed out of the overlay.
    Cancelled,
    /// An overlay was already up — one daemon shows one at a time. The switcher makes
    /// that the no-op a keybinding pressed twice has always been; the chooser shows
    /// its own overlay rather than answer the portal nothing.
    Busy,
    /// The run was refused or failed; the reason is already in the user's language.
    Err(String),
}

impl Reply {
    fn to_line(&self) -> String {
        match self {
            Reply::Done(out) if out.is_empty() => "ok".into(),
            // The payload is one line by construction (the portal's `Window: <id>`,
            // or one JSON object), and is read back as the rest of this one.
            Reply::Done(out) => format!("ok {}", out.replace('\n', " ")),
            Reply::Cancelled => "cancel".into(),
            Reply::Busy => "busy".into(),
            // A reason spanning lines would be read back as several replies.
            Reply::Err(why) => format!("err {}", why.replace('\n', " ")),
        }
    }

    fn parse(line: &str) -> Reply {
        let line = line.trim();
        match line {
            "ok" => Reply::Done(String::new()),
            "cancel" => Reply::Cancelled,
            "busy" => Reply::Busy,
            _ => match line.strip_prefix("ok ") {
                Some(out) => Reply::Done(out.to_string()),
                None => Reply::Err(line.strip_prefix("err ").unwrap_or(line).trim().to_string()),
            },
        }
    }

    /// The exit status a one-shot run would have ended on: 0 for a run that did what
    /// it was asked, 1 for a cancel, 2 for a failure.
    pub fn exit_code(&self) -> i32 {
        match self {
            Reply::Done(_) => 0,
            Reply::Cancelled => 1,
            Reply::Err(_) => 2,
            // Never reached: a busy answer is the caller's to act on, not to exit on.
            Reply::Busy => 0,
        }
    }
}

// --- client side -------------------------------------------------------------

/// Whether these arguments survive the wire.
///
/// An argument holding a separator (or a newline) would come out of the daemon's
/// parser as two. Vanishingly unlikely in a command line, and showing the overlay
/// here answers the same question — slower, but right.
pub fn can_encode(args: &[String]) -> bool {
    !args
        .iter()
        .any(|a| a.contains(SEP) || a.contains('\n') || a.contains('\r'))
}

/// Hand this invocation's arguments to a running daemon and wait for its answer.
///
/// `None` means there is no daemon listening, and the caller should show the overlay
/// itself. Anything else is the daemon's answer to act on.
pub fn request(tool: Tool, args: &[String]) -> Option<Reply> {
    send(tool.verb(), args)
}

/// Move the highlight of the switcher overlay a running daemon shows, or have it show
/// one for these arguments if it shows none.
///
/// `None` means there is no daemon listening. A moved highlight is answered `ok` at
/// once; otherwise the answer is the run's, as [`request`] gives it.
pub fn request_step(step: Step, args: &[String]) -> Option<Reply> {
    send(&format!("step{SEP}{}", step.word()), args)
}

/// Send one request line, the arguments after `head`, and wait for the answer.
fn send(head: &str, args: &[String]) -> Option<Reply> {
    let mut stream = UnixStream::connect(socket_path()).ok()?;
    let mut line = String::from(head);
    for a in args {
        line.push(SEP);
        line.push_str(a);
    }
    line.push('\n');
    stream.write_all(line.as_bytes()).ok()?;
    stream.flush().ok()?;

    // No timeout: the answer comes when the user is done with the overlay, which is
    // as long as they care to take.
    let mut reply = String::new();
    if stream.read_to_string(&mut reply).is_err() || reply.trim().is_empty() {
        return Some(Reply::Err(crate::tr!("daemon-gone")));
    }
    Some(Reply::parse(&reply))
}

/// Ask a running daemon to stop. Errors if none is listening.
pub fn quit() -> anyhow::Result<()> {
    let path = socket_path();
    let mut stream =
        UnixStream::connect(&path).map_err(|_| anyhow::anyhow!("{}", crate::tr!("daemon-none")))?;
    stream.write_all(b"quit\n")?;
    stream.flush()?;
    let mut reply = String::new();
    let _ = stream.read_to_string(&mut reply);
    Ok(())
}

// --- daemon side -------------------------------------------------------------

/// The overlay the daemon shows, as the accept thread and the main thread both see it.
///
/// Raised by the accept thread the moment it hands a run over, and lowered by the main
/// thread once the overlay is down. The accept thread reads it, which is the whole
/// point: a client that arrives while one is up must be told *at once*. Told when the
/// overlay finally closes — whenever the user is done with it — a portal picker has
/// long since given up, and a keybinding pressed twice has left a process hanging
/// around in the meantime. Raised before the hand-over rather than after, a `step`
/// sent while the overlay is still being built finds it and is kept for it.
pub struct Stage {
    state: Mutex<StageState>,
    /// Readable while steps wait for the overlay (see [`Stage::take_steps`]).
    wake: OwnedFd,
}

#[derive(Default)]
struct StageState {
    /// Which front-end's overlay is up, or on its way.
    showing: Option<Tool>,
    /// How far `step` requests have moved the switcher's highlight since the overlay
    /// last took them.
    steps: isize,
}

/// What the accept thread does with a request, given what is on screen.
#[derive(Debug, PartialEq)]
enum Admission {
    /// Hand it to the main thread.
    Forward,
    /// The switcher overlay up took it as a step: answer `ok`.
    Stepped,
    /// Another overlay is up: answer `busy`.
    Busy,
}

impl Stage {
    fn new() -> std::io::Result<Self> {
        Ok(Stage {
            state: Mutex::default(),
            // Non-blocking: the stage drains it whether or not a step is waiting.
            wake: eventfd(0, EventfdFlags::NONBLOCK)?,
        })
    }

    /// Settle `request` against the overlay on screen, raising the stage for a run
    /// that will show one.
    fn admit(&self, request: &Request) -> Admission {
        let mut state = self.state.lock().unwrap();
        let tool = match request {
            Request::Ping | Request::Quit => return Admission::Forward,
            Request::Run(tool, _) => *tool,
            Request::Step(..) => Tool::Switch,
        };
        match (state.showing, request) {
            (Some(Tool::Switch), Request::Step(step, _)) => {
                state.steps += step.delta();
                let _ = rustix::io::write(&self.wake, &1u64.to_ne_bytes());
                Admission::Stepped
            }
            (Some(_), _) => Admission::Busy,
            (None, _) => {
                state.showing = Some(tool);
                Admission::Forward
            }
        }
    }

    /// The overlay is down: what comes next is a new run, and no step left over from
    /// this one may move the next one's highlight.
    fn lower(&self) {
        let mut state = self.state.lock().unwrap();
        state.showing = None;
        state.steps = 0;
        drain(&self.wake);
    }

    /// The descriptor that is readable while steps wait to be taken.
    pub fn wake_fd(&self) -> BorrowedFd<'_> {
        self.wake.as_fd()
    }

    /// Take the steps sent since the last call: how far to move the highlight.
    pub fn take_steps(&self) -> isize {
        let mut state = self.state.lock().unwrap();
        drain(&self.wake);
        std::mem::take(&mut state.steps)
    }
}

/// Reset an eventfd's counter, so the next wait on it sleeps again.
fn drain(fd: &OwnedFd) {
    let mut ticks = [0u8; 8];
    let _ = rustix::io::read(fd, &mut ticks);
}

/// Run the daemon until it is asked to stop (or the compositor goes away).
///
/// `show` is handed the warm host, which front-end asked, and that invocation's
/// arguments; it answers with what the client should make of the run. It is called
/// from this thread, so an overlay is up exactly while it runs.
pub fn run(
    t0: Instant,
    mut show: impl FnMut(&mut Host, Tool, Vec<String>) -> Reply,
) -> anyhow::Result<()> {
    let listener = bind()?;
    let mut host = Host::new()?;
    host.prewarm(t0)?;
    tlog(t0, "daemon ready");

    let outcome = serve(listener, &mut host, &mut show);
    // The socket file outlives the process otherwise, and the next daemon would have
    // to tell a stale one from a live one.
    let _ = std::fs::remove_file(socket_path());
    outcome
}

/// Bind the control socket, refusing to start if another daemon already owns it and
/// clearing a stale socket file otherwise.
fn bind() -> anyhow::Result<UnixListener> {
    let path = socket_path();
    if UnixStream::connect(&path).is_ok() {
        anyhow::bail!("{}", crate::tr!("daemon-already-running"));
    }
    // A leftover file from a crashed daemon: nothing is listening (just checked), so
    // it is in the way rather than in use.
    let _ = std::fs::remove_file(&path);
    UnixListener::bind(&path).map_err(|e| anyhow::anyhow!("cannot bind {}: {e}", path.display()))
}

fn serve(
    listener: UnixListener,
    host: &mut Host,
    show: &mut impl FnMut(&mut Host, Tool, Vec<String>) -> Reply,
) -> anyhow::Result<()> {
    let stage = Arc::new(Stage::new()?);
    host.steer_with(Some(stage.clone()));
    // Accepting has to keep working while the main thread is busy holding an overlay
    // — that is where `busy` answers come from — so it runs on its own thread and
    // hands the runs it cannot answer itself over, waking us on an eventfd. Same
    // shape as wlr-draw's control socket, which also has one loop to protect.
    let (tx, rx) = mpsc::channel::<(Request, UnixStream)>();
    let wake = eventfd(0, EventfdFlags::empty())?;
    let thread_wake = wake.try_clone()?;
    let thread_stage = stage.clone();
    std::thread::spawn(move || accept_loop(listener, &tx, &thread_wake, &thread_stage));

    loop {
        // Idle: nothing of ours is on screen, and the only thing worth waking for is
        // a client — but the compositor keeps talking, so the Wayland connection is
        // drained in the same wait.
        host.idle_until_readable(wake.as_fd())?;
        // The counter, so the next wait sleeps again rather than spinning.
        drain(&wake);

        while let Ok((request, mut stream)) = rx.try_recv() {
            let (tool, args) = match request {
                Request::Ping => {
                    answer(&mut stream, &Reply::Done(String::new()));
                    continue;
                }
                Request::Quit => {
                    answer(&mut stream, &Reply::Done(String::new()));
                    return Ok(());
                }
                Request::Run(tool, args) => (tool, args),
                // With no overlay up, a step is the run that opens one; its arguments
                // say which way the highlight starts.
                Request::Step(_, args) => (Tool::Switch, args),
            };
            // A client killed while its overlay is up leaves no one to answer: the
            // overlay goes with it instead of holding the keyboard.
            host.cancel_on_hangup(stream.try_clone().ok().map(OwnedFd::from));
            let reply = show(host, tool, args);
            host.cancel_on_hangup(None);
            // The accept thread raised the stage before handing this run over, and
            // answered every run since itself; what is still queued is a ping or a
            // quit, served on the next round.
            stage.lower();
            answer(&mut stream, &reply);
        }
    }
}

/// Accept clients and read their requests, answering the ones that need no overlay
/// and handing the rest to the main thread.
fn accept_loop(
    listener: UnixListener,
    tx: &Sender<(Request, UnixStream)>,
    wake: &OwnedFd,
    stage: &Stage,
) {
    for conn in listener.incoming() {
        let Ok(mut stream) = conn else { continue };
        let request = match read_request(&mut stream) {
            Ok(request) => request,
            Err(why) => {
                answer(&mut stream, &Reply::Err(why));
                continue;
            }
        };
        // One daemon shows one overlay at a time. Said now, the client can do
        // something about it: the switcher shrugs — pressing the keybinding twice
        // has always been a no-op — and the chooser shows its own overlay rather
        // than answer the portal nothing. A step is no second overlay but a move of
        // the switcher's highlight, done by the time it is answered.
        match stage.admit(&request) {
            Admission::Busy => answer(&mut stream, &Reply::Busy),
            Admission::Stepped => answer(&mut stream, &Reply::Done(String::new())),
            Admission::Forward => {
                if tx.send((request, stream)).is_err() {
                    return; // the main loop is gone
                }
                let _ = rustix::io::write(wake, &1u64.to_ne_bytes());
            }
        }
    }
}

/// Read one request line, giving up on a client that connects and says nothing.
fn read_request(stream: &mut UnixStream) -> Result<Request, String> {
    let _ = stream.set_read_timeout(Some(REQUEST_TIMEOUT));
    let Ok(read_half) = stream.try_clone() else {
        return Err("cannot read the request".into());
    };
    let mut line = String::new();
    if BufReader::new(read_half).read_line(&mut line).is_err() {
        return Err("cannot read the request".into());
    }
    Request::parse(&line)
}

fn answer(stream: &mut UnixStream, reply: &Reply) {
    let _ = stream.write_all(reply.to_line().as_bytes());
    let _ = stream.write_all(b"\n");
    let _ = stream.flush();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_run_line_survives_whatever_a_filter_holds() {
        // `--title` takes what the application wrote in its title bar: spaces,
        // quotes, anything a shell would have to quote. Splitting on whitespace
        // would tear it in two, so the wire holds one field per argument.
        let args = vec![
            "--layout".to_string(),
            "grid".to_string(),
            "--title".to_string(),
            r#"a b "c" \d"#.to_string(),
        ];
        let line = format!("switch{SEP}{}", args.join(&SEP.to_string()));
        match Request::parse(&line).unwrap() {
            Request::Run(Tool::Switch, got) => assert_eq!(got, args),
            _ => panic!("not a switch run"),
        }
    }

    #[test]
    fn a_bare_verb_is_a_run_with_no_arguments() {
        // The common case, and the one a hand-typed `printf 'switch\n' | socat …`
        // sends. Each front-end has its own verb, and they must not be confused: one
        // focuses a window, the other only names it.
        match Request::parse("switch\n").unwrap() {
            Request::Run(Tool::Switch, args) => assert!(args.is_empty()),
            _ => panic!("not a switch run"),
        }
        match Request::parse("choose").unwrap() {
            Request::Run(Tool::Choose, args) => assert!(args.is_empty()),
            _ => panic!("not a choose run"),
        }
        assert!(matches!(Request::parse("ping").unwrap(), Request::Ping));
        assert!(matches!(Request::parse("quit").unwrap(), Request::Quit));
        assert!(Request::parse("").is_err());
        assert!(Request::parse("frobnicate").is_err());
    }

    #[test]
    fn a_step_line_names_its_direction_before_the_arguments() {
        match Request::parse(&format!("step{SEP}prev{SEP}--hold")).unwrap() {
            Request::Step(Step::Prev, args) => assert_eq!(args, ["--hold"]),
            _ => panic!("not a step back"),
        }
        assert!(matches!(
            Request::parse("step\x1fnext").unwrap(),
            Request::Step(Step::Next, args) if args.is_empty()
        ));
        assert!(Request::parse("step").is_err());
        assert!(Request::parse("step\x1fsideways").is_err());
    }

    #[test]
    fn a_step_moves_the_switcher_up_and_opens_one_otherwise() {
        let stage = Stage::new().unwrap();
        let step = |s| Request::Step(s, Vec::new());
        // Nothing on screen: the step is the run that opens the switcher.
        assert_eq!(stage.admit(&step(Step::Next)), Admission::Forward);
        // From then on, steps move its highlight, kept until the overlay takes them,
        // and a second overlay is refused.
        assert_eq!(stage.admit(&step(Step::Next)), Admission::Stepped);
        assert_eq!(stage.admit(&step(Step::Next)), Admission::Stepped);
        assert_eq!(stage.admit(&step(Step::Prev)), Admission::Stepped);
        assert_eq!(
            stage.admit(&Request::Run(Tool::Switch, Vec::new())),
            Admission::Busy
        );
        assert_eq!(stage.admit(&Request::Ping), Admission::Forward);
        assert_eq!(stage.take_steps(), 1);
        assert_eq!(stage.take_steps(), 0);
        // Steps the overlay never took do not reach the next one.
        assert_eq!(stage.admit(&step(Step::Prev)), Admission::Stepped);
        stage.lower();
        assert_eq!(stage.admit(&step(Step::Prev)), Admission::Forward);
        assert_eq!(stage.take_steps(), 0);
    }

    #[test]
    fn a_step_leaves_the_chooser_alone() {
        let stage = Stage::new().unwrap();
        assert_eq!(
            stage.admit(&Request::Run(Tool::Choose, Vec::new())),
            Admission::Forward
        );
        assert_eq!(
            stage.admit(&Request::Step(Step::Next, Vec::new())),
            Admission::Busy
        );
        assert_eq!(stage.take_steps(), 0);
    }

    #[test]
    fn replies_round_trip_and_name_an_exit_status() {
        for reply in [
            Reply::Done(String::new()),
            // What the chooser answers the portal with, carried back to the client
            // that has to print it.
            Reply::Done("Window: ext-7".into()),
            Reply::Done(r#"{"app-id":"foot","type":"window"}"#.into()),
            Reply::Cancelled,
            Reply::Busy,
            Reply::Err("no such layout".into()),
        ] {
            assert_eq!(Reply::parse(&reply.to_line()), reply);
        }
        assert_eq!(Reply::Done(String::new()).exit_code(), 0);
        assert_eq!(Reply::Cancelled.exit_code(), 1);
        assert_eq!(Reply::Err(String::new()).exit_code(), 2);
    }

    #[test]
    fn a_multi_line_reason_stays_one_reply() {
        // Reasons are read back a line at a time; a wrapped one would leave the rest
        // in the socket for whoever reads next.
        let reply = Reply::Err("first line\nsecond line".into());
        assert_eq!(reply.to_line().lines().count(), 1);
        assert_eq!(
            Reply::parse(&reply.to_line()),
            Reply::Err("first line second line".into())
        );
    }
}
