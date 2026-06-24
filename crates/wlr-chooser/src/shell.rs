//! Native `wlr-layer-shell` host for the egui UI — a real rofi-like overlay:
//! overlay layer, optional exclusive keyboard grab, dimmed transparent backdrop.
//!
//! Rendering goes through the shared [`wlr_capture::render::Gpu`] (egui →
//! `egui_glow` on an EGL/GLES context bound to the layer surface). Only this
//! windowing layer differs from a normal app; the whole UI (`ui::App`) is reused
//! unchanged.

use crate::ui::App;
use smithay_client_toolkit::{
    compositor::{CompositorHandler, CompositorState},
    delegate_compositor, delegate_keyboard, delegate_layer, delegate_output, delegate_pointer,
    delegate_registry, delegate_seat,
    output::{OutputHandler, OutputState},
    registry::{ProvidesRegistryState, RegistryState},
    registry_handlers,
    seat::{
        Capability, SeatHandler, SeatState,
        keyboard::{KeyEvent, KeyboardHandler, Keysym, Modifiers, RawModifiers},
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
use std::time::{Duration, Instant};
use wayland_client::{
    Connection, Dispatch, Proxy, QueueHandle,
    globals::registry_queue_init,
    protocol::{wl_keyboard, wl_output, wl_pointer, wl_seat, wl_surface},
};
use wayland_protocols::wp::keyboard_shortcuts_inhibit::zv1::client::{
    zwp_keyboard_shortcuts_inhibit_manager_v1::ZwpKeyboardShortcutsInhibitManagerV1,
    zwp_keyboard_shortcuts_inhibitor_v1::ZwpKeyboardShortcutsInhibitorV1,
};
use wlr_capture::render::Gpu;

struct State {
    registry_state: RegistryState,
    seat_state: SeatState,
    output_state: OutputState,

    layer: LayerSurface,
    keyboard: Option<wl_keyboard::WlKeyboard>,
    pointer: Option<wl_pointer::WlPointer>,

    /// Compositor-shortcuts inhibitor: while the overlay is focused, the compositor
    /// forwards every key (incl. the `Mod1+Tab` chord) to us instead of running its
    /// own bindings. `None` if the compositor lacks the protocol.
    shortcuts_mgr: Option<ZwpKeyboardShortcutsInhibitManagerV1>,
    shortcuts_inhibitor: Option<ZwpKeyboardShortcutsInhibitorV1>,

    egui_ctx: egui::Context,
    app: App,
    gpu: Option<Gpu>,

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
    /// We've painted at least one frame with the modifier genuinely held; gates the
    /// confirm-on-release so the launching chord's tail can't trigger it.
    armed_rendered: bool,
    /// When the layer surface first got keyboard focus, for the fallback arming
    /// window (some compositors report the held modifier just after `enter`).
    enter_at: Option<Instant>,

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
    let conn = Connection::connect_to_env()?;
    let (globals, mut event_queue) = registry_queue_init(&conn)?;
    let qh = event_queue.handle();
    tlog(t0, "wayland connected + globals");

    let compositor =
        CompositorState::bind(&globals, &qh).map_err(|e| anyhow::anyhow!("wl_compositor: {e}"))?;
    let layer_shell =
        LayerShell::bind(&globals, &qh).map_err(|e| anyhow::anyhow!("layer-shell missing: {e}"))?;
    // Optional: present on sway and most wlroots compositors.
    let shortcuts_mgr: Option<ZwpKeyboardShortcutsInhibitManagerV1> =
        globals.bind(&qh, 1..=1, ()).ok();

    let surface = compositor.create_surface(&qh);
    let layer = layer_shell.create_layer_surface(
        &qh,
        surface,
        Layer::Overlay,
        Some(crate::ui::APP_ID),
        None,
    );
    layer.set_anchor(Anchor::TOP | Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT);
    layer.set_keyboard_interactivity(KeyboardInteractivity::Exclusive);
    layer.set_exclusive_zone(-1); // cover everything, including bars
    layer.commit();

    let egui_ctx = egui::Context::default();
    app.apply_theme(&egui_ctx);

    let hold = app.hold();
    let mut state = State {
        registry_state: RegistryState::new(&globals),
        seat_state: SeatState::new(&globals, &qh),
        output_state: OutputState::new(&globals, &qh),
        layer,
        keyboard: None,
        pointer: None,
        shortcuts_mgr,
        shortcuts_inhibitor: None,
        egui_ctx,
        app,
        gpu: None,
        width: 0,
        height: 0,
        scale: 1,
        start: Instant::now(),
        events: Vec::new(),
        modifiers: egui::Modifiers::default(),
        pointer_pos: egui::Pos2::ZERO,
        hold,
        armed: false,
        alt_down: false,
        logo_down: false,
        armed_alt: false,
        armed_logo: false,
        prev_held: false,
        armed_rendered: false,
        enter_at: None,
        t0,
        first_paint_logged: false,
    };

    while !state.app.closing() {
        event_queue.blocking_dispatch(&mut state)?;
    }
    Ok(())
}

impl State {
    fn ensure_gpu(&mut self, conn: &Connection) {
        if self.gpu.is_some() || self.width == 0 {
            return;
        }
        let (pw, ph) = (
            (self.width * self.scale) as i32,
            (self.height * self.scale) as i32,
        );
        self.gpu = Some(Gpu::new(conn, self.layer.wl_surface(), pw, ph));
        tlog(self.t0, "gpu ready (egl init + shader compile)");
    }

    fn render(&mut self) {
        // Record that we've shown at least one frame with the modifier held; this
        // gates confirm-on-release (see `reconcile`).
        if self.armed && self.any_armed_held() {
            self.armed_rendered = true;
        }
        let (pw, ph) = (self.width * self.scale, self.height * self.scale);
        let raw_input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(self.width as f32, self.height as f32),
            )),
            time: Some(self.start.elapsed().as_secs_f64()),
            modifiers: self.modifiers,
            events: std::mem::take(&mut self.events),
            focused: true,
            ..Default::default()
        };
        let backdrop = self.app.backdrop();
        let app = &mut self.app;
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
        self.ensure_gpu(conn);
        // ask for the next frame so we keep draining the capture channel.
        let surface = self.layer.wl_surface().clone();
        surface.frame(qh, surface.clone());
        self.render();
        self.layer.commit();
        if !self.first_paint_logged {
            self.first_paint_logged = true;
            tlog(self.t0, "first frame committed (overlay visible)");
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
        self.layer.wl_surface().set_buffer_scale(new_factor.max(1));
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
        self.app.cancel();
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
            // Stop the compositor from eating our own keybinding chord (e.g.
            // `Mod1+Tab`) while we're up, so Tab reaches us to cycle. Held until
            // the surface (and inhibitor) is dropped at exit.
            if self.shortcuts_inhibitor.is_none()
                && let Some(mgr) = &self.shortcuts_mgr {
                    self.shortcuts_inhibitor =
                        Some(mgr.inhibit_shortcuts(self.layer.wl_surface(), &seat, qh, ()));
                }
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
        self.enter_at = Some(Instant::now());
        if !self.hold {
            return;
        }
        self.alt_down = keysyms.iter().copied().any(is_alt);
        self.logo_down = keysyms.iter().copied().any(is_logo);
        self.reconcile();
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
        self.modifiers = egui::Modifiers {
            alt: modifiers.alt,
            ctrl: modifiers.ctrl,
            shift: modifiers.shift,
            mac_cmd: false,
            command: modifiers.ctrl,
        };
        // Authoritative modifier state for hold-to-switch.
        self.alt_down = modifiers.alt;
        self.logo_down = modifiers.logo;
        self.reconcile();
    }
}

