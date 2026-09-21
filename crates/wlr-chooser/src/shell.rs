//! Native `wlr-layer-shell` host for the egui UI — a real rofi-like overlay:
//! overlay layer, optional exclusive keyboard grab, dimmed transparent backdrop.
//!
//! Rendering goes through the shared [`wlr_capture::render::Gpu`] (egui →
//! `egui_glow` on an EGL/GLES context bound to the layer surface). Only this
//! windowing layer differs from a normal app; the whole UI (`ui::App`) is reused
//! unchanged.
//!
//! The host is [`Host`], and it outlives the overlay it shows: a one-shot run
//! ([`run`]) builds one, shows one overlay and drops it, while the switcher's daemon
//! keeps one for the whole session and shows every overlay on it.

use crate::ui::App;
use rustix::event::{PollFd, PollFlags, poll};
use smithay_client_toolkit::{
    compositor::{CompositorHandler, CompositorState, FrameCallbackData},
    delegate_dispatch2, delegate_registry,
    output::{OutputHandler, OutputState},
    registry::{ProvidesRegistryState, RegistryState},
    registry_handlers,
    seat::{
        Capability, SeatHandler, SeatState,
        keyboard::{KeyEvent, KeyboardHandler, Keymap, Keysym, Modifiers, RawModifiers},
        pointer::{PointerEvent, PointerEventKind, PointerHandler},
    },
    shell::{
        WaylandSurface,
        wlr_layer::{
            Anchor, KeyboardInteractivity, Layer, LayerShell, LayerShellHandler, LayerSurface,
            LayerSurfaceConfigure,
        },
    },
};
use std::os::fd::{AsFd, BorrowedFd};
use std::time::Instant;
use wayland_client::{
    Connection, Dispatch, EventQueue, Proxy, QueueHandle,
    globals::registry_queue_init,
    protocol::{wl_keyboard, wl_output, wl_pointer, wl_seat, wl_surface},
};
use wayland_protocols::wp::keyboard_shortcuts_inhibit::zv1::client::{
    zwp_keyboard_shortcuts_inhibit_manager_v1::ZwpKeyboardShortcutsInhibitManagerV1,
    zwp_keyboard_shortcuts_inhibitor_v1::ZwpKeyboardShortcutsInhibitorV1,
};
use wlr_capture::render::Gpu;
use wlr_capture::theme;

struct State {
    registry_state: RegistryState,
    seat_state: SeatState,
    output_state: OutputState,

    compositor: CompositorState,
    layer_shell: LayerShell,
    /// The overlay's surface, built for it and destroyed with it. The compositor
    /// picks which output a layer surface belongs to when it is created, so the next
    /// overlay — which may want another output — gets one of its own.
    layer: Option<LayerSurface>,
    /// A surface with no role, never committed, kept only so a prewarmed host has
    /// somewhere to realise its EGL context before any overlay exists.
    scratch: Option<wl_surface::WlSurface>,
    keyboard: Option<wl_keyboard::WlKeyboard>,
    pointer: Option<wl_pointer::WlPointer>,
    /// The seat the keyboard came from, to inhibit its shortcuts on each overlay's
    /// own surface.
    seat: Option<wl_seat::WlSeat>,

    /// Compositor-shortcuts inhibitor: while the overlay is focused, the compositor
    /// forwards every key (incl. the `Mod1+Tab` chord) to us instead of running its
    /// own bindings. `None` if the compositor lacks the protocol.
    shortcuts_mgr: Option<ZwpKeyboardShortcutsInhibitManagerV1>,
    shortcuts_inhibitor: Option<ZwpKeyboardShortcutsInhibitorV1>,

    egui_ctx: egui::Context,
    /// The overlay being shown, or `None` while the host sits idle between two.
    app: Option<App>,
    gpu: Option<Gpu>,
    /// The seat's keymap, as last announced. Kept because it is announced once, with
    /// the keyboard: an overlay created later would otherwise never learn what the
    /// physical keys its hints name actually print.
    keymap: Option<String>,

    // logical size (points) and integer scale.
    width: u32,
    height: u32,
    scale: u32,

    start: Instant,
    events: Vec<egui::Event>,
    modifiers: egui::Modifiers,
    pointer_pos: egui::Pos2,

