//! egui front-end: a grid of live thumbnails. Capture happens on a dedicated
//! thread (it owns the non-`Send` Wayland client) and streams downscaled
//! thumbnails to the UI over a channel, so the window opens instantly and fills
//! in. Toplevel capture is occlusion-independent, so showing our own window
//! first is fine.

use crate::hints::{Hint, HintRow};
use crate::tr;
use std::cmp;
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use wlr_capture::capture::WindowFilter;
use wlr_capture::render::DmabufImporter;
use wlr_capture::theme::Theme;
use wlr_capture::{focus, icons, wl};

/// Shared slot where the chosen source lands; read by `main` after the window closes.
pub type Outcome = Arc<Mutex<Option<Selection>>>;

/// The picked source, carrying the full identity so `main` can act on it per mode:
/// print `token` (portal) or focus it (`wlr-switcher`). Focusing needs both keys: the
/// `identifier` COSMIC addresses a window by, and the `app_id`+`title`+`dup_index`
/// triple zwlr forces on us. See [`Selection::identity`].
#[derive(Clone)]
pub struct Selection {
    pub token: String, // portal stdout contract: "Window: <id>" / "Monitor: <name>"
    pub is_window: bool,
    pub identifier: String, // ext-foreign-toplevel identifier; empty for outputs
    pub app_id: String,     // for zwlr activation / tile labelling
    pub title: String,      // window title
    /// The output's name (screens only; empty for windows) — what addresses a screen
    /// outside the portal's `Monitor: <name>` line.
    pub output: String,
    /// Ordinal among windows sharing this (app_id, title), in creation order, to
    /// disambiguate identical windows when correlating to zwlr handles.
    pub dup_index: usize,
}

impl Selection {
    /// This window in the terms `zwlr-foreign-toplevel-management` exposes.
    pub fn identity(&self) -> wl::WindowIdentity {
        wl::WindowIdentity {
            app_id: self.app_id.clone(),
            title: self.title.clone(),
            dup_index: self.dup_index,
        }
    }
}

pub const APP_ID: &str = "wlr-chooser";
const TILE_W: f32 = 300.0; // reference tile size (aspect ratio for the thumbnail)
const TILE_H: f32 = 180.0;
const MIN_TILE: f32 = 280.0; // tiles grow from here to fill the row width
const GRID_GAP: f32 = 10.0; // gap between tiles
const THUMB_MAX: u32 = 480;

/// Which kinds of source to show. Set by `--windows`/`--outputs`/`--both` and
/// switchable at runtime via the tab bar.
#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum Mode {
    #[default]
    All,
    Windows,
    Outputs,
}

/// How the overlay presents its sources.
#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum View {
    /// Centred rofi-like card with tabs + search (portal picker).
    #[default]
    Card,
    /// macOS-style single horizontal row of tiles (Alt-Tab).
    Strip,
    /// Full-screen mission-control exposé grid.
    Grid,
}

/// Which tiles show a *live* capture in the Alt-Tab strip (vs. just the app
/// icon). Live capture is the project's differentiator; `all` is the default.
#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum Live {
    /// App icons only (lightest; closest to a plain macOS Cmd-Tab).
    None,
    /// Only the highlighted window shows a live preview; others show their icon.
    Current,
    /// Every window shows its live preview (default).
    #[default]
    All,
}

/// How windows are ordered; outputs always come first, by name.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Order {
    /// By app-id, then title.
    ByName,
    /// Most recently focused first, if the compositor reports it; by name otherwise.
    Mru,
}

impl Order {
    /// The window order to apply, and the window that holds the focus right now if
    /// it could be read. Both describe the state the overlay is about to replace, so
    /// this has to run before it takes the keyboard.
    pub(crate) fn resolve(self) -> (WindowOrder, Option<wl::WindowIdentity>) {
        // The active window comes from the foreign-toplevel protocol rather than a
        // compositor IPC, so the opening tile is the same wherever that protocol is;
        // without it the first tile wins. MRU history, below, is the part only some
        // compositors can answer.
        let focused = wl::active_window().ok().flatten();
        let order = match self {
            Self::Mru => WindowOrder::mru(
                focus::detect()
                    .and_then(|b| b.focus_order())
                    .unwrap_or_default()
                    .windows(),
            ),
            Self::ByName => WindowOrder::ByName,
        };
        (order, focused)
    }
}

/// How the capture thread orders windows.
pub(crate) enum WindowOrder {
    ByName,
    /// Each window identifier's rank, most recently focused first.
    Mru(HashMap<String, usize>),
}

impl WindowOrder {
    /// From window identifiers, most recently focused first.
    fn mru<'a>(windows: impl Iterator<Item = &'a str>) -> Self {
        Self::Mru(windows.map(String::from).zip(0..).collect())
    }

    /// Whether a focus history was read but ranks none of the `windows` on screen.
    /// Callers pass a non-empty window list.
    ///
    /// The ranking is keyed by what a compositor IPC calls a window, the tiles by the
    /// `ext-foreign-toplevel-list-v1` identifier; that the two agree is how every
    /// backend correlates them, and no protocol owes it. When they stop agreeing every
    /// pair lands in the unranked arm of [`Self::compare`] and the order quietly
    /// degrades to alphabetical, which is the right fallback but a silent one.
    fn mru_unmatched<'a>(&self, windows: impl IntoIterator<Item = &'a Source>) -> bool {
        let Self::Mru(rank) = self else {
            return false;
        };
        !rank.is_empty() && !windows.into_iter().any(|w| rank.contains_key(&w.key))
    }

    /// Whether window `a` goes before, after or level with `b`.
    fn compare(&self, a: &Source, b: &Source) -> cmp::Ordering {
        let by_name = |s: &Source| (s.app_id.to_lowercase(), s.win_title.to_lowercase());
        match self {
            WindowOrder::ByName => by_name(a).cmp(&by_name(b)),
            WindowOrder::Mru(rank) => {
                match (rank.get(&a.key), rank.get(&b.key)) {
                    (Some(x), Some(y)) => x.cmp(y),
                    // Windows opened since the snapshot go after every ranked one.
                    (Some(_), None) => cmp::Ordering::Less,
                    (None, Some(_)) => cmp::Ordering::Greater,
                    (None, None) => by_name(a).cmp(&by_name(b)),
                }
            }
        }
    }
}

/// One pickable source, as shown in the grid.
#[derive(Clone)]
pub struct Source {
    pub key: String,   // texture key (window identifier or "out:<name>")
    pub token: String, // what we print on stdout: "Window: …" / "Monitor: …"
    pub title: String,
    pub subtitle: String,
    pub filter: String,
    pub is_window: bool,
    pub is_system: bool, // window with an empty app-id (hidden unless asked)
    /// Raw window identity (windows only), for activation / PiP.
    pub app_id: String,
    pub win_title: String,
    /// Raw output name (screens only), kept apart from the localised `title`.
    pub output: String,
    /// Ordinal among windows sharing this (app_id, title), in creation order.
    pub dup_index: usize,
}

impl Source {
    /// Whether this source is the window `w` denotes. Windows are matched the way
    /// they are activated — (app_id, title, creation-order index) — so only a window
    /// neither foreign-toplevel protocol names matches nothing.
    fn is(&self, w: &wl::WindowIdentity) -> bool {
        self.is_window
            && self.app_id == w.app_id
            && self.win_title == w.title
            && self.dup_index == w.dup_index
    }

    /// The identity to hand back to `main` when this source is picked.
    fn selection(&self) -> Selection {
        Selection {
            token: self.token.clone(),
            is_window: self.is_window,
            identifier: if self.is_window {
                self.key.clone()
            } else {
                String::new()
            },
            app_id: self.app_id.clone(),
            title: self.win_title.clone(),
            output: self.output.clone(),
            dup_index: self.dup_index,
        }
    }
}

/// Messages from the capture thread to the UI.
pub enum Msg {
    Sources(Vec<Source>),
    Thumb {
        key: String,
        w: usize,
        h: usize,
        rgba: Vec<u8>,
    },
    Icon {
        key: String,
        w: usize,
        h: usize,
        rgba: Vec<u8>,
    },
    /// A GPU dma-buf frame to import zero-copy as a GL texture (host-side).
    Dmabuf {
        key: String,
        frame: wl::DmabufFrame,
    },
    /// A source disappeared (window closed): drop its cached textures.
    Drop {
        key: String,
    },
}

/// Per-round time budget = upper bound on the refresh rate (capture is
/// damage-driven, so this is a ceiling, not a forced rate). shm pays a full CPU
/// readback+convert+downscale+upload per frame, so we keep it modest (~6 fps);
/// the GPU dma-buf path is near-free per frame, so it runs much faster (~30 fps).
#[cfg(not(feature = "gpu"))]
const ROUND_SHM: Duration = Duration::from_millis(160);
#[cfg(feature = "gpu")]
const ROUND_GPU: Duration = Duration::from_millis(33);

/// The round budget for this run: faster when built with the near-free GPU path.
fn round_budget() -> Duration {
    #[cfg(feature = "gpu")]
    {
        ROUND_GPU
    }
    #[cfg(not(feature = "gpu"))]
    {
        ROUND_SHM
    }
}

/// A window as the command-line filter sees it.
#[derive(Clone, Copy)]
pub struct Candidate<'a> {
    /// The window's application id.
    pub app_id: &'a str,
    /// The window title.
    pub title: &'a str,
    /// The `ext-foreign-toplevel-list-v1` identifier, which is how a compositor names
    /// the process behind a window.
    pub identifier: &'a str,
}

impl<'a> From<&'a wl::Toplevel> for Candidate<'a> {
    fn from(w: &'a wl::Toplevel) -> Self {
        Self {
            app_id: &w.app_id,
            title: &w.title,
            identifier: &w.identifier,
        }
    }
}

/// Which windows a run offers at all, as narrowed on the command line. Distinct from
/// the overlay's own filter field, which only hides tiles that are already captured:
/// this one is applied before a capture session is opened, so an excluded window costs
/// nothing.
///
/// Empty admits every window. A criterion given several times admits a window matching
/// any of its values; criteria of different kinds must all be satisfied. A new kind of
/// criterion is a new field here and one more clause in [`WindowFilters::admits`].
#[derive(Clone, Default)]
pub struct WindowFilters {
    /// Application ids, compared exactly and case-insensitively.
    app_ids: Vec<String>,
    /// Title substrings, compared case-insensitively.
    titles: Vec<String>,
    /// Process ids, compared exactly. A process usually owns several windows and every
    /// one of them passes.
    pids: Vec<u32>,
    /// What the compositor last said about the windows on screen: each one's process by
    /// identifier, `None` for a window it covered but named no process for. Absent
    /// means "never asked about", which is the only thing [`Self::refresh_pids`] acts
    /// on — a window the compositor cannot name is not asked about twice.
    processes: HashMap<String, Option<u32>>,
}

impl WindowFilters {
    /// The filters a command line asked for, before any window is known.
    pub fn new(app_ids: Vec<String>, titles: Vec<String>, pids: Vec<u32>) -> Self {
        Self {
            app_ids,
            titles,
            pids,
            processes: HashMap::new(),
        }
    }

    /// Whether nothing was asked for, in which case every window is offered.
    pub fn is_empty(&self) -> bool {
        self.app_ids.is_empty() && self.titles.is_empty() && self.pids.is_empty()
    }

    /// Whether this run has to know the process behind each window.
    pub fn needs_pids(&self) -> bool {
        !self.pids.is_empty()
    }

    /// Learn the process behind the windows in `toplevels` that have not been asked
    /// about yet, from the compositor's IPC. `false` when nothing could answer — no
    /// focus backend, or one whose compositor names no process.
    ///
    /// Without a `--pid` filter this neither connects nor queries: the flags that do
    /// not need a pid do not pay for one. With one, it queries only on the round where
    /// a window it has never seen shows up, so a running overlay costs one query, not
    /// one per frame.
    pub fn refresh_pids(&mut self, toplevels: &[wl::Toplevel]) -> bool {
        if !self.needs_pids()
            || toplevels
                .iter()
                .all(|w| self.processes.contains_key(&w.identifier))
        {
            return true;
        }
        let Some(pids) = focus::detect().and_then(|b| b.window_pids()) else {
            return false;
        };
        for w in toplevels {
            self.processes
                .insert(w.identifier.clone(), pids.get(&w.identifier).copied());
        }
        true
    }

