//! The switcher's daemon and the control socket a `wlr-switcher` invocation reaches
//! it on.
//!
//! An overlay costs about ninety milliseconds to put on screen, and nearly all of it
//! is initialisation the run throws away: the EGL context and its shaders, the font
//! set, the Wayland connection. A daemon pays that once, at startup, and keeps the
//! host ([`crate::shell::Host`]) alive between overlays; each invocation then only
//! builds its surface and paints a frame, which is about ten.
//!
//! Nothing starts a daemon on its own — it is `wlr-switcher --daemon`, from a session
//! autostart. When one is listening, an ordinary invocation hands it the run and
//! reports its answer ([`request`]); when none is, the invocation shows the overlay
//! itself, exactly as it always has.
//!
//! **Capture is not kept warm.** The capture thread is spawned per overlay and dies
//! with it, so an idle daemon reads no window contents: it holds a Wayland connection
//! and a GPU context, nothing else.
//!
//! The protocol is one line of text, like `wlr-draw`'s: `show` followed by the
//! invocation's arguments, `ping`, or `quit`. The daemon answers one line — `ok`,
//! `cancel`, `busy` or `err <reason>` — which the client turns back into its own exit
//! status.

use crate::shell::{Host, tlog};
use std::io::{BufRead, BufReader, Read, Write};
use std::os::fd::AsFd;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
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
    wlr_capture::paths::runtime_dir().join("wlr-switcher.sock")
}

/// What a client asked for.
enum Request {
    /// Show the overlay for these `wlr-switcher` arguments.
    Show(Vec<String>),
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
        Ok(match verb {
            "show" => Request::Show(fields.map(str::to_string).collect()),
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
    /// The run did what it was asked: a window was focused, or there was nothing to
    /// focus but no error either.
    Done,
    /// The user backed out of the overlay.
    Cancelled,
    /// An overlay was already up. Re-pressing the keybinding while the switcher is on
    /// screen is a no-op, not a second overlay.
    Busy,
    /// The run was refused or failed; the reason is already in the user's language.
    Err(String),
}

impl Reply {
    fn to_line(&self) -> String {
        match self {
            Reply::Done => "ok".into(),
            Reply::Cancelled => "cancel".into(),
            Reply::Busy => "busy".into(),
            // A reason spanning lines would be read back as several replies.
            Reply::Err(why) => format!("err {}", why.replace('\n', " ")),
        }
    }

    fn parse(line: &str) -> Reply {
        match line.trim() {
            "ok" => Reply::Done,
            "cancel" => Reply::Cancelled,
            "busy" => Reply::Busy,
            other => Reply::Err(
                other
                    .strip_prefix("err ")
                    .unwrap_or(other)
                    .trim()
                    .to_string(),
            ),
        }
    }

    /// The exit status a one-shot run would have ended on: 0 for a switch (and for a
    /// keybinding pressed twice, which does nothing), 1 for a cancel, 2 for a failure.
    pub fn exit_code(&self) -> i32 {
        match self {
            Reply::Done | Reply::Busy => 0,
            Reply::Cancelled => 1,
            Reply::Err(_) => 2,
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
/// itself. Anything else, including a failure, is the daemon's answer to report as
/// this invocation's own.
pub fn request(args: &[String]) -> Option<Reply> {
    let mut stream = UnixStream::connect(socket_path()).ok()?;
    let mut line = String::from("show");
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

/// Run the daemon until it is asked to stop (or the compositor goes away).
///
/// `show` is handed the warm host and one invocation's arguments, and answers with
/// what the client should exit on. It is called from this thread, so an overlay is up
/// exactly while it runs — nothing else is served meanwhile, by design.
pub fn run(
    t0: Instant,
    mut show: impl FnMut(&mut Host, Vec<String>) -> Reply,
) -> anyhow::Result<()> {
    let listener = bind()?;
    let mut host = Host::new()?;
    host.prewarm(t0)?;
    tlog(t0, "daemon ready");

    let outcome = serve(&listener, &mut host, &mut show);
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
    listener: &UnixListener,
    host: &mut Host,
    show: &mut impl FnMut(&mut Host, Vec<String>) -> Reply,
) -> anyhow::Result<()> {
    loop {
        // Idle: nothing of ours is on screen, and the only thing worth waking for is
        // a client — but the compositor keeps talking, so the Wayland connection is
        // drained in the same wait.
        host.idle_until_readable(listener.as_fd())?;
        let Ok((mut stream, _)) = listener.accept() else {
            continue;
        };
        let request = match read_request(&mut stream) {
            Ok(r) => r,
            Err(why) => {
                answer(&mut stream, &Reply::Err(why));
                continue;
            }
        };
        match request {
            Request::Ping => answer(&mut stream, &Reply::Done),
            Request::Quit => {
                answer(&mut stream, &Reply::Done);
                return Ok(());
            }
            Request::Show(args) => {
                let reply = show(host, args);
                answer(&mut stream, &reply);
                drain_busy(listener);
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

/// Answer everything that piled up while the overlay was on screen with `busy`.
///
/// sway runs its own keybindings over our exclusive keyboard grab, so pressing the
/// bind again while the switcher is up reaches the daemon as a fresh request. For a
/// one-shot run the single-instance lock makes that a no-op; here, everything already
/// in the backlog the moment the overlay closes was such a press, and a second
/// overlay is the last thing it should open.
fn drain_busy(listener: &UnixListener) {
    let _ = listener.set_nonblocking(true);
    while let Ok((mut stream, _)) = listener.accept() {
        let _ = read_request(&mut stream);
        answer(&mut stream, &Reply::Busy);
    }
    let _ = listener.set_nonblocking(false);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_show_line_survives_whatever_a_filter_holds() {
        // `--title` takes what the application wrote in its title bar: spaces,
        // quotes, anything a shell would have to quote. Splitting on whitespace
        // would tear it in two, so the wire holds one field per argument.
        let args = vec![
            "--layout".to_string(),
            "grid".to_string(),
            "--title".to_string(),
            r#"a b "c" \d"#.to_string(),
        ];
        let line = format!("show{SEP}{}", args.join(&SEP.to_string()));
        match Request::parse(&line).unwrap() {
            Request::Show(got) => assert_eq!(got, args),
            _ => panic!("not a show"),
        }
    }

    #[test]
    fn a_bare_show_is_a_run_with_no_arguments() {
        // The common case, and the one a hand-typed `printf 'show\n' | socat …` sends.
        match Request::parse("show\n").unwrap() {
            Request::Show(args) => assert!(args.is_empty()),
            _ => panic!("not a show"),
        }
        assert!(matches!(Request::parse("ping").unwrap(), Request::Ping));
        assert!(matches!(Request::parse("quit").unwrap(), Request::Quit));
        assert!(Request::parse("").is_err());
        assert!(Request::parse("frobnicate").is_err());
    }

    #[test]
    fn replies_round_trip_and_name_an_exit_status() {
        for reply in [
            Reply::Done,
            Reply::Cancelled,
            Reply::Busy,
            Reply::Err("no such layout".into()),
        ] {
            assert_eq!(Reply::parse(&reply.to_line()), reply);
        }
        // A press that lands on an overlay already up did nothing, and did nothing
        // wrong: the keybinding must not report a failure.
        assert_eq!(Reply::Busy.exit_code(), 0);
        assert_eq!(Reply::Done.exit_code(), 0);
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