    // --- Hold-to-switch state ---
    /// Hold-to-switch on: watch the launch modifier (Alt/Super) to arm + confirm.
    hold: bool,
    /// A launch modifier was observed held → Tab cycles and its release confirms.
    armed: bool,
    /// Current physical state of Alt / Super (from the modifier mask + raw keysyms).
    alt_down: bool,
    logo_down: bool,
    /// Which modifier(s) were held when we armed; we confirm once none remain held.
    armed_alt: bool,
    armed_logo: bool,
    /// Previous "an armed modifier is held" state, to detect the release edge.
    prev_held: bool,
    /// Keyboard focus came in and the modifier state that must follow it hasn't yet:
    /// only then can a launch modifier released before focus be told apart.
    awaiting_enter_modifiers: bool,

    /// Process start, for cold-start timing.
    t0: Instant,
    /// Whether the first painted frame has been logged (timing).
    first_paint_logged: bool,
}

/// Lightweight cold-start timing, gated by `WLR_CHOOSER_TIMING=1`, to find where
/// the milliseconds go before the overlay is visible. A no-op unless enabled.
pub fn tlog(t0: Instant, label: &str) {
    if std::env::var_os("WLR_CHOOSER_TIMING").is_some() {
        eprintln!(
            "[timing] {:>7.2} ms  {label}",
            t0.elapsed().as_secs_f64() * 1000.0
        );
    }
}

/// xkb keysyms that count as "Alt" for hold-to-switch (either Alt or Meta).
fn is_alt(k: Keysym) -> bool {
    matches!(
        k,
        Keysym::Alt_L | Keysym::Alt_R | Keysym::Meta_L | Keysym::Meta_R
    )
}

/// xkb keysyms that count as "Super"/Logo (the `$mod` key on most setups).
fn is_logo(k: Keysym) -> bool {
    matches!(k, Keysym::Super_L | Keysym::Super_R)
}

/// Run the picker as a layer-shell overlay until the user picks or cancels.
/// `t0` is the process start, for cold-start timing (see [`tlog`]).
pub fn run(app: App, t0: Instant) -> anyhow::Result<()> {
    Host::new()?.show(app, t0)
}

/// A layer-shell host, reusable across overlays.
///
/// Building one is most of the cold start: the Wayland connection and — some sixty
/// milliseconds of it on an NVIDIA driver — the EGL context with its compiled
/// shaders. [`run`] builds one, shows one overlay and drops it; the
/// switcher's daemon builds one at startup and shows every overlay on it, so each
/// costs no more than building a surface and painting a frame.
///
/// The surface is what does *not* carry over. A compositor settles which output a
/// layer surface belongs to when it is created, so a host that kept one would pin
/// every overlay to whichever screen was in front when it started; each overlay
/// builds its own and the context is bound to it in turn. Between two, the host holds
/// no surface at all — nothing on screen, no keyboard held, and outputs free to come
/// and go.
pub struct Host {
    // Declaration order is drop order, and it matters here: the state owns the `Gpu`,
    // whose destructor calls into EGL, which reaches the compositor through this
    // connection. Drop the connection first and those calls land on a closed display —
    // a segfault on the way out. The state goes first, the connection last.
    state: State,
    queue: EventQueue<State>,
    conn: Connection,
}

impl Host {
    /// Connect and bind the globals. No surface yet: each overlay builds its own.
    pub fn new() -> anyhow::Result<Host> {
        let conn = Connection::connect_to_env()?;
        let (globals, queue) = registry_queue_init(&conn)?;
        let qh = queue.handle();

        let compositor = CompositorState::bind(&globals, &qh)
            .map_err(|e| anyhow::anyhow!("wl_compositor: {e}"))?;
        let layer_shell = LayerShell::bind(&globals, &qh)
            .map_err(|e| anyhow::anyhow!("layer-shell missing: {e}"))?;
        // Optional: present on sway and most wlroots compositors.
        let shortcuts_mgr: Option<ZwpKeyboardShortcutsInhibitManagerV1> =
            globals.bind(&qh, 1..=1, ()).ok();

        let state = State {
            registry_state: RegistryState::new(&globals),
            seat_state: SeatState::new(&globals, &qh),
            output_state: OutputState::new(&globals, &qh),
            compositor,
            layer_shell,
            layer: None,
            scratch: None,
            keyboard: None,
            pointer: None,
            seat: None,
            shortcuts_mgr,
            shortcuts_inhibitor: None,
            egui_ctx: egui::Context::default(),
            app: None,
            gpu: None,
            keymap: None,
            width: 0,
            height: 0,
            scale: 1,
            start: Instant::now(),
            events: Vec::new(),
            modifiers: egui::Modifiers::default(),
            pointer_pos: egui::Pos2::ZERO,
            hold: false,
            armed: false,
            alt_down: false,
            logo_down: false,
            armed_alt: false,
            armed_logo: false,
            prev_held: false,
            awaiting_enter_modifiers: false,
            t0: Instant::now(),
            first_paint_logged: false,
        };
        Ok(Host { state, queue, conn })
    }