    /// Whether this window passes. The app id and title are handed to
    /// [`WindowFilter::matches`], the same comparison `wlr-shot --app-id` / `--title`
    /// uses to name a single window, so the flags mean one thing across the suite.
    ///
    /// A window whose process is unknown fails a `--pid` filter: a filter admits what
    /// it has matched, never what it could not check.
    pub fn admits(&self, w: Candidate<'_>) -> bool {
        let app_ok = self.app_ids.is_empty()
            || self.app_ids.iter().any(|a| {
                WindowFilter {
                    app_id: Some(a),
                    title: None,
                }
                .matches(w.app_id, w.title)
            });
        let title_ok = self.titles.is_empty()
            || self.titles.iter().any(|t| {
                WindowFilter {
                    app_id: None,
                    title: Some(t),
                }
                .matches(w.app_id, w.title)
            });
        let pid_ok = self.pids.is_empty()
            || self
                .processes
                .get(w.identifier)
                .copied()
                .flatten()
                .is_some_and(|pid| self.pids.contains(&pid));
        app_ok && title_ok && pid_ok
    }

    /// The filter written back as the flags that produced it, to quote in the message
    /// shown when it matches nothing.
    pub fn describe(&self) -> String {
        self.app_ids
            .iter()
            .map(|a| format!("--app-id {a}"))
            .chain(self.titles.iter().map(|t| format!("--title {t}")))
            .chain(self.pids.iter().map(|p| format!("--pid {p}")))
            .collect::<Vec<_>>()
            .join(" ")
    }
}

/// The windows a run offers, as positions in `windows` paired with each one's ordinal
/// among windows sharing its (app-id, title).
///
/// The ordinal counts the windows the filter drops as well: it is how a window is named
/// for activation (zwlr enumerates them in creation order), so narrowing the list must
/// not renumber what is left of it.
pub(crate) fn admitted_windows<'a>(
    windows: impl IntoIterator<Item = Candidate<'a>>,
    filters: &WindowFilters,
) -> Vec<(usize, usize)> {
    let mut dup: HashMap<(&str, &str), usize> = HashMap::new();
    let mut kept = Vec::new();
    for (i, w) in windows.into_iter().enumerate() {
        let e = dup.entry((w.app_id, w.title)).or_insert(0);
        let dup_index = *e;
        *e += 1;
        if filters.admits(w) {
            kept.push((i, dup_index));
        }
    }
    kept
}

/// A source paired with what it takes to (re)open its capture session.
enum Capturable {
    Output(wl::Output),
    Window(wl::Toplevel),
}

/// Capture thread body: a continuous loop. Each round it refreshes the window
/// list, opens persistent sessions for new sources, captures one frame from every
/// session, and streams thumbnails — so tiles show *live* content. Sessions are
/// reused across rounds (the buffer is not reallocated unless a window resizes).
///
/// Toplevels with an empty app-id are captured but marked `is_system`, so the UI
/// can hide them by default and reveal them on demand. The loop exits when the UI
/// drops the channel.
///
/// `filters` narrows the window list here, before any session is opened, so a window
/// left out costs neither a capture nor a thumbnail.
pub(crate) fn capture_thread(
    tx: Sender<Msg>,
    gpu_failed: Arc<AtomicBool>,
    order: WindowOrder,
    mut filters: WindowFilters,
) {
    let mut client = match wl::Client::connect() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{}", tr!("error", error = format!("{e:#}")));
            return;
        }
    };

    // Per-source bookkeeping that must persist across rounds.
    let mut sessions: HashMap<String, wl::SessionId> = HashMap::new(); // source key -> session
    let mut by_id: HashMap<wl::SessionId, String> = HashMap::new(); // reverse, to label frames
    let mut iconed: HashSet<String> = HashSet::new();
    let mut last_keys: Vec<String> = Vec::new();
    // Whether the focus history has been confronted with a window list yet.
    let mut mru_checked = false;
    let budget = round_budget();

    'outer: loop {
        // Pick up newly-opened / closed windows since the last round.
        if client.refresh().is_err() {
            break;
        }

        // Build the current source set in a stable, predictable order:
        // outputs first (sorted by name), then windows in `order`.
        let mut outputs = client.outputs().to_vec();
        outputs.sort_by(|a, b| a.name.cmp(&b.name));

        let mut current: Vec<(Source, Capturable)> = Vec::new();
        for o in &outputs {
            current.push((output_source(o), Capturable::Output(o.clone())));
        }
        // Windows that share an (app_id, title) are numbered in creation order, matching
        // zwlr's enumeration for activation — so before sorting them for display.
        let toplevels = client.toplevels();
        // A window opened since the last answer has no process yet, and a `--pid`
        // filter would drop it. The front-end already established that the compositor
        // answers, so a failure here leaves the map as it is.
        let _ = filters.refresh_pids(toplevels);
        let admitted = admitted_windows(toplevels.iter().map(Candidate::from), &filters);
        for (i, dup_index) in admitted {
            let w = &toplevels[i];
            current.push((window_source(w, dup_index), Capturable::Window(w.clone())));
        }
        current[outputs.len()..].sort_by(|(a, _), (b, _)| order.compare(a, b));
        // The ranking is built before the toplevels are known, so whether its keys are
        // the ones the windows carry can only be seen here. One round with windows on
        // it settles the question — the loop runs several times a second, and the
        // answer is a property of the compositor, not of this round's window set.
        let windows = &current[outputs.len()..];
        if !mru_checked && !windows.is_empty() {
            mru_checked = true;
            if order.mru_unmatched(windows.iter().map(|(s, _)| s)) {
                eprintln!("{}", tr!("mru-unmatched"));
            }
        }
        let keys: Vec<String> = current.iter().map(|(s, _)| s.key.clone()).collect();

        // Announce the source list only when it actually changes (set or order).
        if keys != last_keys {
            let srcs: Vec<Source> = current.iter().map(|(s, _)| s.clone()).collect();
            if tx.send(Msg::Sources(srcs)).is_err() {
                break;
            }
            last_keys = keys.clone();
        }

        // Close sessions for windows that vanished and tell the UI to drop them.
        let present: HashSet<&str> = keys.iter().map(String::as_str).collect();
        let gone: Vec<String> = sessions
            .keys()
            .filter(|k| !present.contains(k.as_str()))
            .cloned()
            .collect();
        for k in gone {
            if let Some(id) = sessions.remove(&k) {
                by_id.remove(&id);
                client.close_session(&id);
            }
            iconed.remove(&k);
            if tx.send(Msg::Drop { key: k }).is_err() {
                break 'outer;
            }
        }

        // Open a session for every source we don't track yet.
        for (s, cap) in &current {
            if sessions.contains_key(&s.key) {
                continue;
            }
            let opened = match cap {
                Capturable::Output(o) => client.open_output_session(o),
                Capturable::Window(w) => client.open_toplevel_session(w),
            };
            if let Ok(id) = opened {
                sessions.insert(s.key.clone(), id);
                by_id.insert(id, s.key.clone());
            }

            // App icon (cheap) once per window, so it's identifiable independently
            // of its thumbnail.
            if let Capturable::Window(w) = cap
                && iconed.insert(s.key.clone())
                && let Some(path) = icons::resolve(&w.app_id)
            {
                // Loaded large so the macOS-style Alt-Tab strip stays crisp;
                // smaller tile/exposé uses just downscale it.
                if let Some((iw, ih, rgba)) = icons::load(&path, 128)
                    && tx
                        .send(Msg::Icon {
                            key: s.key.clone(),
                            w: iw as usize,
                            h: ih as usize,
                            rgba,
                        })
                        .is_err()
                {
                    break 'outer;
                }
            }
        }

        // The UI could not import a dma-buf: capture into shm from now on, so the
        // previews come back instead of merely reporting that they can't. The
        // sessions are closed rather than just rebuffered — capture is incremental
        // by damage, so a fresh buffer on a live session would stay empty until the
        // source repaints, which a static window never does. They reopen below.
        if gpu_failed.swap(false, Ordering::Relaxed) && !client.gpu_disabled() {
            eprintln!("wlr-capture: dma-buf import failed; falling back to shm");
            client.disable_gpu();
            for (_, id) in sessions.drain() {
                client.close_session(&id);
            }
            by_id.clear();
        }

        // Drive all sessions for one round: this blocks up to the round budget
        // waiting for damage, so an idle desktop costs ~one syscall, while updating
        // windows stream frames. Only sources that produced new content come back.
        let (frames, failed) = client.poll(budget);
        for (id, frame) in frames {
            let Some(key) = by_id.get(&id) else { continue };
            let msg = match frame {
                wl::Frame::Shm(img) => {
                    let (w, h, rgba) = thumbnail(img);
                    Msg::Thumb {
                        key: key.clone(),
                        w,
                        h,
                        rgba,
                    }
                }
                wl::Frame::Dmabuf(frame) => Msg::Dmabuf {
                    key: key.clone(),
                    frame,
                },
            };
            if tx.send(msg).is_err() {
                break 'outer;
            }
        }
        // Sessions the compositor stopped: drop them; if the window is still
        // listed we reopen it next round.
        for id in failed {
            if let Some(key) = by_id.remove(&id) {
                sessions.remove(&key);
            }
            client.close_session(&id);
        }
    }
}

/// Cheap content fingerprint of a frame (subsampled FNV-1a), to tell whether a
/// capture actually changed between rounds — used by the headless bench.
fn quick_hash(rgba: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    // Sample ~4096 bytes spread across the frame so large captures stay cheap.
    let step = (rgba.len() / 4096).max(1);
    let mut i = 0;
    while i < rgba.len() {
        h = (h ^ rgba[i] as u64).wrapping_mul(0x100000001b3);
        i += step;
    }
    (h ^ rgba.len() as u64).wrapping_mul(0x100000001b3)
}

/// Headless capture benchmark (debug): no overlay, no keyboard grab. Runs the
/// capture loop for `secs` seconds and reports, per source, how many frames were
/// captured and how many actually changed content (proof of "live").
///
/// `filters` narrows the window list the same way the picker does, so the benchmark
/// measures what a filtered run actually costs.
pub fn bench_capture(secs: u64, mut filters: WindowFilters) {
    let mut client = match wl::Client::connect() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("bench: connection failed: {e:#}");
            return;
        }
    };
    let mut sessions: HashMap<String, wl::SessionId> = HashMap::new();
    let mut by_id: HashMap<wl::SessionId, String> = HashMap::new();
    // key -> (frames, changed, last_hash)
    let mut stats: HashMap<String, (u32, u32, u64)> = HashMap::new();

    let _ = client.refresh();
    if !filters.refresh_pids(client.toplevels()) {
        eprintln!("{}", tr!("pid-unsupported"));
        return;
    }
    let admitted = |c: &wl::Client, f: &WindowFilters| -> Vec<wl::Toplevel> {
        c.toplevels()
            .iter()
            .filter(|w| f.admits(Candidate::from(*w)))
            .cloned()
            .collect()
    };
    eprintln!(
        "bench: {} output(s), {} window(s); capturing for {secs}s…",
        client.outputs().len(),
        admitted(&client, &filters).len()
    );

    let deadline = Instant::now() + Duration::from_secs(secs);
    let mut rounds = 0u32;
    while Instant::now() < deadline {
        let _ = client.refresh();
        let _ = filters.refresh_pids(client.toplevels());
        let mut outputs = client.outputs().to_vec();
        outputs.sort_by(|a, b| a.name.cmp(&b.name));
        let windows = admitted(&client, &filters);

        let mut items: Vec<(String, Capturable)> = Vec::new();
        for o in &outputs {
            items.push((format!("out:{}", o.name), Capturable::Output(o.clone())));
        }
        for w in &windows {
            items.push((w.identifier.clone(), Capturable::Window(w.clone())));
        }

        for (key, cap) in &items {
            if sessions.contains_key(key) {
                continue;
            }
            let opened = match cap {
                Capturable::Output(o) => client.open_output_session(o),
                Capturable::Window(w) => client.open_toplevel_session(w),
            };
            match opened {
                Ok(id) => {
                    sessions.insert(key.clone(), id);
                    by_id.insert(id, key.clone());
                }
                Err(e) => eprintln!("bench: open {key}: {e:#}"),
            }
        }

        let (frames, failed) = client.poll(round_budget());
        for (id, frame) in frames {
            if let Some(key) = by_id.get(&id) {
                // shm carries pixels (hashable); dma-buf is GPU-only here, so we
                // just count it as a delivered frame (content lives on the GPU).
                let hash = match &frame {
                    wl::Frame::Shm(img) => quick_hash(&img.rgba),
                    wl::Frame::Dmabuf(_) => 0,
                };
                let e = stats.entry(key.clone()).or_insert((0, 0, hash));
                e.0 += 1;
                if e.0 > 1 && hash != 0 && e.2 != hash {
                    e.1 += 1;
                }
                e.2 = hash;
            }
        }
        for id in failed {
            if let Some(key) = by_id.remove(&id) {
                eprintln!("bench: session stopped {key}");
                sessions.remove(&key);
            }
            client.close_session(&id);
        }
        rounds += 1;
    }

    eprintln!("bench: {rounds} round(s) in {secs}s");
    let mut keys: Vec<_> = stats.keys().cloned().collect();
    keys.sort();
    for k in keys {
        let (frames, changed, _) = stats[&k];
        eprintln!("  {k}: {frames} frames, {changed} changed");
    }
}