impl State {
    /// Is any modifier we armed on still physically held?
    fn any_armed_held(&self) -> bool {
        (self.armed_alt && self.alt_down) || (self.armed_logo && self.logo_down)
    }

    /// Reconcile hold-to-switch state from the current Alt/Super flags: arm during
    /// the startup window, then confirm the selection on the release edge.
    fn reconcile(&mut self) {
        if !self.hold {
            return;
        }
        if !self.armed && (self.alt_down || self.logo_down) {
            // Arm at focus-in, or shortly after (some compositors report the held
            // modifier just after `enter` rather than in its key set).
            let in_window = self
                .enter_at
                .is_none_or(|t| t.elapsed() < Duration::from_millis(300));
            if in_window {
                self.armed = true;
                self.armed_alt = self.alt_down;
                self.armed_logo = self.logo_down;
                self.app.arm();
            }
        }
        // Confirm on release — but only after we've painted a frame with the
        // modifier genuinely held, so the launching chord's tail can't fire it.
        let held = self.any_armed_held();
        if self.armed && self.armed_rendered && self.prev_held && !held {
            self.app.confirm_release();
        }
        self.prev_held = held;
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
        if self.armed && pressed && is_tab {
            let forward = event.keysym == Keysym::Tab && !self.modifiers.shift;
            self.app.cycle(forward);
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
        if pressed && !self.modifiers.ctrl && !self.modifiers.alt
            && let Some(txt) = event.utf8
                && !txt.chars().any(|c| c.is_control()) && !txt.is_empty() {
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

delegate_compositor!(State);
delegate_output!(State);
delegate_seat!(State);
delegate_keyboard!(State);
delegate_pointer!(State);
delegate_layer!(State);
delegate_registry!(State);