    /// Build up front everything the first overlay would otherwise build while the
    /// user waits: the seat (and its keymap), the EGL context, the compiled shaders
    /// and the glyph atlas. For a host that will show more than one overlay; a
    /// one-shot run has nothing to gain from it.
    pub fn prewarm(&mut self, t0: Instant) -> anyhow::Result<()> {
        // The seat and output globals, and with the keyboard the keymap the tile
        // hints are named from.
        self.queue.roundtrip(&mut self.state)?;
        tlog(t0, "seat + outputs bound");
        // The theme the overlays will use: its fonts are what the atlas is made of,
        // and resolving them is cached process-wide for the ones that follow.
        theme::Theme::load().apply(&self.state.egui_ctx);
        tlog(t0, "theme applied (fonts)");
        // The context has to be realised on *some* surface, and no overlay exists yet
        // — nor would its surface outlive it. A role-less `wl_surface`, never
        // committed and so never shown, is enough to build it on; each overlay then
        // takes it over with `Gpu::bind`.
        let scratch = self.state.compositor.create_surface(&self.queue.handle());
        self.state.ensure_gpu(&self.conn, &scratch);
        self.state.scratch = Some(scratch);
        let ctx = self.state.egui_ctx.clone();
        if let Some(gpu) = self.state.gpu.as_mut() {
            gpu.prewarm(&ctx);
        }
        tlog(t0, "glyph atlas uploaded");
        Ok(())
    }

    /// Wait until `other` has something to read, keeping the Wayland connection
    /// alive meanwhile.
    ///
    /// How a host with no overlay on it waits for the next one. Nothing of ours is on
    /// screen, but the compositor keeps talking — outputs come and go, scales change,
    /// and it pings — and a connection left unread fills its buffer and wedges, so
    /// the two are waited on together rather than the socket alone.
    pub fn idle_until_readable(&mut self, other: BorrowedFd<'_>) -> anyhow::Result<()> {
        loop {
            self.queue.dispatch_pending(&mut self.state)?;
            self.queue.flush()?;
            // `None` means events arrived between the dispatch and here: go round and
            // hand them over before sleeping on the fd.
            let Some(guard) = self.queue.prepare_read() else {
                continue;
            };
            let wayland = self.conn.as_fd();
            let mut fds = [
                PollFd::new(&wayland, PollFlags::IN),
                PollFd::new(&other, PollFlags::IN),
            ];
            poll(&mut fds, None)?;
            let (wayland_ready, other_ready) =
                (!fds[0].revents().is_empty(), !fds[1].revents().is_empty());
            if wayland_ready {
                // Also how the daemon learns the compositor is gone: the read fails
                // and the error takes it down, rather than leaving it on a dead
                // connection answering keybindings with nothing.
                guard.read()?;
            } else {
                drop(guard);
            }
            if other_ready {
                return Ok(());
            }
        }
    }

    /// Show one overlay: build its surface, run until the user picks or cancels,
    /// then take it down and hand the host back for the next one. `t0` is the start
    /// of *this* overlay, for timing (see [`tlog`]).
    pub fn show(&mut self, app: App, t0: Instant) -> anyhow::Result<()> {
        self.state.begin(app, t0, &self.queue.handle());
        // A commit with no buffer attached is what asks for a configure; the frame
        // painted in answer is what maps the surface.
        if let Some(layer) = self.state.layer.as_ref() {
            layer.commit();
        }
        while !self.state.closing() {
            self.queue.blocking_dispatch(&mut self.state)?;
        }
        self.state.end();
        // Let the teardown reach the compositor before the caller moves on — focusing
        // the window it picked, answering a client.
        self.queue.roundtrip(&mut self.state)?;
        Ok(())
    }
}

impl State {
    /// Whether the run is over — no overlay installed, or the one installed is done.
    fn closing(&self) -> bool {
        self.app.as_ref().is_none_or(|a| a.closing())
    }