/// Build the grid entry for an output.
fn output_source(o: &wl::Output) -> Source {
    let title = tr!("screen-label", name = o.name.clone());
    Source {
        key: format!("out:{}", o.name),
        token: format!("Monitor: {}", o.name),
        filter: format!("{} {}", title, o.name).to_lowercase(),
        title,
        subtitle: String::new(),
        is_window: false,
        is_system: false,
        app_id: String::new(),
        win_title: String::new(),
        output: o.name.clone(),
        dup_index: 0,
    }
}

/// Build the grid entry for a window. `dup_index` is its ordinal among windows
/// with the same (app_id, title), used to disambiguate identical windows.
fn window_source(w: &wl::Toplevel, dup_index: usize) -> Source {
    let is_system = w.app_id.is_empty();
    let (title, subtitle) = if is_system {
        (w.title.clone(), String::new())
    } else {
        (w.app_id.clone(), w.title.clone())
    };
    Source {
        key: w.identifier.clone(),
        token: format!("Window: {}", w.identifier),
        filter: format!("{} {}", w.app_id, w.title).to_lowercase(),
        title,
        subtitle,
        is_window: true,
        is_system,
        app_id: w.app_id.clone(),
        win_title: w.title.clone(),
        output: String::new(),
        dup_index,
    }
}

/// Downscale a capture to a thumbnail (max side `THUMB_MAX`), never upscaling.
fn thumbnail(img: wl::CapturedImage) -> (usize, usize, Vec<u8>) {
    let (w, h) = (img.width, img.height);
    let scale = (THUMB_MAX as f32 / w as f32)
        .min(THUMB_MAX as f32 / h as f32)
        .min(1.0);
    let src = match image::RgbaImage::from_raw(w, h, img.rgba) {
        Some(s) => s,
        None => return (0, 0, Vec::new()),
    };
    if scale >= 0.999 {
        return (w as usize, h as usize, src.into_raw());
    }
    let nw = ((w as f32 * scale) as u32).max(1);
    let nh = ((h as f32 * scale) as u32).max(1);
    let small = image::imageops::thumbnail(&src, nw, nh);
    (
        small.width() as usize,
        small.height() as usize,
        small.into_raw(),
    )
}

/// How the picker presents and behaves, as resolved from the CLI.
pub struct Options {
    pub mode: Mode,
    pub show_system: bool,
    /// Fixed grid size (columns, rows), or `None` for an auto-fitting grid.
    pub grid: Option<(u32, u32)>,
    /// How sources are presented (card / strip / grid).
    pub view: View,
    /// Hold-to-switch: confirm and close when the held launch modifier (Alt/Super)
    /// is released. Default on for the switcher, off for the portal picker.
    pub hold: bool,
    /// Which Alt-Tab tiles show a live preview (vs. just the icon).
    pub live: Live,
    pub order: Order,
    /// Which windows the run offers at all; applied before capture.
    pub window_filters: WindowFilters,
    /// Label the tiles with a key that picks them, taken from this physical row, or
    /// `None` to leave them bare. Only the views that own the whole keyboard honour
    /// it (see [`App::hints_apply`]).
    pub hints: Option<HintRow>,
}

/// How long the tiles stay hidden in hold-to-switch mode if keyboard focus
/// never arrives (e.g. no keyboard on the seat, another exclusive layer
/// surface outranking us).
///
/// Safety net to avoid leaving a screen-sized fully transparent overlay which,
/// still swallowing every click, could render the desktop unusable.
const REVEAL_BACKSTOP_SECS: f64 = 0.15;

pub struct App {
    rx: Receiver<Msg>,
    /// The latest source list from the capture thread; `None` until the first arrives.
    sources: Option<Vec<Source>>,
    textures: HashMap<String, egui::TextureHandle>,
    /// GPU dma-buf thumbnails: egui texture id + source pixel size, imported by
    /// the host. Looked up before `textures` when drawing a tile.
    native: HashMap<String, (egui::TextureId, egui::Vec2)>,
    /// Sources whose dma-buf frames the GPU refused to import. Failure is
    /// deterministic, so these will never get a preview: drawing them as
    /// "loading" would be a lie that never resolves.
    failed: HashSet<String>,
    /// Raised when an import fails, so the capture thread drops to shm.
    gpu_failed: Arc<AtomicBool>,
    icons: HashMap<String, egui::TextureHandle>,
    filter: String,
    mode: Mode,
    show_system: bool,
    /// Fixed grid size (columns, rows), or `None` for an auto-fitting grid.
    grid: Option<(u32, u32)>,
    /// How sources are presented (card / strip / grid).
    view: View,
    /// Time (egui seconds) of the first exposé frame, to anchor the intro animation.
    expose_t0: Option<f32>,
    /// Selected index into the *visible* list, for keyboard navigation.
    selected: usize,
    /// Hold-to-switch: release of the launch modifier confirms (host-driven).
    hold: bool,
    /// Which Alt-Tab tiles show a live preview (vs. just the icon).
    live: Live,
    /// Set once the host arms hold-to-switch; enables Tab-cycle and
    /// confirm-on-Alt-release.
    armed: bool,
    /// Once armed with sources present, place the initial selection so that
    /// releasing Alt immediately switches — like a real Alt-Tab where the launching
    /// Tab already advanced once (see [`App::apply_initial_select`]).
    pending_initial_select: bool,
    /// The window that had the focus at launch, if it could be read: the one the
    /// initial selection steps away from.
    focused: Option<wl::WindowIdentity>,
    /// A confirm that landed before the source list did — a release before the first
    /// frame — carried out as soon as [`App::pump`] delivers the sources.
    pending_confirm: bool,
    /// Whether the tiles are drawn yet; held back under hold-to-switch until the tap
    /// question is settled (see [`App::reveal`]).
    revealed: bool,
    /// First frame on egui's clock.
    first_frame_at_secs: Option<f64>,
    /// Focus the filter field on the first frame.
    focus_filter: bool,
    /// Set once a choice is made or the picker is cancelled; the host loop exits.
    closing: bool,
    out: Outcome,
    theme: Theme,
    /// Which physical row the tile hints come from, or `None` when they are off.
    hint_row: Option<HintRow>,
    /// The hints handed to the tiles, in tile order; empty until the host delivers a
    /// keymap (see [`App::set_keymap`]).
    hints: Vec<Hint>,
}

impl App {
    pub fn new(
        rx: Receiver<Msg>,
        out: Outcome,
        opts: Options,
        focused: Option<wl::WindowIdentity>,
        theme: Theme,
        gpu_failed: Arc<AtomicBool>,
    ) -> Self {
        Self {
            rx,
            sources: None,
            textures: HashMap::new(),
            native: HashMap::new(),
            failed: HashSet::new(),
            gpu_failed,
            icons: HashMap::new(),
            filter: String::new(),
            mode: opts.mode,
            show_system: opts.show_system,
            grid: opts.grid,
            view: opts.view,
            expose_t0: None,
            selected: 0,
            hold: opts.hold,
            live: opts.live,
            armed: false,
            pending_initial_select: false,
            focused,
            pending_confirm: false,
            // Without hold-to-switch there is no tap to mistake the first frames for.
            revealed: !opts.hold,
            first_frame_at_secs: None,
            focus_filter: true,
            closing: false,
            out,
            theme,
            hint_row: opts.hints,
            hints: Vec::new(),
        }
    }

    /// True once a selection or cancellation happened; the host loop should exit.
    pub fn closing(&self) -> bool {
        self.closing
    }

    /// Cancel without a selection (e.g. the compositor closed the surface).
    pub fn cancel(&mut self) {
        self.closing = true;
    }

    /// Whether hold-to-switch is on; the host uses this to decide whether to watch
    /// the launch modifier (Alt/Super) and confirm on its release.
    pub fn hold(&self) -> bool {
        self.hold
    }

    /// Whether this run labels its tiles with a key that picks them.
    ///
    /// Only the views that own the whole keyboard do. The card has a filter field the
    /// user types into, where a letter is a letter and nothing else; a hint there would
    /// either eat the keystroke or never fire, and both break the filter.
    fn hints_apply(&self) -> bool {
        self.hint_row.is_some() && matches!(self.view, View::Strip | View::Grid)
    }

    /// Take the compositor's keymap (xkb text format) and work out this layout's hints.
    ///
    /// The host calls this whenever the keymap arrives or changes, so the labels always
    /// name the keys of the layout in force — and never the ones of the layout the
    /// alphabet happened to be written for.
    pub fn set_keymap(&mut self, keymap: &str) {
        let Some(row) = self.hint_row.filter(|_| self.hints_apply()) else {
            return;
        };
        self.hints = crate::hints::hints(keymap, row);
    }

    /// Pick the tile the physical key `code` (an evdev code) labels, and say whether
    /// it did — the host then keeps the keystroke to itself rather than passing it on.
    ///
    /// The match is on the position, not on the character: the label was read off that
    /// same position, so the key that shows and the key that fires are the same one
    /// whatever the layout, and whatever modifier is held (Alt-Tab holds one).
    /// Picking is immediate, like a click on the tile.
    pub fn press_hint(&mut self, code: u32) -> bool {
        if !self.hints_apply() || self.closing {
            return false;
        }
        let Some(i) = self.hints.iter().position(|h| h.code == code) else {
            return false;
        };
        // More tiles than hints leaves the tail bare, and a bare tile's key is not ours
        // to swallow.
        let Some(sel) = self.visible().get(i).map(|s| s.selection()) else {
            return false;
        };
        self.selected = i;
        self.choose(sel);
        true
    }

    /// The character labelling the tile at `index`, if it has one.
    fn hint_label(&self, index: usize) -> Option<&str> {
        self.hints_apply()
            .then(|| self.hints.get(index))
            .flatten()
            .map(|h| h.label.as_str())
    }

    /// Arm hold-to-switch: enable Tab-cycle and confirm-on-release, and arm the
    /// initial selection (see [`App::apply_initial_select`]).
    pub fn arm(&mut self) {
        if !self.armed {
            self.armed = true;
            self.pending_initial_select = true;
        }
    }

    /// Draw the tiles from now on.
    ///
    /// Under hold-to-switch the first frames are blank on purpose. The surface has to be
    /// mapped — and so a frame committed — before the compositor hands over keyboard
    /// focus, and only with focus can we tell a held modifier from one released before
    /// we were listening. Until then the overlay is present but shows nothing, so a
    /// quick tap can resolve without flickering.
    pub fn reveal(&mut self) {
        self.revealed = true;
    }