    /// Install the overlay for one run, resetting everything the previous one left
    /// behind so a reused host opens exactly like a fresh process would.
    fn begin(&mut self, app: App, t0: Instant, qh: &QueueHandle<Self>) {
        // egui's memory is where widget state lives — scroll offsets, which field
        // holds the focus, animation clocks. Wiping it is what keeps the second
        // overlay from inheriting the first one's; the fonts and the glyph atlas live
        // elsewhere and survive, which is the whole point of reusing the context.
        self.egui_ctx.memory_mut(|m| *m = Default::default());
        app.apply_theme(&self.egui_ctx);
        tlog(t0, "theme applied (fonts)");
        self.hold = app.hold();
        self.app = Some(app);
        if let (Some(app), Some(keymap)) = (self.app.as_mut(), self.keymap.as_ref()) {
            app.set_keymap(keymap);
        }
        self.start = Instant::now();
        self.events.clear();
        self.modifiers = egui::Modifiers::default();
        self.pointer_pos = egui::Pos2::ZERO;
        self.armed = false;
        self.alt_down = false;
        self.logo_down = false;
        self.armed_alt = false;
        self.armed_logo = false;
        self.prev_held = false;
        self.awaiting_enter_modifiers = false;
        self.t0 = t0;
        self.first_paint_logged = false;

        // The surface comes last, and it is this overlay's own: the compositor reads
        // the focused output when the layer surface is created, so one built at
        // daemon startup would pin every overlay to whichever screen was in front
        // back then.
        let surface = self.compositor.create_surface(qh);
        let layer = self.layer_shell.create_layer_surface(
            qh,
            surface,
            Layer::Overlay,
            Some(crate::ui::APP_ID),
            None,
        );
        layer.set_anchor(Anchor::TOP | Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT);
        layer.set_keyboard_interactivity(KeyboardInteractivity::Exclusive);
        layer.set_exclusive_zone(-1); // cover everything, including bars
        // Stop the compositor from eating our own keybinding chord (e.g. `Mod1+Tab`)
        // while we're up, so Tab reaches us to cycle. Tied to the surface, so it is
        // asked for again with every overlay.
        if let (Some(mgr), Some(seat)) = (&self.shortcuts_mgr, &self.seat) {
            self.shortcuts_inhibitor =
                Some(mgr.inhibit_shortcuts(layer.wl_surface(), seat, qh, ()));
        }
        // A context built earlier draws to the new surface from here on; at the last
        // size we knew, which the first configure corrects.
        if let Some(gpu) = self.gpu.as_mut() {
            gpu.bind(
                layer.wl_surface(),
                (self.width * self.scale) as i32,
                (self.height * self.scale) as i32,
            );
        }
        self.layer = Some(layer);
    }

    /// Take the overlay down: the surface goes, which hands the keyboard back and
    /// clears the screen, while the host — and the EGL context the next overlay
    /// reuses — stays.
    fn end(&mut self) {
        self.app = None;
        if let Some(inhibitor) = self.shortcuts_inhibitor.take() {
            inhibitor.destroy();
        }
        // In this order: the GPU frees what the overlay left on it while its context
        // is still current, then lets go of the surface, and only then is the
        // compositor told to forget it.
        if let Some(gpu) = self.gpu.as_mut() {
            gpu.release_textures(&self.egui_ctx);
            gpu.unbind();
        }
        self.layer = None;
    }

    fn ensure_gpu(&mut self, conn: &Connection, surface: &wl_surface::WlSurface) {
        if self.gpu.is_some() {
            return;
        }
        // A host built ahead of any overlay has no configure yet, so no size: build
        // at 1×1 and let the first `configure` resize. What costs is realising the
        // context, not the size it is realised at.
        let (pw, ph) = (
            (self.width * self.scale).max(1) as i32,
            (self.height * self.scale).max(1) as i32,
        );
        self.gpu = Some(Gpu::new(conn, surface, pw, ph));
        tlog(self.t0, "gpu ready (egl init + shader compile)");
    }