    fn apply_reveal_backstop(&mut self, ctx: &egui::Context) {
        if !self.revealed {
            let now = ctx.input(|i| i.time);
            match self.first_frame_at_secs {
                None => {
                    self.first_frame_at_secs = Some(now);
                }
                Some(first) if now - first >= REVEAL_BACKSTOP_SECS => self.reveal(),
                _ => {}
            }
        }
    }

    /// Place the initial selection, once sources exist: on the first visible window
    /// that is *not* the one the user is on, whatever the order. A switcher exists to
    /// leave the current window, so releasing the modifier straight away must land
    /// somewhere else — the way a real Alt-Tab's launching Tab has already moved on.
    ///
    /// The current window being unknown (no focus, or an identity the tiles don't
    /// carry) or absent from the list falls back to the first tile, which is then not
    /// the one the user is on either.
    fn apply_initial_select(&mut self) {
        if self.pending_initial_select && self.sources.is_some() {
            self.pending_initial_select = false;
            self.selected = match &self.focused {
                Some(f) => self
                    .visible()
                    .iter()
                    .position(|s| !s.is(f))
                    .unwrap_or_default(),
                None => 0,
            };
        }
    }

    /// Advance (or retreat) the highlighted source — Tab / Shift+Tab.
    pub fn cycle(&mut self, forward: bool) {
        let n = self.visible().len();
        if n == 0 {
            return; // nothing to cycle yet; keep the pending initial jump
        }
        // The initial placement (if still pending) represents the launching chord;
        // a real Tab press supersedes it.
        self.pending_initial_select = false;
        self.selected = if forward {
            (self.selected + 1) % n
        } else {
            (self.selected + n - 1) % n
        };
    }

    /// The held launch modifier was released: confirm the highlighted source and
    /// quit. No-op if a choice was already made or the picker is closing.
    pub fn confirm_release(&mut self) {
        if self.closing {
            return;
        }
        // A quick release gets here before the first frame, so the sources may not be in
        // yet.
        if self.sources.is_none() {
            self.pending_confirm = true;
            return;
        }
        // Arming may have happened since the last frame, leaving the jump still pending.
        self.apply_initial_select();
        if let Some(sel) = self.visible().get(self.selected).map(|s| s.selection()) {
            self.choose(sel);
        } else {
            self.closing = true;
        }
    }

    /// Apply the pending confirm, once sources exist.
    fn apply_confirm(&mut self) {
        if self.pending_confirm && self.sources.is_some() {
            self.pending_confirm = false;
            self.confirm_release();
        }
    }

    /// Install the palette into an egui context (host loops own the context).
    pub fn apply_theme(&self, ctx: &egui::Context) {
        self.theme.apply(ctx);
    }

    fn choose(&mut self, sel: Selection) {
        *self.out.lock().unwrap() = Some(sel);
        self.closing = true;
    }

    fn pump(&mut self, ctx: &egui::Context, importer: &mut dyn DmabufImporter) {
        while let Ok(msg) = self.rx.try_recv() {
            match msg {
                Msg::Sources(s) => self.sources = Some(s),
                Msg::Thumb { key, w, h, rgba } if w > 0 && h > 0 => {
                    let img = egui::ColorImage::from_rgba_unmultiplied([w, h], &rgba);
                    // Update the existing texture in place when we can (live frames
                    // stream in continuously), else allocate it the first time.
                    match self.textures.get_mut(&key) {
                        Some(tex) => tex.set(img, egui::TextureOptions::LINEAR),
                        None => {
                            let tex = ctx.load_texture(&key, img, egui::TextureOptions::LINEAR);
                            self.textures.insert(key, tex);
                        }
                    }
                }
                Msg::Dmabuf { key, frame } => {
                    // Imported by the host (it owns the GL context); the resulting
                    // texture samples the dma-buf directly (zero copy).
                    match importer.import(&key, frame) {
                        Some(tex) => {
                            self.native.insert(key.clone(), tex);
                            self.failed.remove(&key);
                        }
                        None => {
                            self.failed.insert(key);
                            // Ask the capture thread to switch this session to shm.
                            self.gpu_failed.store(true, Ordering::Relaxed);
                        }
                    }
                }
                Msg::Icon { key, w, h, rgba } if w > 0 && h > 0 => {
                    let img = egui::ColorImage::from_rgba_unmultiplied([w, h], &rgba);
                    let tex =
                        ctx.load_texture(format!("icon:{key}"), img, egui::TextureOptions::LINEAR);
                    self.icons.insert(key, tex);
                }
                Msg::Drop { key } => {
                    self.textures.remove(&key);
                    self.native.remove(&key);
                    self.failed.remove(&key);
                    self.icons.remove(&key);
                    importer.forget(&key);
                }
                Msg::Thumb { .. } | Msg::Icon { .. } => {}
            }
        }
    }

    /// The drawable thumbnail for a source: the GPU dma-buf texture if present,
    /// else the shm texture. Returns the egui texture id and source pixel size.
    fn thumb_tex(&self, key: &str) -> Option<(egui::TextureId, egui::Vec2)> {
        if let Some(&(id, size)) = self.native.get(key) {
            return Some((id, size));
        }
        self.textures.get(key).map(|t| (t.id(), t.size_vec2()))
    }

    /// Whether this source's preview will never arrive, so the tile should say so
    /// instead of showing a loading state forever.
    fn preview_failed(&self, key: &str) -> bool {
        self.failed.contains(key)
            && !self.native.contains_key(key)
            && !self.textures.contains_key(key)
    }

    fn visible(&self) -> Vec<&Source> {
        let f = self.filter.to_lowercase();
        self.sources
            .iter()
            .flatten()
            .filter(|s| self.show_system || !s.is_system)
            .filter(|s| match self.mode {
                Mode::All => true,
                Mode::Windows => s.is_window,
                Mode::Outputs => !s.is_window,
            })
            .filter(|s| f.is_empty() || s.filter.contains(&f))
            .collect()
    }

    /// Whether any captured source is a system window (to decide if we show the
    /// "show system windows" toggle).
    fn has_system(&self) -> bool {
        self.sources.iter().flatten().any(|s| s.is_system)
    }
}

impl App {
    /// GL clear colour: the transparent, dimmed backdrop behind the card (rofi-like).
    pub fn backdrop(&self) -> [f32; 4] {
        if !self.revealed {
            return [0.0; 4];
        }
        let mut c = self.theme.backdrop.to_normalized_gamma_f32();
        // Exposé covers the whole screen: dim almost to opaque so the real windows
        // behind are hidden (a client can't move them; this hides them instead).
        if self.view == View::Grid {
            c[3] = c[3].max(0.96);
        }
        c
    }

    /// Build one egui frame. Toolkit-agnostic: the host loop drives it and checks
    /// [`App::closing`] afterwards. The host passes its dma-buf importer (it owns
    /// the GL context) so GPU frames can be turned into drawable textures.
    pub fn run_ui(&mut self, ui: &mut egui::Ui, importer: &mut dyn DmabufImporter) {
        let ctx = ui.ctx().clone();
        self.pump(&ctx, importer);
        ctx.request_repaint(); // keep draining the channel while captures stream in

        self.apply_initial_select();
        self.apply_confirm();
        if self.closing {
            return;
        }

        // Keyboard (read states first; don't call ctx methods inside ctx.input).
        let vis_len = self.visible().len();
        // In the views with no search field (exposé grid, Alt-Tab strip), Tab /
        // Shift+Tab also navigate — even when not armed (e.g. `$mod+Tab` exposé).
        // When armed, the host intercepts Tab before egui, so this never collides.
        let switch_nav = matches!(self.view, View::Strip | View::Grid);
        let (esc, next, prev, enter) = ctx.input(|i| {
            let tab = switch_nav && i.key_pressed(egui::Key::Tab);
            (
                i.key_pressed(egui::Key::Escape),
                i.key_pressed(egui::Key::ArrowRight)
                    || i.key_pressed(egui::Key::ArrowDown)
                    || (tab && !i.modifiers.shift),
                i.key_pressed(egui::Key::ArrowLeft)
                    || i.key_pressed(egui::Key::ArrowUp)
                    || (tab && i.modifiers.shift),
                i.key_pressed(egui::Key::Enter),
            )
        });
        if esc {
            self.closing = true;
        }
        if vis_len > 0 {
            if next {
                self.selected = (self.selected + 1) % vis_len;
            }
            if prev {
                self.selected = (self.selected + vis_len - 1) % vis_len;
            }
        }
        if enter && let Some(sel) = self.visible().get(self.selected).map(|s| s.selection()) {
            self.choose(sel);
        }

        self.apply_reveal_backstop(&ctx);
        if !self.revealed {
            return;
        }

        let chosen = match self.view {
            View::Grid => self.render_expose(ui),
            View::Strip => self.render_switcher(ui),
            View::Card => self.render_card(ui),
        };
        if let Some(sel) = chosen {
            self.choose(sel);
        }
    }

    /// Rofi-like card: a centred panel with mode tabs, a filter field and a
    /// scrolling grid of tiles. Returns the picked source, if any.
    fn render_card(&mut self, ui: &mut egui::Ui) -> Option<Selection> {
        let ctx = ui.ctx().clone();
        let mut chosen: Option<Selection> = None;

        // A centred card on the dimmed overlay backdrop. Its size is either fixed
        // to show exactly `grid` tiles, or a sensible default. Clicking the
        // backdrop cancels, like rofi.
        let screen = ctx.content_rect();
        let forced_cols = self.grid.map(|(c, _)| c as usize);
        let (cw, ch) = match self.grid {
            Some((cols, rows)) => {
                let (cols, rows) = (cols as f32, rows as f32);
                let bar = 14.0; // scrollbar gutter
                let tile_h = MIN_TILE * (TILE_H / TILE_W) + 26.0;
                let inner_w = cols * MIN_TILE + (cols - 1.0) * GRID_GAP + bar;
                let inner_h = 78.0 + rows * tile_h + (rows - 1.0) * GRID_GAP; // 78 = header
                (inner_w + 24.0, inner_h + 24.0) // + card inner margin (12 each side)
            }
            None => (1000.0, 760.0),
        };
        let w = cw.min(screen.width() - 24.0);
        let h = ch.min(screen.height() - 24.0);
        let card_rect = egui::Rect::from_center_size(screen.center(), egui::vec2(w, h));
        let radius = 12.0;

        egui::Window::new("wlr-chooser-card")
            .title_bar(false)
            .resizable(false)
            .fixed_rect(card_rect)
            .frame(
                egui::Frame::new()
                    .fill(self.theme.card)
                    .corner_radius(radius)
                    .inner_margin(12.0),
            )
            .show(&ctx, |ui| {
                ui.horizontal(|ui| {
                    let before = self.mode;
                    ui.selectable_value(&mut self.mode, Mode::All, tr!("tab-all"));
                    ui.selectable_value(&mut self.mode, Mode::Windows, tr!("tab-windows"));
                    ui.selectable_value(&mut self.mode, Mode::Outputs, tr!("tab-outputs"));
                    if self.mode != before {
                        self.selected = 0;
                    }
                    // Reveal system windows (empty app-id) only when some exist.
                    if self.has_system() {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui
                                .checkbox(&mut self.show_system, tr!("show-system"))
                                .changed()
                            {
                                self.selected = 0;
                            }
                        });
                    }
                });
                ui.add_space(6.0);
                let te = egui::TextEdit::singleline(&mut self.filter)
                    .hint_text(tr!("filter-hint"))
                    .desired_width(f32::INFINITY);
                let resp = ui.add(te);
                if resp.changed() {
                    self.selected = 0;
                }
                if self.focus_filter {
                    resp.request_focus(); // type-to-filter immediately
                    self.focus_filter = false;
                }
                ui.add_space(8.0);
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        // Grid: either a forced column count (--grid) or as many as
                        // fit. Tiles fill the row exactly; reserve the scrollbar gutter
                        // so the last column isn't hidden by it.
                        let gap = GRID_GAP;
                        ui.spacing_mut().item_spacing = egui::vec2(gap, gap);
                        let bar =
                            ui.spacing().scroll.bar_width + ui.spacing().scroll.bar_inner_margin;
                        let avail = ui.available_width() - bar;
                        let cols = forced_cols
                            .unwrap_or_else(|| ((avail + gap) / (MIN_TILE + gap)).floor() as usize)
                            .max(1);
                        let tile_w = (avail - gap * (cols as f32 - 1.0)) / cols as f32;
                        let visible = self.visible();
                        let mut idx = 0;
                        for chunk in visible.chunks(cols) {
                            ui.horizontal(|ui| {
                                for s in chunk {
                                    if self.tile(ui, s, idx == self.selected, tile_w) {
                                        chosen = Some(s.selection());
                                    }
                                    idx += 1;
                                }
                            });
                        }
                    });
            });

        // Click on the backdrop cancels, like rofi (works in both modes).
        let bg_click = ctx.input(|i| {
            i.pointer.any_pressed()
                && i.pointer
                    .interact_pos()
                    .is_some_and(|pos| !card_rect.contains(pos))
        });
        if bg_click {
            self.closing = true;
        }
        chosen
    }
}