    fn render(&mut self) {
        let (pw, ph) = (self.width * self.scale, self.height * self.scale);
        let raw_input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(self.width as f32, self.height as f32),
            )),
            time: Some(self.start.elapsed().as_secs_f64()),
            events: std::mem::take(&mut self.events),
            focused: true,
            ..Default::default()
        };
        let Some(app) = self.app.as_mut() else {
            return;
        };
        let backdrop = app.backdrop();
        let Some(gpu) = self.gpu.as_mut() else {
            return;
        };
        gpu.render(
            &self.egui_ctx,
            raw_input,
            self.scale as f32,
            (pw, ph),
            backdrop,
            |ui, imp| app.run_ui(ui, imp),
        );
    }

    fn draw_frame(&mut self, conn: &Connection, qh: &QueueHandle<Self>) {
        // Once closing, don't paint: a frame now would flash the overlay a quick tap
        // kept blank. Idle between two overlays, there is nothing to paint at all.
        if self.closing() {
            return;
        }
        let Some(surface) = self.layer.as_ref().map(|l| l.wl_surface().clone()) else {
            return;
        };
        self.ensure_gpu(conn, &surface);
        // ask for the next frame so we keep draining the capture channel.
        surface.frame(qh, FrameCallbackData(surface.clone()));
        self.render();
        surface.commit();
        if !self.first_paint_logged {
            self.first_paint_logged = true;
            tlog(self.t0, "first frame committed (overlay visible)");
        }
    }
}

impl Drop for State {
    /// Let EGL go of the overlay's surface before anything else is torn down.
    ///
    /// Fields are dropped in declaration order, and the layer surface is declared
    /// well before the `Gpu`: without this, `eglDestroySurface` would run against a
    /// `wl_surface` the compositor has already forgotten.
    fn drop(&mut self) {
        if let Some(gpu) = self.gpu.as_mut() {
            gpu.unbind();
        }
        if let Some(scratch) = self.scratch.take() {
            scratch.destroy();
        }
    }
}

impl CompositorHandler for State {
    fn scale_factor_changed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        new_factor: i32,
    ) {
        self.scale = new_factor.max(1) as u32;
        if let Some(layer) = self.layer.as_ref() {
            layer.wl_surface().set_buffer_scale(new_factor.max(1));
        }
        if let (Some(gpu), true) = (self.gpu.as_ref(), self.width > 0) {
            gpu.resize(
                (self.width * self.scale) as i32,
                (self.height * self.scale) as i32,
            );
        }
    }

    fn transform_changed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: wl_output::Transform,
    ) {
    }

    fn frame(
        &mut self,
        conn: &Connection,
        qh: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: u32,
    ) {
        self.draw_frame(conn, qh);
    }

    fn surface_enter(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: &wl_output::WlOutput,
    ) {
    }
    fn surface_leave(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: &wl_output::WlOutput,
    ) {
    }
}

impl LayerShellHandler for State {
    fn closed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &LayerSurface) {
        if let Some(app) = self.app.as_mut() {
            app.cancel();
        }
    }

    fn configure(
        &mut self,
        conn: &Connection,
        qh: &QueueHandle<Self>,
        _: &LayerSurface,
        configure: LayerSurfaceConfigure,
        _: u32,
    ) {
        let (w, h) = configure.new_size;
        if w > 0 && h > 0 {
            self.width = w;
            self.height = h;
        }
        if self.width == 0 {
            return;
        }
        if let Some(gpu) = self.gpu.as_ref() {
            gpu.resize(
                (self.width * self.scale) as i32,
                (self.height * self.scale) as i32,
            );
        }
        self.draw_frame(conn, qh);
    }
}

impl SeatHandler for State {
    fn seat_state(&mut self) -> &mut SeatState {
        &mut self.seat_state
    }
    fn new_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}
    fn new_capability(
        &mut self,
        _: &Connection,
        qh: &QueueHandle<Self>,
        seat: wl_seat::WlSeat,
        cap: Capability,
    ) {
        if cap == Capability::Keyboard && self.keyboard.is_none() {
            self.keyboard = self.seat_state.get_keyboard(qh, &seat, None).ok();
            // Kept for the shortcuts inhibitor, which is asked for per overlay: it
            // names a surface, and every overlay brings a new one (see
            // [`State::begin`]).
            self.seat = Some(seat.clone());
        }
        if cap == Capability::Pointer && self.pointer.is_none() {
            self.pointer = self.seat_state.get_pointer(qh, &seat).ok();
        }
    }
    fn remove_capability(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: wl_seat::WlSeat,
        _: Capability,
    ) {
    }
    fn remove_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}
}