impl App {
    /// Draw one tile of width `w`; returns true if it was clicked.
    fn tile(&self, ui: &mut egui::Ui, s: &Source, selected: bool, w: f32) -> bool {
        let thumb_h = w * (TILE_H / TILE_W); // keep the 300:180 thumbnail aspect
        let desired = egui::vec2(w, thumb_h + 26.0);
        let (rect, resp) = ui.allocate_exact_size(desired, egui::Sense::click());
        if !ui.is_rect_visible(rect) {
            return resp.clicked();
        }
        let t = &self.theme;
        let p = ui.painter();
        let bg = if selected {
            t.tile_selected
        } else if resp.hovered() {
            t.tile_hover
        } else {
            t.tile
        };
        p.rect_filled(rect, 8.0, bg);

        // Coloured outline distinguishing screens (screen_accent) from windows
        // (window_accent) at a glance.
        let accent = if s.is_window {
            t.window_accent
        } else {
            t.screen_accent
        };
        p.rect_stroke(
            rect,
            8.0,
            egui::Stroke::new(if selected { 3.0 } else { 2.0 }, accent),
            egui::StrokeKind::Inside,
        );

        let pad = 6.0;
        let img_rect = egui::Rect::from_min_size(
            rect.min + egui::vec2(pad, pad),
            egui::vec2(w - 2.0 * pad, thumb_h - 2.0 * pad),
        );
        p.rect_filled(img_rect, 4.0, t.thumb);

        if let Some((tex_id, ts)) = self.thumb_tex(&s.key) {
            // Contain (no crop): fit the texture inside img_rect, centred.
            let scale = (img_rect.width() / ts.x).min(img_rect.height() / ts.y);
            let size = ts * scale;
            let draw = egui::Rect::from_center_size(img_rect.center(), size);
            p.image(
                tex_id,
                draw,
                egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                egui::Color32::WHITE,
            );
        } else {
            let placeholder = if self.preview_failed(&s.key) {
                tr!("preview-unavailable")
            } else if s.is_window {
                tr!("loading")
            } else {
                s.title.clone()
            };
            p.text(
                img_rect.center(),
                egui::Align2::CENTER_CENTER,
                placeholder,
                egui::FontId::proportional(20.0),
                t.text_dim,
            );
        }

        // Label row: a type-distinguishing icon, then the name.
        let icon_sz = 16.0;
        let icon_rect = egui::Rect::from_min_size(
            egui::pos2(rect.min.x + 8.0, rect.max.y - 21.0),
            egui::vec2(icon_sz, icon_sz),
        );
        if !s.is_window {
            draw_monitor_glyph(p, icon_rect, t.screen_accent);
        } else if let Some(ic) = self.icons.get(&s.key) {
            let ts = ic.size_vec2();
            let scale = (icon_rect.width() / ts.x).min(icon_rect.height() / ts.y);
            let draw = egui::Rect::from_center_size(icon_rect.center(), ts * scale);
            p.image(
                ic.id(),
                draw,
                egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                egui::Color32::WHITE,
            );
        } else {
            draw_window_glyph(p, icon_rect, t.window_accent);
        }

        let text_x = icon_rect.max.x + 6.0;
        let label = if s.subtitle.is_empty() {
            s.title.clone()
        } else {
            format!("{} — {}", s.title, s.subtitle)
        };
        let mut job = egui::text::LayoutJob::simple_singleline(
            label,
            egui::FontId::proportional(13.0),
            t.text,
        );
        job.wrap = egui::text::TextWrapping::truncate_at_width(rect.max.x - 6.0 - text_x);
        let galley = ui.painter().layout_job(job);
        p.galley(
            egui::pos2(text_x, rect.max.y - 20.0),
            galley,
            egui::Color32::PLACEHOLDER,
        );

        resp.clicked()
    }
}

impl App {
    /// Exposé: a full-screen, mission-control-style layout over the dimmed
    /// backdrop. Tiles are placed in justified rows (variable heights, each row
    /// filling the width with aspect ratios preserved) and scaled to fit on
    /// screen. Returns the picked source, if any.
    fn render_expose(&mut self, ui: &mut egui::Ui) -> Option<Selection> {
        let ctx = ui.ctx().clone();
        let area = ctx.content_rect().shrink(24.0);
        let gap = 12.0;

        // Intro animation clock: anchor t0 to the first exposé frame so startup
        // latency doesn't eat the animation. (Computed before borrowing sources.)
        let now = ctx.input(|i| i.time) as f32;
        let elapsed = now - *self.expose_t0.get_or_insert(now);
        const ANIM: f32 = 0.28;

        let vis = self.visible();
        let items: Vec<(usize, f32)> = vis
            .iter()
            .enumerate()
            .map(|(i, s)| {
                let aspect = self
                    .thumb_tex(&s.key)
                    .map(|(_, sz)| (sz.x / sz.y).clamp(0.3, 4.0))
                    .unwrap_or(16.0 / 9.0);
                (i, aspect)
            })
            .collect();
        let rects = expose_layout(&items, area, gap);

        let mut chosen = None;
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show(ui, |ui| {
                for (i, rect) in &rects {
                    let s = vis[*i];
                    let resp =
                        ui.interact(*rect, ui.id().with(("expose", *i)), egui::Sense::click());
                    // Per-tile ease-out, slightly delayed by index. Alt-Tab skips
                    // the intro entirely — every millisecond to first usable frame
                    // counts, so tiles appear at full size immediately.
                    let ease = if self.armed {
                        1.0
                    } else {
                        let lt = ((elapsed - *i as f32 * 0.012) / ANIM).clamp(0.0, 1.0);
                        1.0 - (1.0 - lt).powi(3)
                    };
                    let scaled = egui::Rect::from_center_size(
                        rect.center(),
                        rect.size() * (0.86 + 0.14 * ease),
                    );
                    self.paint_expose_tile(ui, s, *i, scaled, resp.hovered(), ease);
                    if resp.clicked() {
                        chosen = Some(s.selection());
                    }
                }
            });

        // A press on empty space (no tile) cancels.
        let pressed_outside = ctx.input(|inp| {
            inp.pointer.any_pressed()
                && inp
                    .pointer
                    .interact_pos()
                    .is_some_and(|pos| !rects.iter().any(|(_, r)| r.contains(pos)))
        });
        if pressed_outside {
            self.closing = true;
        }
        chosen
    }

    /// Paint one exposé tile: the live thumbnail filling `rect`, a translucent
    /// label strip with icon + name, and a selection/hover outline.
    fn paint_expose_tile(
        &self,
        ui: &egui::Ui,
        s: &Source,
        index: usize, // position in the visible list: its hint, and whether it is the highlighted one
        rect: egui::Rect,
        hovered: bool,
        a: f32, // intro-animation opacity (1.0 once settled)
    ) {
        let selected = index == self.selected;
        let t = &self.theme;
        let p = ui.painter();
        let radius = 8.0;
        let fade = |c: egui::Color32| c.gamma_multiply(a);
        let white = egui::Color32::WHITE.gamma_multiply(a);
        p.rect_filled(rect, radius, fade(t.thumb));

        if let Some((tex_id, ts)) = self.thumb_tex(&s.key) {
            let scale = (rect.width() / ts.x).min(rect.height() / ts.y);
            let draw = egui::Rect::from_center_size(rect.center(), ts * scale);
            p.image(
                tex_id,
                draw,
                egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                white,
            );
        } else {
            let placeholder = if self.preview_failed(&s.key) {
                tr!("preview-unavailable")
            } else {
                tr!("loading")
            };
            p.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                placeholder,
                egui::FontId::proportional(18.0),
                fade(t.text_dim),
            );
        }

        // Translucent label strip at the bottom: icon + name.
        let strip_h = 24.0_f32.min(rect.height() * 0.3);
        let strip =
            egui::Rect::from_min_max(egui::pos2(rect.left(), rect.bottom() - strip_h), rect.max);
        p.rect_filled(
            strip,
            0.0,
            egui::Color32::from_black_alpha(160).gamma_multiply(a),
        );
        let icon_sz = (strip_h - 8.0).max(10.0);
        let icon_rect = egui::Rect::from_min_size(
            egui::pos2(strip.left() + 6.0, strip.center().y - icon_sz / 2.0),
            egui::vec2(icon_sz, icon_sz),
        );
        if let Some(ic) = self.icons.get(&s.key) {
            let isz = ic.size_vec2();
            let sc = (icon_rect.width() / isz.x).min(icon_rect.height() / isz.y);
            let d = egui::Rect::from_center_size(icon_rect.center(), isz * sc);
            p.image(
                ic.id(),
                d,
                egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                white,
            );
        } else {
            draw_window_glyph(p, icon_rect, fade(t.window_accent));
        }
        let label = if s.subtitle.is_empty() {
            s.title.clone()
        } else {
            format!("{} — {}", s.title, s.subtitle)
        };
        let tx = icon_rect.max.x + 6.0;
        let mut job = egui::text::LayoutJob::simple_singleline(
            label,
            egui::FontId::proportional(13.0),
            fade(t.text),
        );
        job.wrap = egui::text::TextWrapping::truncate_at_width((strip.right() - 6.0 - tx).max(0.0));
        let galley = ui.painter().layout_job(job);
        p.galley(
            egui::pos2(tx, strip.center().y - galley.size().y / 2.0),
            galley,
            fade(t.text),
        );

        self.paint_hint(ui, index, rect, a);

        let accent = if s.is_window {
            t.window_accent
        } else {
            t.screen_accent
        };
        let (sw, col) = if selected {
            (3.0, accent)
        } else if hovered {
            (2.0, accent)
        } else {
            (1.0, t.thumb)
        };
        p.rect_stroke(
            rect,
            radius,
            egui::Stroke::new(sw, fade(col)),
            egui::StrokeKind::Inside,
        );
    }
}

impl App {
    /// macOS-style Alt-Tab: a single horizontal row of tiles on a centred rounded
    /// panel, the highlighted window's name above it. Each tile shows a live
    /// preview (per `--live`) with an app-icon badge, or just the big app icon.
    /// Returns the picked source, if any. Used for `--alt-tab` (compact); the
    /// full-screen exposé is a separate path.
    fn render_switcher(&mut self, ui: &mut egui::Ui) -> Option<Selection> {
        let ctx = ui.ctx().clone();
        let vis = self.visible();
        let n = vis.len();
        let screen = ctx.content_rect();
        if n == 0 {
            return None;
        }
        let sel = self.selected.min(n - 1);

        // Geometry: shrink the icon until the single row fits the screen width.
        let gap = 14.0;
        let pad = 12.0; // inside each cell, around the icon
        let margin = 22.0; // panel padding
        let label_h = 30.0;
        let max_panel_w = screen.width() * 0.92;
        let cell = |ic: f32| ic + 2.0 * pad;
        let needed = |ic: f32| n as f32 * cell(ic) + (n as f32 - 1.0) * gap + 2.0 * margin;
        let mut icon = 96.0_f32;
        while icon > 44.0 && needed(icon) > max_panel_w {
            icon -= 4.0;
        }
        let cw = cell(icon);
        let row_w = n as f32 * cw + (n as f32 - 1.0) * gap;
        let panel_w = (row_w + 2.0 * margin).min(max_panel_w);
        let panel_h = 2.0 * margin + label_h + cw;
        let panel = egui::Rect::from_center_size(screen.center(), egui::vec2(panel_w, panel_h));
        let row_y = panel.top() + margin + label_h;
        let row_left = panel.center().x - row_w / 2.0;

        let mut chosen = None;
        let mut hovered = None;
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show(ui, |ui| {
                ui.painter().rect_filled(panel, 16.0, self.theme.card);

                // Highlighted window's name, centred above the row.
                let label = vis
                    .get(sel)
                    .map(|s| {
                        if s.subtitle.is_empty() {
                            s.title.clone()
                        } else {
                            format!("{} — {}", s.title, s.subtitle)
                        }
                    })
                    .unwrap_or_default();
                ui.painter().text(
                    egui::pos2(panel.center().x, panel.top() + margin + label_h / 2.0),
                    egui::Align2::CENTER_CENTER,
                    label,
                    egui::FontId::proportional(16.0),
                    self.theme.text,
                );

                for (i, s) in vis.iter().enumerate() {
                    let x = row_left + i as f32 * (cw + gap);
                    let cell_rect =
                        egui::Rect::from_min_size(egui::pos2(x, row_y), egui::vec2(cw, cw));
                    let resp =
                        ui.interact(cell_rect, ui.id().with(("switch", i)), egui::Sense::click());
                    if resp.hovered() {
                        hovered = Some(i);
                    }
                    if i == sel {
                        ui.painter()
                            .rect_filled(cell_rect, 12.0, self.theme.tile_selected);
                    } else if resp.hovered() {
                        ui.painter()
                            .rect_filled(cell_rect, 12.0, self.theme.tile_hover);
                    }
                    let inner =
                        egui::Rect::from_center_size(cell_rect.center(), egui::vec2(icon, icon));
                    let live = match self.live {
                        Live::None => false,
                        Live::All => true,
                        Live::Current => i == sel,
                    };
                    self.paint_switch_cell(ui, s, inner, live);
                    self.paint_hint(ui, i, cell_rect, 1.0);
                    if resp.clicked() {
                        chosen = Some(s.selection());
                    }
                }
            });

        // A press outside the panel cancels.
        let pressed_outside = ctx.input(|inp| {
            inp.pointer.any_pressed()
                && inp
                    .pointer
                    .interact_pos()
                    .is_some_and(|pos| !panel.contains(pos))
        });
        if pressed_outside {
            self.closing = true;
        }
        if let Some(i) = hovered {
            self.selected = i;
        }
        chosen
    }

    /// Draw one Alt-Tab tile's content: a live preview with an app-icon badge so the
    /// app stays identifiable, or — where no preview is coming — the big app icon.
    ///
    /// A tile that expects a preview paints the badge layout from the very first
    /// frame, to avoid flickering when the capture lands shortly after.
    fn paint_switch_cell(&self, ui: &egui::Ui, s: &Source, rect: egui::Rect, live: bool) {
        if !live {
            self.paint_app_icon(ui, s, rect);
            return;
        }
        let full = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
        let p = ui.painter();
        p.rect_filled(rect, 6.0, self.theme.thumb); // backdrop for letterboxing
        if let Some((tex, ts)) = self.thumb_tex(&s.key) {
            let scale = (rect.width() / ts.x).min(rect.height() / ts.y);
            let d = egui::Rect::from_center_size(rect.center(), ts * scale);
            p.image(tex, d, full, egui::Color32::WHITE);
        }
        // App-icon badge, bottom-left, so the window stays identifiable.
        if let Some(ic) = self.icons.get(&s.key) {
            let bsz = (rect.width() * 0.34).clamp(20.0, 48.0);
            let brect = egui::Rect::from_min_size(
                egui::pos2(rect.left() + 4.0, rect.bottom() - bsz - 4.0),
                egui::vec2(bsz, bsz),
            );
            let isz = ic.size_vec2();
            let sc = (brect.width() / isz.x).min(brect.height() / isz.y);
            let bd = egui::Rect::from_center_size(brect.center(), isz * sc);
            p.image(ic.id(), bd, full, egui::Color32::WHITE);
        }
    }

    /// Draw the hint badge of the tile at `index`, in the top-left of `rect`: the
    /// character to press, on a dark pill.
    ///
    /// Top-left because the app-icon badge already holds the bottom-left, and a pill
    /// because a bare glyph disappears over a bright preview. It is sized off the tile
    /// and clamped, so it stays readable down to a thumbnail without covering the
    /// preview it labels. `a` is the intro animation's opacity (1.0 once settled).
    fn paint_hint(&self, ui: &egui::Ui, index: usize, rect: egui::Rect, a: f32) {
        let Some(label) = self.hint_label(index) else {
            return;
        };
        let p = ui.painter();
        let size = (rect.width() * 0.2).clamp(16.0, 34.0);
        let badge =
            egui::Rect::from_min_size(rect.min + egui::vec2(5.0, 5.0), egui::vec2(size, size));
        p.rect_filled(
            badge,
            5.0,
            egui::Color32::from_black_alpha(190).gamma_multiply(a),
        );
        p.text(
            badge.center(),
            egui::Align2::CENTER_CENTER,
            label,
            egui::FontId::proportional(size * 0.62),
            self.theme.accent.gamma_multiply(a),
        );
    }

    /// Draw a source's app icon filling `rect` (contain). Falls back to its live
    /// thumbnail, then a generic glyph, when no icon resolved.
    fn paint_app_icon(&self, ui: &egui::Ui, s: &Source, rect: egui::Rect) {
        let full = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
        let p = ui.painter();
        if let Some(ic) = self.icons.get(&s.key) {
            let sz = ic.size_vec2();
            let scale = (rect.width() / sz.x).min(rect.height() / sz.y);
            let d = egui::Rect::from_center_size(rect.center(), sz * scale);
            p.image(ic.id(), d, full, egui::Color32::WHITE);
        } else if let Some((tex, ts)) = self.thumb_tex(&s.key) {
            let scale = (rect.width() / ts.x).min(rect.height() / ts.y);
            let d = egui::Rect::from_center_size(rect.center(), ts * scale);
            p.image(tex, d, full, egui::Color32::WHITE);
        } else if s.is_window {
            draw_window_glyph(p, rect, self.theme.window_accent);
        } else {
            draw_monitor_glyph(p, rect, self.theme.screen_accent);
        }
    }
}