impl KeyboardHandler for State {
    fn enter(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: &wl_surface::WlSurface,
        _: u32,
        _: &[u32],
        keysyms: &[Keysym],
    ) {
        // Primary arming: the set of keys already held at focus-in. On wlroots the
        // modifier that triggered the chord (Alt or Super) is still down here.
        if !self.hold {
            return;
        }
        self.alt_down = keysyms.iter().copied().any(is_alt);
        self.logo_down = keysyms.iter().copied().any(is_logo);
        self.reconcile();
        // Some compositors report the held modifier in the `modifiers` event that
        // follows `enter` rather than in its key set; decide once it's in.
        self.awaiting_enter_modifiers = true;
    }
    fn leave(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: &wl_surface::WlSurface,
        _: u32,
    ) {
    }
    fn press_key(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: u32,
        event: KeyEvent,
    ) {
        self.key(event, true);
    }
    fn release_key(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: u32,
        event: KeyEvent,
    ) {
        self.key(event, false);
    }
    fn repeat_key(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: u32,
        event: KeyEvent,
    ) {
        self.key(event, true);
    }
    fn update_keymap(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        keymap: Keymap<'_>,
    ) {
        // The tile hints name physical keys; only the keymap says what this layout
        // prints on them. It arrives before focus does, so the labels are right from
        // the first frame, and again whenever the user switches layout. Kept as well
        // as forwarded: on a reused host it is announced long before the overlay that
        // needs it exists (see [`State::begin`]).
        let keymap = keymap.as_string();
        if let Some(app) = self.app.as_mut() {
            app.set_keymap(&keymap);
        }
        self.keymap = Some(keymap);
    }

    fn update_modifiers(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: u32,
        modifiers: Modifiers,
        _: RawModifiers,
        _: u32,
    ) {
        let mods = egui::Modifiers {
            alt: modifiers.alt,
            ctrl: modifiers.ctrl,
            shift: modifiers.shift,
            mac_cmd: false,
            command: modifiers.ctrl,
        };
        if mods != self.modifiers {
            self.modifiers = mods;
            self.events.push(egui::Event::ModifiersChanged(mods));
        }
        // Authoritative modifier state for hold-to-switch.
        self.alt_down = modifiers.alt;
        self.logo_down = modifiers.logo;
        self.reconcile();
        if std::mem::take(&mut self.awaiting_enter_modifiers) {
            self.infer_release();
        }
    }
}

impl State {
    /// Is any modifier we armed on still physically held?
    fn any_armed_held(&self) -> bool {
        (self.armed_alt && self.alt_down) || (self.armed_logo && self.logo_down)
    }

    /// Reconcile hold-to-switch state from the current Alt/Super flags: arm once one is
    /// held, then confirm the selection on the release edge.
    fn reconcile(&mut self) {
        if !self.hold {
            return;
        }
        if !self.armed
            && (self.alt_down || self.logo_down)
            && let Some(app) = self.app.as_mut()
        {
            self.armed = true;
            self.armed_alt = self.alt_down;
            self.armed_logo = self.logo_down;
            app.arm();
            // A held modifier means a switch the user is steering, not a tap passing
            // through.
            app.reveal();
        }
        // Confirm on the release edge, without waiting for a painted frame: a quick
        // release deserves the switch it asked for.
        let held = self.any_armed_held();
        if self.armed
            && self.prev_held
            && !held
            && let Some(app) = self.app.as_mut()
        {
            app.confirm_release();
        }
        self.prev_held = held;
    }

    /// Confirm if no launch modifier is held once focus-in's modifier state is known:
    /// it was released before we got focus, so no release event will come.
    fn infer_release(&mut self) {
        if !self.armed
            && let Some(app) = self.app.as_mut()
        {
            app.arm();
            app.confirm_release();
        }
    }