/// Lay out `items` (index, aspect = w/h) as justified rows (each row fills the
/// width), sizing the tiles as large as possible while still fitting the height —
/// so the grid fills the screen (mission-control) instead of hugging the centre.
fn expose_layout(items: &[(usize, f32)], area: egui::Rect, gap: f32) -> Vec<(usize, egui::Rect)> {
    if items.is_empty() {
        return Vec::new();
    }
    let (aw, ah) = (area.width(), area.height());

    // Greedily pack items into rows at a trial row height `th`.
    let group = |th: f32| -> Vec<Vec<(usize, f32)>> {
        let mut rows: Vec<Vec<(usize, f32)>> = Vec::new();
        let mut cur: Vec<(usize, f32)> = Vec::new();
        let mut cur_w = 0.0;
        for &(idx, aspect) in items {
            let w = th * aspect;
            let extra = if cur.is_empty() { w } else { gap + w };
            if !cur.is_empty() && cur_w + extra > aw {
                rows.push(std::mem::take(&mut cur));
                cur.push((idx, aspect));
                cur_w = w;
            } else {
                cur.push((idx, aspect));
                cur_w += extra;
            }
        }
        if !cur.is_empty() {
            rows.push(cur);
        }
        rows
    };
    // Height of a row once justified to the full width (capped so a near-empty
    // last row doesn't balloon to a giant tile).
    let row_h = |row: &[(usize, f32)], th: f32| -> f32 {
        let sum_aspect: f32 = row.iter().map(|(_, a)| *a).sum();
        let total_gap = gap * (row.len() as f32 - 1.0);
        ((aw - total_gap) / sum_aspect).min(th * 1.5)
    };
    let total_h = |th: f32| -> f32 {
        let rows = group(th);
        rows.iter().map(|r| row_h(r, th)).sum::<f32>() + gap * (rows.len().saturating_sub(1) as f32)
    };

    // Bisect the trial row height to the largest value whose laid-out total still
    // fits the available height.
    let (mut lo, mut hi) = (30.0_f32, ah);
    for _ in 0..24 {
        let mid = 0.5 * (lo + hi);
        if total_h(mid) <= ah {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    let rows = group(lo);
    let heights: Vec<f32> = rows.iter().map(|r| row_h(r, lo)).collect();
    let used_h = heights.iter().sum::<f32>() + gap * (rows.len().saturating_sub(1) as f32);

    let mut out = Vec::with_capacity(items.len());
    let mut y = area.top() + ((ah - used_h) * 0.5).max(0.0);
    for (row, &rh) in rows.iter().zip(heights.iter()) {
        let row_w = row.iter().map(|(_, a)| a * rh).sum::<f32>() + gap * (row.len() as f32 - 1.0);
        let mut x = area.left() + ((aw - row_w) * 0.5).max(0.0);
        for &(idx, aspect) in row {
            let w = aspect * rh;
            out.push((
                idx,
                egui::Rect::from_min_size(egui::pos2(x, y), egui::vec2(w, rh)),
            ));
            x += w + gap;
        }
        y += rh + gap;
    }
    out
}

/// A small monitor glyph marking *output* tiles (so a full-screen window can't be
/// mistaken for a screen).
fn draw_monitor_glyph(p: &egui::Painter, r: egui::Rect, col: egui::Color32) {
    let screen = egui::Rect::from_min_max(r.min, egui::pos2(r.max.x, r.max.y - r.height() * 0.28));
    p.rect_stroke(
        screen,
        2.0,
        egui::Stroke::new(1.6, col),
        egui::StrokeKind::Inside,
    );
    let cx = r.center().x;
    p.line_segment(
        [
            egui::pos2(cx - r.width() * 0.18, r.max.y),
            egui::pos2(cx + r.width() * 0.18, r.max.y),
        ],
        egui::Stroke::new(1.6, col),
    );
}

/// A generic window glyph for windows whose app icon could not be resolved.
fn draw_window_glyph(p: &egui::Painter, r: egui::Rect, col: egui::Color32) {
    p.rect_stroke(
        r,
        2.0,
        egui::Stroke::new(1.4, col),
        egui::StrokeKind::Inside,
    );
    p.line_segment(
        [
            egui::pos2(r.min.x, r.min.y + r.height() * 0.3),
            egui::pos2(r.max.x, r.min.y + r.height() * 0.3),
        ],
        egui::Stroke::new(1.4, col),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    /// The tests send no dma-buf frames, so this is never asked to import one.
    struct NoGpu;

    impl DmabufImporter for NoGpu {
        fn import(&mut self, _: &str, _: wl::DmabufFrame) -> Option<(egui::TextureId, egui::Vec2)> {
            None
        }
        fn forget(&mut self, _: &str) {}
    }

    /// A window source; an empty `app_id` makes it a system window.
    fn window(key: &str, app_id: &str) -> Source {
        Source {
            key: key.into(),
            token: format!("Window: {key}"),
            title: app_id.into(),
            subtitle: String::new(),
            filter: app_id.into(),
            is_window: true,
            is_system: app_id.is_empty(),
            app_id: app_id.into(),
            win_title: String::new(),
            output: String::new(),
            dup_index: 0,
        }
    }

    /// One of several identical windows, told apart by its creation-order ordinal.
    fn duplicate(key: &str, app_id: &str, dup_index: usize) -> Source {
        Source {
            dup_index,
            ..window(key, app_id)
        }
    }

    /// The compositor's view of `window(_, app_id)`, as the window holding the focus.
    fn focus(app_id: &str) -> wl::WindowIdentity {
        wl::WindowIdentity {
            app_id: app_id.into(),
            title: String::new(),
            dup_index: 0,
        }
    }

    /// Options for a plain window list with every optional behaviour off; tests switch
    /// on what they exercise.
    fn options() -> Options {
        Options {
            mode: Mode::Windows,
            show_system: false,
            grid: None,
            view: View::Strip,
            hold: false,
            live: Live::All,
            order: Order::ByName,
            window_filters: WindowFilters::default(),
            hints: None,
        }
    }

    /// A compositor keymap for `layout`, as the host hands one to [`App::set_keymap`].
    fn keymap(layout: &str) -> String {
        let ctx = xkbcommon::xkb::Context::new(xkbcommon::xkb::CONTEXT_NO_FLAGS);
        xkbcommon::xkb::Keymap::new_from_names(
            &ctx,
            "",
            "",
            layout,
            "",
            None,
            xkbcommon::xkb::KEYMAP_COMPILE_NO_FLAGS,
        )
        .expect("the layouts under test come with xkeyboard-config")
        .get_as_string(xkbcommon::xkb::KEYMAP_FORMAT_TEXT_V1)
    }

    /// The evdev code of the `n`-th key of the home row (`a`/`q`, `s`, `d`, …).
    fn home_key(n: u32) -> u32 {
        30 + n
    }

    /// A `--app-id` / `--title` filter as the CLI builds it.
    fn filters(app_ids: &[&str], titles: &[&str]) -> WindowFilters {
        WindowFilters::new(
            app_ids.iter().map(|s| (*s).to_string()).collect(),
            titles.iter().map(|s| (*s).to_string()).collect(),
            Vec::new(),
        )
    }

    /// Put the compositor's answer into a filter: the process of the window at each
    /// position of the `offered` window list, `None` for a window it named none for.
    fn with_processes(mut f: WindowFilters, processes: &[Option<u32>]) -> WindowFilters {
        for (i, pid) in processes.iter().enumerate() {
            f.processes.insert(identifier(i), *pid);
        }
        f
    }

    /// A `--pid` filter, with the compositor's answer already in it.
    fn pid_filters(pids: &[u32], processes: &[Option<u32>]) -> WindowFilters {
        with_processes(
            WindowFilters::new(Vec::new(), Vec::new(), pids.to_vec()),
            processes,
        )
    }

    /// The `ext-foreign-toplevel-list-v1` identifier `offered` gives the window at
    /// position `i`.
    fn identifier(i: usize) -> String {
        format!("ext-{i}")
    }

    /// The sources the capture thread hands the overlay for these `(app-id, title)`
    /// windows under `f` — which are exactly the ones it opens a capture session for.
    fn offered(windows: &[(&str, &str)], f: &WindowFilters) -> Vec<Source> {
        let ids: Vec<String> = (0..windows.len()).map(identifier).collect();
        let candidates = windows
            .iter()
            .zip(&ids)
            .map(|(&(app_id, title), identifier)| Candidate {
                app_id,
                title,
                identifier,
            });
        admitted_windows(candidates, f)
            .into_iter()
            .map(|(i, dup_index)| {
                let (app_id, title) = windows[i];
                Source {
                    win_title: title.into(),
                    filter: format!("{app_id} {title}").to_lowercase(),
                    dup_index,
                    ..window(&ids[i], app_id)
                }
            })
            .collect()
    }

    /// An `App` fed by hand in place of the capture thread, driven one frame at a time.
    struct Harness {
        app: App,
        tx: Sender<Msg>,
        out: Outcome,
        ctx: egui::Context,
    }

    impl Harness {
        fn new(opts: Options) -> Self {
            Self::focused_on(opts, None)
        }

        /// With `focused` as the window that had the focus at launch.
        fn focused_on(opts: Options, focused: Option<wl::WindowIdentity>) -> Self {
            let (tx, rx) = mpsc::channel();
            let out: Outcome = Arc::new(Mutex::new(None));
            let gpu_failed = Arc::new(AtomicBool::new(false));
            let app = App::new(rx, out.clone(), opts, focused, Theme::default(), gpu_failed);
            Self {
                app,
                tx,
                out,
                ctx: egui::Context::default(),
            }
        }

        fn send(&self, sources: Vec<Source>) {
            self.tx.send(Msg::Sources(sources)).unwrap();
        }

        fn frame(&mut self) {
            let app = &mut self.app;
            let mut output = self
                .ctx
                .run_ui(egui::RawInput::default(), |ui| app.run_ui(ui, &mut NoGpu));
            // There is no GPU to upload the font atlas to, and egui insists the delta
            // is handled before it's dropped.
            output.textures_delta.clear();
        }

        fn picked(&self) -> Option<String> {
            self.out.lock().unwrap().as_ref().map(|s| s.token.clone())
        }
    }

    #[test]
    fn mru_notices_a_ranking_that_names_no_window() {
        let listed = [window("ext-a", "foot"), window("ext-b", "firefox")];
        // Identifiers the compositor and the toplevel list agree on: ranked.
        let agreed = WindowOrder::mru(["ext-b", "ext-a"].into_iter());
        assert!(!agreed.mru_unmatched(&listed));
        // One window opened since the snapshot still leaves the ranking usable.
        let partial = WindowOrder::mru(["ext-a"].into_iter());
        assert!(!partial.mru_unmatched(&listed));
        // A compositor naming its windows otherwise: nothing to rank by.
        let diverged = WindowOrder::mru(["0x7f2c", "0x7f31"].into_iter());
        assert!(diverged.mru_unmatched(&listed));
        // No history at all is the documented "not supported" case, not a mismatch,
        // and by-name order has nothing to match in the first place.
        assert!(!WindowOrder::mru(std::iter::empty()).mru_unmatched(&listed));
        assert!(!WindowOrder::ByName.mru_unmatched(&listed));
    }

    #[test]
    fn a_release_before_the_source_list_switches_once_it_arrives() {
        let hold = Options {
            hold: true,
            order: Order::Mru,
            ..options()
        };
        let mut h = Harness::focused_on(hold, Some(focus("foot")));
        h.app.arm();
        h.app.confirm_release();
        // Nothing to pick from yet: closing now would switch nowhere.
        assert!(!h.app.closing());

        h.send(vec![window("a", "foot"), window("b", "firefox")]);
        h.frame();
        assert!(h.app.closing());
        assert_eq!(h.picked().as_deref(), Some("Window: b"));
    }

    #[test]
    fn a_release_right_after_arming_switches_to_the_next_window() {
        // The source list is already in when the modifier is seen held, and the modifier
        // is released before another frame is drawn.
        let hold = Options {
            hold: true,
            order: Order::Mru,
            ..options()
        };
        let mut h = Harness::focused_on(hold, Some(focus("foot")));
        h.send(vec![window("a", "foot"), window("b", "firefox")]);
        h.frame();
        h.app.arm();
        h.app.confirm_release();
        assert!(h.app.closing());
        assert_eq!(h.picked().as_deref(), Some("Window: b"));
    }

    #[test]
    fn an_empty_source_list_uses_up_the_initial_advance() {
        // Arming with nothing on offer advances past nothing; windows turning up later
        // don't catch up on it.
        let hold = Options {
            hold: true,
            order: Order::Mru,
            ..options()
        };
        let mut h = Harness::focused_on(hold, Some(focus("foot")));
        h.app.arm();
        h.send(vec![]);
        h.frame();
        h.send(vec![window("a", "foot"), window("b", "firefox")]);
        h.frame();
        h.app.confirm_release();
        assert!(h.app.closing());
        assert_eq!(h.picked().as_deref(), Some("Window: a"));
    }

    #[test]
    fn an_unknown_current_window_starts_on_the_first_tile() {
        // Nothing had the focus; the focused window is hidden from the tiles (a system
        // window); or its identity matches no tile at all (the XWayland app-id blind
        // spot). Either way the first tile is not the window the user is on, so it is
        // the one to switch to.
        for focused in [None, Some(focus("")), Some(focus("steam"))] {
            let hold = Options {
                hold: true,
                ..options()
            };
            let mut h = Harness::focused_on(hold, focused.clone());
            h.send(vec![
                window("s", ""),
                window("a", "foot"),
                window("b", "firefox"),
            ]);
            h.frame();
            h.app.arm();
            h.app.confirm_release();
            assert_eq!(
                h.picked().as_deref(),
                Some("Window: a"),
                "focused: {focused:?}"
            );
        }
    }

    #[test]
    fn the_current_window_is_stepped_over_whatever_the_order() {
        // It leads the list here — as MRU always puts it, and as by-name may: a
        // switcher that offered the window you are already on would switch nowhere.
        for (order, name) in [(Order::ByName, "by-name"), (Order::Mru, "mru")] {
            let hold = Options {
                hold: true,
                order,
                ..options()
            };
            let mut h = Harness::focused_on(hold, Some(focus("foot")));
            h.send(vec![window("a", "foot"), window("b", "firefox")]);
            h.frame();
            h.app.arm();
            h.app.confirm_release();
            assert_eq!(h.picked().as_deref(), Some("Window: b"), "order: {name}");
        }
    }

    #[test]
    fn a_current_window_further_down_leaves_the_first_tile_selected() {
        // By name, the window the user is on is second: the first tile is already
        // somewhere else to go, and stepping over would skip it for nothing.
        let hold = Options {
            hold: true,
            ..options()
        };
        let mut h = Harness::focused_on(hold, Some(focus("firefox")));
        h.send(vec![window("a", "foot"), window("b", "firefox")]);
        h.frame();
        h.app.arm();
        h.app.confirm_release();
        assert_eq!(h.picked().as_deref(), Some("Window: a"));
    }

    #[test]
    fn the_only_window_is_selected_even_when_it_is_the_current_one() {
        // There is nowhere else to go: switching to itself beats switching to nothing.
        let hold = Options {
            hold: true,
            ..options()
        };
        let mut h = Harness::focused_on(hold, Some(focus("foot")));
        h.send(vec![window("a", "foot")]);
        h.frame();
        h.app.arm();
        h.app.confirm_release();
        assert_eq!(h.picked().as_deref(), Some("Window: a"));
    }

    #[test]
    fn identical_windows_are_told_apart_by_their_creation_order() {
        // Two windows of the same app with the same title: only the ordinal says which
        // one the user is on, and the other one is where the switch goes.
        let hold = Options {
            hold: true,
            ..options()
        };
        let second = wl::WindowIdentity {
            dup_index: 1,
            ..focus("foot")
        };
        let mut h = Harness::focused_on(hold, Some(second));
        h.send(vec![duplicate("a", "foot", 0), duplicate("b", "foot", 1)]);
        h.frame();
        h.app.arm();
        h.app.confirm_release();
        assert_eq!(h.picked().as_deref(), Some("Window: a"));
    }

    #[test]
    fn hold_to_switch_shows_nothing_until_revealed() {
        // Sources in and a frame painted, and still not a pixel: until a held modifier
        // settles it, a quick tap may yet be on its way past.
        let mut h = Harness::new(Options {
            hold: true,
            ..options()
        });
        h.send(vec![window("a", "foot"), window("b", "firefox")]);
        h.frame();
        assert_eq!(
            h.app.backdrop(),
            [0.0; 4],
            "the dim would give the overlay away as surely as the tiles"
        );
    }

    #[test]
    fn revealing_brings_back_the_dimmed_backdrop() {
        let mut h = Harness::new(Options {
            hold: true,
            ..options()
        });
        h.app.reveal();
        assert_ne!(h.app.backdrop(), [0.0; 4]);
    }

    #[test]
    fn without_hold_to_switch_the_picker_is_drawn_straight_away() {
        // No tap to mistake the first frames for, so nothing is held back.
        let h = Harness::new(options());
        assert_ne!(h.app.backdrop(), [0.0; 4]);
    }

    #[test]
    fn a_window_filter_keeps_what_it_names_and_nothing_else() {
        let open = [("foot", "vim"), ("firefox", "news"), ("foot", "logs")];
        // Several windows left: both terminals, not the browser.
        let terminals = offered(&open, &filters(&["foot"], &[]));
        assert_eq!(terminals.len(), 2);
        assert!(terminals.iter().all(|s| s.app_id == "foot"));
        // Repeating a flag widens the set; the two kinds of criteria narrow each other.
        assert_eq!(offered(&open, &filters(&["foot", "firefox"], &[])).len(), 3);
        let one = offered(&open, &filters(&["foot"], &["log"]));
        assert_eq!(one.len(), 1);
        assert_eq!(one[0].win_title, "logs");
        // Compared the way wlr-shot compares them: the app id exactly, the title as a
        // substring, neither minding case.
        assert_eq!(offered(&open, &filters(&["FOOT"], &["LOG"])).len(), 1);
        assert!(offered(&open, &filters(&["foo"], &[])).is_empty());
        // Nothing left: no source at all, so no session, no capture, no thumbnail.
        assert!(offered(&open, &filters(&["chromium"], &[])).is_empty());
        // Which is the emptiness the front-ends refuse before raising the overlay.
        let none = filters(&["chromium"], &[]);
        assert!(!open.iter().enumerate().any(|(i, &(app_id, title))| {
            none.admits(Candidate {
                app_id,
                title,
                identifier: &identifier(i),
            })
        }));
        // No filter at all offers every window.
        assert_eq!(offered(&open, &WindowFilters::default()).len(), 3);
    }

    #[test]
    fn a_pid_filter_keeps_every_window_of_the_process() {
        // Two windows of one terminal, a browser, and a second terminal.
        let open = [
            ("foot", "vim"),
            ("firefox", "news"),
            ("foot", "logs"),
            ("foot", "other"),
        ];
        let processes = [Some(4242), Some(777), Some(4242), Some(99)];

        // A process with several windows offers all of them, not the first one.
        let terminal = offered(&open, &pid_filters(&[4242], &processes));
        assert_eq!(terminal.len(), 2);
        assert_eq!(
            terminal
                .iter()
                .map(|s| s.win_title.as_str())
                .collect::<Vec<_>>(),
            ["vim", "logs"]
        );
        // Repeating the flag widens the set, like --app-id and --title.
        assert_eq!(
            offered(&open, &pid_filters(&[4242, 777], &processes)).len(),
            3
        );
        // A pid nothing runs under keeps nothing: no source, no session, no capture.
        assert!(offered(&open, &pid_filters(&[1], &processes)).is_empty());

        // The kinds of criteria narrow each other, as --app-id and --title do.
        let both = with_processes(
            WindowFilters::new(vec!["foot".into()], vec!["log".into()], vec![4242]),
            &processes,
        );
        let one = offered(&open, &both);
        assert_eq!(one.len(), 1);
        assert_eq!(one[0].win_title, "logs");
    }

    #[test]
    fn a_window_whose_process_is_unknown_is_not_offered() {
        let open = [("foot", "vim"), ("foot", "logs")];
        // The compositor named a process for the first window and none for the second.
        let known = pid_filters(&[4242], &[Some(4242), None]);
        let kept = offered(&open, &known);
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].win_title, "vim");
        // Without a --pid flag the same unnamed window is offered as before: the pid
        // is only ever a condition when it was asked for.
        assert_eq!(offered(&open, &WindowFilters::default()).len(), 2);
    }

    #[test]
    fn the_overlay_shows_only_the_windows_the_pid_filter_kept() {
        let open = [("foot", "vim"), ("firefox", "news"), ("foot", "logs")];
        let mut h = Harness::new(options());
        h.send(offered(
            &open,
            &pid_filters(&[4242], &[Some(4242), Some(777), Some(4242)]),
        ));
        h.frame();
        assert_eq!(h.app.visible().len(), 2);
        assert!(h.app.visible().iter().all(|s| s.app_id == "foot"));
    }

    #[test]
    fn the_message_for_an_empty_filter_quotes_every_flag() {
        let f = WindowFilters::new(vec!["foot".into()], vec!["log".into()], vec![4242, 777]);
        assert_eq!(
            f.describe(),
            "--app-id foot --title log --pid 4242 --pid 777"
        );
        // Only a --pid run has to ask the compositor for anything.
        assert!(f.needs_pids());
        assert!(!filters(&["foot"], &[]).needs_pids());
        assert!(!WindowFilters::default().needs_pids());
        // And an empty filter is empty whichever flag is missing.
        assert!(WindowFilters::default().is_empty());
        assert!(!WindowFilters::new(Vec::new(), Vec::new(), vec![1]).is_empty());
    }

    #[test]
    fn a_filtered_list_numbers_identical_windows_as_the_compositor_does() {
        // The ordinal is how a window is named for activation, so it is counted over
        // every toplevel, including the ones the filter drops.
        let open = [("foot", "vim"), ("firefox", "news"), ("foot", "vim")];
        let kept = offered(&open, &filters(&["foot"], &[]));
        assert_eq!(kept.iter().map(|s| s.dup_index).collect::<Vec<_>>(), [0, 1]);
    }

    #[test]
    fn the_overlay_shows_only_the_windows_the_filter_kept() {
        let open = [("foot", "vim"), ("firefox", "news"), ("foot", "logs")];
        let mut h = Harness::new(options());
        h.send(offered(&open, &filters(&["foot"], &[])));
        h.frame();
        assert_eq!(h.app.visible().len(), 2);
        assert!(h.app.visible().iter().all(|s| s.app_id == "foot"));

        // Down to one window, and it is the one the overlay opens on.
        let mut h = Harness::new(options());
        h.send(offered(&open, &filters(&["foot"], &["log"])));
        h.frame();
        assert_eq!(h.app.visible().len(), 1);
        assert_eq!(h.app.visible()[0].win_title, "logs");

        // Down to none: the overlay has nothing to draw.
        let mut h = Harness::new(options());
        h.send(offered(&open, &filters(&["chromium"], &[])));
        h.frame();
        assert!(h.app.visible().is_empty());
    }

    #[test]
    fn the_filter_and_include_system_decide_separately() {
        // A system surface carries no app-id, so --app-id never keeps one.
        let open = [("foot", "vim"), ("", "wlr-draw overlay")];
        assert!(
            offered(&open, &filters(&["foot"], &[]))
                .iter()
                .all(|s| !s.is_system)
        );
        // A title can keep one — and --include-system still decides whether it shows.
        let kept = offered(&open, &filters(&[], &["overlay"]));
        assert_eq!(kept.len(), 1);
        assert!(kept[0].is_system);
        let mut h = Harness::new(options());
        h.send(kept.clone());
        h.frame();
        assert!(h.app.visible().is_empty());
        let mut h = Harness::new(Options {
            show_system: true,
            ..options()
        });
        h.send(kept);
        h.frame();
        assert_eq!(h.app.visible().len(), 1);
    }

    #[test]
    fn the_tiles_are_labelled_with_the_keys_of_the_active_layout() {
        // The overlay is the same; only the keyboard under it differs. A tile carries
        // the character its own layout prints on the key that picks it.
        for (layout, expected) in [("us", ["a", "s", "d"]), ("fr", ["q", "s", "d"])] {
            let mut h = Harness::new(Options {
                hints: Some(HintRow::Home),
                ..options()
            });
            h.app.set_keymap(&keymap(layout));
            h.send(vec![
                window("a", "foot"),
                window("b", "firefox"),
                window("c", "mpv"),
            ]);
            h.frame();
            let labels: Vec<&str> = (0..3).filter_map(|i| h.app.hint_label(i)).collect();
            assert_eq!(labels, expected, "layout: {layout}");
        }
    }

    #[test]
    fn a_hint_key_picks_the_tile_it_labels() {
        // On AZERTY the third home key prints `d`, as it does on QWERTY — but the first
        // prints `q`. Either way it is the position that fires, so the tile the user
        // read is the tile they get.
        let mut h = Harness::new(Options {
            hints: Some(HintRow::Home),
            ..options()
        });
        h.app.set_keymap(&keymap("fr"));
        h.send(vec![
            window("a", "foot"),
            window("b", "firefox"),
            window("c", "mpv"),
        ]);
        h.frame();
        assert_eq!(h.app.hint_label(0), Some("q"));
        assert!(h.app.press_hint(home_key(0)));
        assert!(h.app.closing());
        assert_eq!(h.picked().as_deref(), Some("Window: a"));
    }

    #[test]
    fn hints_run_out_before_the_windows_do() {
        // Nine keys in the home row and twelve windows on screen: the last three carry
        // no hint, and their keys are none of ours to swallow. Tab, the arrows and the
        // mouse still reach them.
        let mut h = Harness::new(Options {
            hints: Some(HintRow::Home),
            ..options()
        });
        h.app.set_keymap(&keymap("fr"));
        let many: Vec<Source> = (0..12)
            .map(|i| window(&format!("w{i}"), &format!("app{i}")))
            .collect();
        h.send(many);
        h.frame();
        assert_eq!(h.app.hints.len(), 9);
        assert!(h.app.hint_label(8).is_some());
        assert!(h.app.hint_label(9).is_none());
        // A key no tile carries is left alone, and picks nothing.
        assert!(!h.app.press_hint(home_key(9)));
        assert!(!h.app.closing());
        assert!(h.picked().is_none());
        // Fewer windows than keys leaves the extra keys inert just the same.
        let mut h = Harness::new(Options {
            hints: Some(HintRow::Home),
            ..options()
        });
        h.app.set_keymap(&keymap("us"));
        h.send(vec![window("a", "foot")]);
        h.frame();
        assert!(!h.app.press_hint(home_key(1)));
        assert!(h.picked().is_none());
    }

    #[test]
    fn the_card_leaves_its_letters_to_the_filter_field() {
        // The picker's card is typed into. A hint there would either eat the keystroke
        // or never fire, so there is none: no label, and the key goes through untouched.
        let mut h = Harness::new(Options {
            view: View::Card,
            hints: Some(HintRow::Home),
            ..options()
        });
        h.app.set_keymap(&keymap("fr"));
        h.send(vec![window("a", "foot"), window("b", "firefox")]);
        h.frame();
        assert!(h.app.hint_label(0).is_none());
        assert!(!h.app.press_hint(home_key(0)));
        assert!(h.picked().is_none());
        // And the field itself still narrows the list, key by key.
        h.app.filter = "fire".into();
        assert_eq!(h.app.visible().len(), 1);
    }

    #[test]
    fn without_the_flag_no_tile_carries_a_hint() {
        // Off unless asked for: the keymap arrives all the same, and nothing comes of
        // it — no label, and every key left to the rest of the UI.
        let mut h = Harness::new(options());
        h.app.set_keymap(&keymap("us"));
        h.send(vec![window("a", "foot"), window("b", "firefox")]);
        h.frame();
        assert!(h.app.hints.is_empty());
        assert!(h.app.hint_label(0).is_none());
        assert!(!h.app.press_hint(home_key(0)));
        assert!(h.picked().is_none());
    }

    #[test]
    fn a_hint_picked_under_a_held_modifier_still_picks() {
        // Alt-Tab holds Alt the whole time. The hint is matched on the physical key and
        // never on the modifier state, so it fires under the held modifier as it would
        // without one — which is the only way it is any use in a switcher.
        let mut h = Harness::focused_on(
            Options {
                hold: true,
                hints: Some(HintRow::Home),
                ..options()
            },
            Some(focus("foot")),
        );
        h.app.set_keymap(&keymap("fr"));
        h.send(vec![
            window("a", "foot"),
            window("b", "firefox"),
            window("c", "mpv"),
        ]);
        h.frame();
        h.app.arm(); // the modifier is down: the overlay is armed
        assert!(h.app.press_hint(home_key(2)));
        assert_eq!(h.picked().as_deref(), Some("Window: c"));
    }

    #[test]
    fn the_overlay_filter_field_narrows_what_the_flags_kept() {
        let open = [("foot", "vim"), ("foot", "logs"), ("firefox", "news")];
        let mut h = Harness::new(options());
        h.send(offered(&open, &filters(&["foot"], &[])));
        h.frame();
        h.app.filter = "log".into();
        assert_eq!(h.app.visible().len(), 1);
        // It narrows; it cannot bring back a window the flags never captured.
        h.app.filter = "firefox".into();
        assert!(h.app.visible().is_empty());
    }
}