    fn key(&mut self, event: KeyEvent, pressed: bool) {
        // Raw-keysym modifier tracking: a second, compositor-independent signal
        // alongside `update_modifiers` (modifier-mask ordering vs. enter varies).
        if is_alt(event.keysym) {
            self.alt_down = pressed;
            self.reconcile();
        }
        if is_logo(event.keysym) {
            self.logo_down = pressed;
            self.reconcile();
        }
        // While armed, Tab / Shift+Tab cycle the highlight instead of reaching
        // egui (its TextEdit would otherwise eat Tab for focus traversal). Some
        // compositors send `ISO_Left_Tab` for Shift+Tab.
        let is_tab = event.keysym == Keysym::Tab || event.keysym == Keysym::ISO_Left_Tab;
        if self.armed
            && pressed
            && is_tab
            && let Some(app) = self.app.as_mut()
        {
            let forward = event.keysym == Keysym::Tab && !self.modifiers.shift;
            app.cycle(forward);
            return;
        }
        // A tile hint is a physical key, so it is matched on the evdev code rather than
        // on the keysym the layout derives from it. It picks straight away; the
        // keystroke stops here so it can't also land in the UI.
        if pressed
            && self
                .app
                .as_mut()
                .is_some_and(|app| app.press_hint(event.raw_code))
        {
            return;
        }
        if let Some(key) = map_key(event.keysym) {
            self.events.push(egui::Event::Key {
                key,
                physical_key: None,
                pressed,
                repeat: false,
                modifiers: self.modifiers,
            });
        }
        if pressed
            && !self.modifiers.ctrl
            && !self.modifiers.alt
            && let Some(txt) = event.utf8
            && !txt.chars().any(|c| c.is_control())
            && !txt.is_empty()
        {
            self.events.push(egui::Event::Text(txt));
        }
    }
}

impl PointerHandler for State {
    fn pointer_frame(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_pointer::WlPointer,
        events: &[PointerEvent],
    ) {
        for e in events {
            let pos = egui::pos2(e.position.0 as f32, e.position.1 as f32);
            match e.kind {
                PointerEventKind::Enter { .. } | PointerEventKind::Motion { .. } => {
                    self.pointer_pos = pos;
                    self.events.push(egui::Event::PointerMoved(pos));
                }
                PointerEventKind::Leave { .. } => {
                    self.events.push(egui::Event::PointerGone);
                }
                PointerEventKind::Press { button, .. }
                | PointerEventKind::Release { button, .. } => {
                    let pressed = matches!(e.kind, PointerEventKind::Press { .. });
                    let btn = match button {
                        0x110 => egui::PointerButton::Primary,
                        0x111 => egui::PointerButton::Secondary,
                        0x112 => egui::PointerButton::Middle,
                        _ => continue,
                    };
                    self.events.push(egui::Event::PointerButton {
                        pos: self.pointer_pos,
                        button: btn,
                        pressed,
                        modifiers: self.modifiers,
                    });
                }
                PointerEventKind::Axis {
                    vertical,
                    horizontal,
                    ..
                } => {
                    let delta = egui::vec2(-horizontal.absolute as f32, -vertical.absolute as f32);
                    self.events.push(egui::Event::MouseWheel {
                        unit: egui::MouseWheelUnit::Point,
                        delta,
                        phase: egui::TouchPhase::Move,
                        modifiers: self.modifiers,
                    });
                }
            }
        }
    }
}

impl OutputHandler for State {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.output_state
    }
    fn new_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
    fn update_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
    fn output_destroyed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
}

impl ProvidesRegistryState for State {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry_state
    }
    registry_handlers![OutputState, SeatState];
}

fn map_key(k: Keysym) -> Option<egui::Key> {
    use egui::Key;
    Some(match k {
        Keysym::Escape => Key::Escape,
        Keysym::Return | Keysym::KP_Enter => Key::Enter,
        Keysym::Tab | Keysym::ISO_Left_Tab => Key::Tab,
        Keysym::BackSpace => Key::Backspace,
        Keysym::Delete => Key::Delete,
        Keysym::Left => Key::ArrowLeft,
        Keysym::Right => Key::ArrowRight,
        Keysym::Up => Key::ArrowUp,
        Keysym::Down => Key::ArrowDown,
        Keysym::Home => Key::Home,
        Keysym::End => Key::End,
        Keysym::space => Key::Space,
        _ => return None,
    })
}

// keyboard-shortcuts-inhibit: neither object carries events we act on (the
// inhibitor's active/inactive are advisory), so the handlers are empty.
impl Dispatch<ZwpKeyboardShortcutsInhibitManagerV1, ()> for State {
    fn event(
        _: &mut Self,
        _: &ZwpKeyboardShortcutsInhibitManagerV1,
        _: <ZwpKeyboardShortcutsInhibitManagerV1 as Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}
impl Dispatch<ZwpKeyboardShortcutsInhibitorV1, ()> for State {
    fn event(
        _: &mut Self,
        _: &ZwpKeyboardShortcutsInhibitorV1,
        _: <ZwpKeyboardShortcutsInhibitorV1 as Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

delegate_dispatch2!(State);
delegate_registry!(State);
