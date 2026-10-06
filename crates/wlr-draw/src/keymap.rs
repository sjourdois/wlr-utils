//! Configurable keybindings and the pen's dwell settings: the `[draw]` section of
//! `config.toml`.
//!
//! Each action resolves to a [`Trigger`] — a regular key (an XKB keysym), a modifier, or
//! a mouse button. Names follow the convention of sway's `bindsym` (`space`, `Caps_Lock`,
//! `plus`, `a`, `button8`, `BTN_SIDE`, …), parsed case-insensitively, keys via
//! libxkbcommon, so anything you can bind in your compositor you can bind here. The left
//! and right buttons draw and move, and stay out of reach. A missing section or key falls
//! back to the built-in defaults.
//!
//! The held roles (`passthrough`, `constrain`, `spotlight`, `snap-invert`) take a modifier,
//! a regular key, a button (but `passthrough`, whose overlay gets no clicks), or a list of
//! them, any of which engages the role; the rest are discrete actions.
//!
//! Next to the bindings, `[draw]` holds the two settings the `snap` binding acts on:
//! `dwell` (whether the pen snaps on its own) and `dwell-ms` (how long it must hold
//! still).
//!
//! Example (some of the defaults):
//! ```toml
//! [draw]
//! dwell = true
//! dwell-ms = 650
//!
//! [draw.keys]
//! pen = "p"
//! save = "w"
//! width-inc = ["plus", "equal"]   # a single string or a list
//! passthrough = "caps"            # a modifier (caps/ctrl/shift/alt/super), a key, or a list
//! constrain = "ctrl"
//! spotlight = "shift"
//! snap = "d"
//! snap-invert = "alt"
//! ```

use crate::tr;
use serde::Deserialize;
use smithay_client_toolkit::seat::keyboard::Keysym;
use std::time::Duration;
use wlr_config::Config;

/// How long the pen holds still before its freehand stroke snaps to a clean shape.
const DEFAULT_DWELL: Duration = Duration::from_millis(650);

/// A modifier, usable as a bindable trigger and matched against the xkb modifier state.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ModKind {
    Ctrl,
    Shift,
    Alt,
    Logo,
    Caps,
}

/// What a binding fires on.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Trigger {
    /// A regular key.
    Key(Keysym),
    /// A modifier held (or, for Caps Lock, latched).
    Mod(ModKind),
    /// A mouse button, by its evdev code.
    Button(u32),
}

/// The left button: it draws, so no binding can take it.
pub const BTN_LEFT: u32 = 0x110;
/// The right button: it moves an element, so no binding can take it.
pub const BTN_RIGHT: u32 = 0x111;

/// The mouse buttons a binding can name, by their evdev name and the `buttonN` name sway's
/// `bindsym` gives them (X11 numbering, where 4 to 7 are the wheel, not buttons).
const BUTTONS: &[(&str, Option<&str>, u32)] = &[
    ("BTN_LEFT", Some("button1"), BTN_LEFT),
    ("BTN_RIGHT", Some("button3"), BTN_RIGHT),
    ("BTN_MIDDLE", Some("button2"), 0x112),
    ("BTN_SIDE", Some("button8"), 0x113),
    ("BTN_EXTRA", Some("button9"), 0x114),
    ("BTN_FORWARD", None, 0x115),
    ("BTN_BACK", None, 0x116),
    ("BTN_TASK", None, 0x117),
];

/// The triggers of a held role (pass-through, constrain, spotlight, snap-invert). Any of
/// them engages it: a modifier while it is active, a key or a button while it is held —
/// or, for pass-through, a key toggles it on each press, like the Caps Lock latch.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Role(Vec<Trigger>);

impl Role {
    fn one(t: Trigger) -> Self {
        Self(vec![t])
    }

    /// Whether `t` is one of this role's triggers.
    pub fn has(&self, t: Trigger) -> bool {
        self.0.contains(&t)
    }

    /// The modifiers among this role's triggers.
    pub fn mods(&self) -> impl Iterator<Item = ModKind> + '_ {
        self.0.iter().filter_map(|t| match t {
            Trigger::Mod(m) => Some(*m),
            Trigger::Key(_) | Trigger::Button(_) => None,
        })
    }

    /// Every trigger's label, for the HUD and the help legend: `Caps / F13`. `None` when
    /// the file gave all its triggers to another role.
    pub fn label(&self) -> Option<String> {
        let labels: Vec<String> = self.0.iter().map(|t| trigger_label(*t)).collect();
        (!labels.is_empty()).then(|| labels.join(" / "))
    }
}

/// A discrete action — fires once on key press. The held roles (pass-through, constrain,
/// spotlight, snap-invert) are not here; they live as the [`Role`] fields of [`Keymap`]
/// because they can be modifiers as well as keys.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Action {
    Pen,
    Rect,
    Mask,
    Arrow,
    Text,
    Move,
    Eraser,
    Palette,
    Undo,
    Redo,
    Visibility,
    Save,
    Help,
    Clear,
    WidthInc,
    WidthDec,
    Freeze,
    Snap,
}

/// Parse one trigger name. Modifier aliases first (`ctrl`, `shift`, `alt`, `super`/`logo`,
/// `caps`/`caps_lock`), then mouse buttons (`button2`, `button8`, `BTN_SIDE`…),
/// otherwise an XKB keysym name; all case-insensitive. `None` for an unknown name, so the
/// loader can warn and keep the default.
pub fn parse_trigger(name: &str) -> Option<Trigger> {
    let n = name.trim();
    if let Some(&(_, _, code)) = BUTTONS.iter().find(|(evdev, x11, _)| {
        evdev.eq_ignore_ascii_case(n) || x11.is_some_and(|x11| x11.eq_ignore_ascii_case(n))
    }) {
        return Some(Trigger::Button(code));
    }
    match n.to_ascii_lowercase().as_str() {
        "ctrl" | "control" => return Some(Trigger::Mod(ModKind::Ctrl)),
        "shift" => return Some(Trigger::Mod(ModKind::Shift)),
        "alt" | "meta" => return Some(Trigger::Mod(ModKind::Alt)),
        "super" | "logo" | "win" => return Some(Trigger::Mod(ModKind::Logo)),
        "caps" | "caps_lock" | "capslock" => return Some(Trigger::Mod(ModKind::Caps)),
        // Friendly aliases for the symbols the legend prettifies, so the displayed label
        // round-trips and users can write the natural symbol instead of the XKB name.
        "+" | "plus" => return Some(Trigger::Key(Keysym::plus)),
        "-" | "minus" => return Some(Trigger::Key(Keysym::minus)),
        "=" => return Some(Trigger::Key(Keysym::equal)),
        "del" => return Some(Trigger::Key(Keysym::Delete)),
        "esc" => return Some(Trigger::Key(Keysym::Escape)),
        _ => {}
    }
    let ks = xkbcommon::xkb::keysym_from_name(n, xkbcommon::xkb::KEYSYM_CASE_INSENSITIVE);
    (ks != Keysym::NoSymbol).then_some(Trigger::Key(ks))
}

/// The display label for a trigger (the key column of the help legend). Modifiers get a
/// short name; keys use libxkbcommon's canonical name, with a few prettied for the HUD;
/// buttons their `buttonN` name, else their evdev one.
pub fn trigger_label(t: Trigger) -> String {
    match t {
        Trigger::Button(code) => match BUTTONS.iter().find(|&&(_, _, c)| c == code) {
            Some((_, Some(x11), _)) => format!("Button{}", &x11["button".len()..]),
            Some((evdev, None, _)) => (*evdev).into(),
            None => format!("{code:#x}"),
        },
        Trigger::Mod(ModKind::Ctrl) => "Ctrl".into(),
        Trigger::Mod(ModKind::Shift) => "Shift".into(),
        Trigger::Mod(ModKind::Alt) => "Alt".into(),
        Trigger::Mod(ModKind::Logo) => "Super".into(),
        Trigger::Mod(ModKind::Caps) => "Caps".into(),
        Trigger::Key(k) => match xkbcommon::xkb::keysym_get_name(k).as_str() {
            "plus" | "KP_Add" => "+".into(),
            "minus" | "KP_Subtract" => "-".into(),
            "equal" => "=".into(),
            "Delete" => "Del".into(),
            "space" | "KP_Space" => "Space".into(),
            "Escape" => "Esc".into(),
            // Single letters/digits get a capital so the column reads uniformly (`P`, not
            // `p`, next to `Del`/`Ctrl`); multi-char names keep libxkbcommon's casing.
            other if other.len() == 1 => other.to_ascii_uppercase(),
            other => other.to_string(),
        },
    }
}

/// The resolved configuration. `keys` maps discrete-action triggers (an action may have
/// several — e.g. `+` and `=`); each held role carries its triggers; the dwell pair tunes
/// what the `snap` binding and its held counterpart act on.
#[derive(Clone)]
pub struct Keymap {
    keys: Vec<(Trigger, Action)>,
    pub passthrough: Role,
    pub constrain: Role,
    pub spotlight: Role,
    /// Held to invert [`Keymap::snap_on_dwell`] for the stroke in progress — suppressing
    /// the snap where it is on, arming it where it is off.
    pub snap_invert: Role,
    /// Whether the pen snaps a held-still stroke to a clean shape without being asked.
    /// The starting state only: [`Action::Snap`] flips it at runtime.
    pub snap_on_dwell: bool,
    /// How long the pen must hold still before that snap fires.
    pub dwell: Duration,
}

impl Default for Keymap {
    /// The default hardcoded layout
    fn default() -> Self {
        let c = |ch: char| Trigger::Key(Keysym::from_char(ch));
        Keymap {
            keys: vec![
                (c('p'), Action::Pen),
                (c('r'), Action::Rect),
                (c('m'), Action::Mask),
                (c('a'), Action::Arrow),
                (c('t'), Action::Text),
                (c('s'), Action::Move),
                (c('e'), Action::Eraser),
                (c('c'), Action::Palette),
                (c('u'), Action::Undo),
                (c('y'), Action::Redo),
                (c('v'), Action::Visibility),
                (c('w'), Action::Save),
                (c('h'), Action::Help),
                (Trigger::Key(Keysym::Delete), Action::Clear),
                (Trigger::Key(Keysym::plus), Action::WidthInc),
                (Trigger::Key(Keysym::equal), Action::WidthInc),
                (Trigger::Key(Keysym::KP_Add), Action::WidthInc),
                (Trigger::Key(Keysym::minus), Action::WidthDec),
                (Trigger::Key(Keysym::KP_Subtract), Action::WidthDec),
                (Trigger::Key(Keysym::space), Action::Freeze),
                (Trigger::Key(Keysym::KP_Space), Action::Freeze),
                (c('d'), Action::Snap),
            ],
            passthrough: Role::one(Trigger::Mod(ModKind::Caps)),
            constrain: Role::one(Trigger::Mod(ModKind::Ctrl)),
            spotlight: Role::one(Trigger::Mod(ModKind::Shift)),
            snap_invert: Role::one(Trigger::Mod(ModKind::Alt)),
            snap_on_dwell: true,
            dwell: DEFAULT_DWELL,
        }
    }
}

/// What was wrong with a setting: the key, as `config.toml` writes it, and why.
type Warning = (String, String);

impl Keymap {
    /// The `[draw]` section of `config`, over the defaults. What cannot be read keeps
    /// its default and becomes one of `config`'s warnings: a wrong binding is otherwise
    /// baffling.
    pub fn from_config(config: &mut Config) -> Self {
        let section: Section = config.section("draw");
        let mut km = Keymap::default();
        for (key, message) in km.apply(section) {
            config.warn_at(&key, message);
        }
        km
    }

    /// The discrete action a key or button press triggers, if any.
    pub fn action_for(&self, trigger: Trigger) -> Option<Action> {
        self.keys
            .iter()
            .find(|(t, _)| *t == trigger)
            .map(|(_, a)| *a)
    }

    /// Whether `trigger` engages a held role.
    pub fn is_role_trigger(&self, trigger: Trigger) -> bool {
        self.roles().into_iter().any(|role| role.has(trigger))
    }

    /// The held roles: pass-through, constrain, spotlight, snap-invert.
    fn roles(&self) -> [&Role; 4] {
        [
            &self.passthrough,
            &self.constrain,
            &self.spotlight,
            &self.snap_invert,
        ]
    }

    fn roles_mut(&mut self) -> [&mut Role; 4] {
        [
            &mut self.passthrough,
            &mut self.constrain,
            &mut self.spotlight,
            &mut self.snap_invert,
        ]
    }

    /// The label for an action's (first) trigger — for the help legend — or `None` when
    /// the file gave all its keys to something else.
    pub fn label_for(&self, action: Action) -> Option<String> {
        self.keys
            .iter()
            .find(|(_, a)| *a == action)
            .map(|(t, _)| trigger_label(*t))
    }

    /// Replace a held role's triggers from a config value (or keep the default if the
    /// value is absent or has no parseable trigger). Returns whether it did.
    fn override_role(
        opt: Option<OneOrMany>,
        role: &mut Role,
        name: &str,
        warnings: &mut Vec<Warning>,
    ) -> bool {
        let Some(o) = opt else { return false };
        let triggers = parse_triggers(o, name, warnings);
        let set = !triggers.is_empty();
        if set {
            *role = Role(triggers);
        }
        set
    }

    /// Lay `section` over this keymap, returning what was wrong with it.
    fn apply(&mut self, section: Section) -> Vec<Warning> {
        let mut warnings = Vec::new();
        let raw = section.keys;
        // The file's action bindings, kept apart from the defaults until the end so a key
        // it names can be taken from whichever action had it by default. A value with no
        // parseable name keeps the action's default.
        let mut file: Vec<(Trigger, Action)> = Vec::new();
        for (value, action, name) in [
            (raw.pen, Action::Pen, "pen"),
            (raw.rect, Action::Rect, "rect"),
            (raw.mask, Action::Mask, "mask"),
            (raw.arrow, Action::Arrow, "arrow"),
            (raw.text, Action::Text, "text"),
            (raw.r#move, Action::Move, "move"),
            (raw.eraser, Action::Eraser, "eraser"),
            (raw.palette, Action::Palette, "palette"),
            (raw.undo, Action::Undo, "undo"),
            (raw.redo, Action::Redo, "redo"),
            (raw.visibility, Action::Visibility, "visibility"),
            (raw.save, Action::Save, "save"),
            (raw.help, Action::Help, "help"),
            (raw.clear, Action::Clear, "clear"),
            (raw.width_inc, Action::WidthInc, "width-inc"),
            (raw.width_dec, Action::WidthDec, "width-dec"),
            (raw.freeze, Action::Freeze, "freeze"),
            (raw.snap, Action::Snap, "snap"),
        ] {
            let Some(value) = value else { continue };
            let triggers = parse_triggers(value, name, &mut warnings);
            if !triggers.is_empty() {
                self.keys.retain(|(_, a)| *a != action);
                file.extend(triggers.into_iter().map(|t| (t, action)));
            }
        }

        if let Some(on) = section.dwell {
            self.snap_on_dwell = on;
        }
        if let Some(ms) = section.dwell_ms {
            // Zero would leave the `snap` binding with nothing to turn back on, so the
            // delay stays a delay and `dwell = false` is the way to switch snapping off.
            if ms == 0 {
                warnings.push(("draw.dwell-ms".into(), tr!("draw-config-dwell-zero")));
            } else {
                self.dwell = Duration::from_millis(ms);
            }
        }

        let from_file = [
            Self::override_role(
                raw.passthrough,
                &mut self.passthrough,
                "passthrough",
                &mut warnings,
            ),
            Self::override_role(
                raw.constrain,
                &mut self.constrain,
                "constrain",
                &mut warnings,
            ),
            Self::override_role(
                raw.spotlight,
                &mut self.spotlight,
                "spotlight",
                &mut warnings,
            ),
            Self::override_role(
                raw.snap_invert,
                &mut self.snap_invert,
                "snap-invert",
                &mut warnings,
            ),
        ];

        // A trigger the file names goes where the file puts it: the action or held role
        // that had it by default gives it up and keeps its other triggers, or none —
        // `passthrough = "alt"` takes Alt from snap-invert. That is the user's choice, not
        // a mistake to report. (The roles' defaults are modifiers, which no default
        // action key is.)
        let file_role_triggers: Vec<Trigger> = self
            .roles()
            .into_iter()
            .zip(from_file)
            .filter(|&(_, from_file)| from_file)
            .flat_map(|(role, _)| role.0.iter().copied())
            .collect();
        for (role, from_file) in self.roles_mut().into_iter().zip(from_file) {
            if !from_file {
                role.0.retain(|t| !file_role_triggers.contains(t));
            }
        }
        let role_triggers: Vec<Trigger> = self
            .roles()
            .into_iter()
            .flat_map(|role| role.0.iter().copied())
            .collect();
        self.keys
            .retain(|(t, _)| !file.iter().any(|(f, _)| f == t) && !role_triggers.contains(t));
        self.keys.extend(file);

        self.conflicts(&mut warnings);
        warnings
    }

    /// The file's own clashes, since the defaults have given up every trigger it names:
    /// one key named for two actions (the first one wins), for a held role and an action
    /// (the role wins), or one trigger for two held roles (both engage). Changes nothing.
    fn conflicts(&self, warnings: &mut Vec<Warning>) {
        let mut warn = |message: String| warnings.push(("draw.keys".into(), message));
        for i in 0..self.keys.len() {
            for j in (i + 1)..self.keys.len() {
                if self.keys[i].0 == self.keys[j].0 && self.keys[i].1 != self.keys[j].1 {
                    let key = trigger_label(self.keys[i].0);
                    warn(tr!("draw-config-two-actions", key = key));
                }
            }
        }
        let roles = self.roles();
        for (i, role) in roles.iter().enumerate() {
            for &t in &role.0 {
                if !matches!(t, Trigger::Mod(_)) && self.keys.iter().any(|(k, _)| *k == t) {
                    warn(tr!("draw-config-role-and-tool", key = trigger_label(t)));
                }
                if roles[i + 1..].iter().any(|other| other.0.contains(&t)) {
                    warn(tr!("draw-config-two-roles", key = trigger_label(t)));
                }
            }
        }
    }
}

/// Parse every name of a binding, reporting (and dropping) the ones that don't.
fn parse_triggers(o: OneOrMany, name: &str, warnings: &mut Vec<Warning>) -> Vec<Trigger> {
    o.into_vec()
        .iter()
        .filter_map(|s| {
            let message = match parse_trigger(s) {
                None => tr!("draw-config-bad-key-name", name = s.as_str()),
                // The left button draws and the right one moves: taking either would
                // leave no way to do that.
                Some(Trigger::Button(BTN_LEFT | BTN_RIGHT)) => {
                    tr!("draw-config-reserved-button", name = s.as_str())
                }
                // Click-through empties the overlay's input region: no click reaches it
                // to end click-through.
                Some(Trigger::Button(_)) if name == "passthrough" => {
                    tr!("draw-config-passthrough-button", name = s.as_str())
                }
                trigger => return trigger,
            };
            warnings.push((format!("draw.keys.{name}"), message));
            None
        })
        .collect()
}

/// A binding value: one trigger name or a list of them.
#[derive(Deserialize)]
#[serde(untagged)]
enum OneOrMany {
    One(String),
    Many(Vec<String>),
}

impl OneOrMany {
    fn into_vec(self) -> Vec<String> {
        match self {
            OneOrMany::One(s) => vec![s],
            OneOrMany::Many(v) => v,
        }
    }
}

/// The `[draw]` section. Every field optional so a partial section is valid; missing keys
/// keep their default. Keys are kebab-case (`dwell-ms`, `width-inc`).
#[derive(Deserialize, Default)]
#[serde(rename_all = "kebab-case", default)]
struct Section {
    dwell: Option<bool>,
    dwell_ms: Option<u64>,
    keys: Bindings,
}

/// `[draw.keys]`: the actions, then the held roles.
#[derive(Deserialize, Default)]
#[serde(rename_all = "kebab-case", default)]
struct Bindings {
    pen: Option<OneOrMany>,
    rect: Option<OneOrMany>,
    mask: Option<OneOrMany>,
    arrow: Option<OneOrMany>,
    text: Option<OneOrMany>,
    r#move: Option<OneOrMany>,
    eraser: Option<OneOrMany>,
    palette: Option<OneOrMany>,
    undo: Option<OneOrMany>,
    redo: Option<OneOrMany>,
    visibility: Option<OneOrMany>,
    save: Option<OneOrMany>,
    help: Option<OneOrMany>,
    clear: Option<OneOrMany>,
    width_inc: Option<OneOrMany>,
    width_dec: Option<OneOrMany>,
    freeze: Option<OneOrMany>,
    snap: Option<OneOrMany>,
    passthrough: Option<OneOrMany>,
    constrain: Option<OneOrMany>,
    spotlight: Option<OneOrMany>,
    snap_invert: Option<OneOrMany>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use wlr_config::Dirs;

    /// Lay a `[draw]` section, as `config.toml` writes it, over the defaults.
    fn apply(s: &str) -> (Keymap, Vec<Warning>) {
        let mut km = Keymap::default();
        let warnings = km.apply(toml::from_str(s).unwrap());
        (km, warnings)
    }

    /// The configuration of a home holding `files` (path from the home, content).
    fn load(files: &[(&str, &str)]) -> (Config, tempfile::TempDir) {
        let tmp = tempfile::tempdir().unwrap();
        for (path, text) in files {
            let file = tmp.path().join(path);
            std::fs::create_dir_all(file.parent().unwrap()).unwrap();
            std::fs::write(file, text).unwrap();
        }
        let dirs = Dirs {
            home: Some(tmp.path().into()),
            config_home: Some(tmp.path().join(".config")),
            data_dirs: vec![tmp.path().join("share")],
            ..Dirs::default()
        };
        (Config::load(&dirs), tmp)
    }

    #[test]
    fn parse_names_and_modifiers() {
        assert_eq!(
            parse_trigger("p"),
            Some(Trigger::Key(Keysym::from_char('p')))
        );
        assert_eq!(parse_trigger("space"), Some(Trigger::Key(Keysym::space)));
        assert_eq!(
            parse_trigger("Caps_Lock"),
            Some(Trigger::Mod(ModKind::Caps))
        );
        assert_eq!(parse_trigger("CTRL"), Some(Trigger::Mod(ModKind::Ctrl)));
        assert_eq!(parse_trigger("super"), Some(Trigger::Mod(ModKind::Logo)));
        assert_eq!(parse_trigger("wobble"), None);
    }

    #[test]
    fn label_round_trips_through_parse() {
        for name in [
            "p",
            "space",
            "plus",
            "Delete",
            "F5",
            "button8",
            "BTN_FORWARD",
        ] {
            let t = parse_trigger(name).unwrap();
            // The label re-parses to the same trigger (prettied labels included).
            assert_eq!(parse_trigger(&trigger_label(t)).unwrap(), t, "{name}");
        }
    }

    #[test]
    fn default_keymap_matches_legacy_layout() {
        let km = Keymap::default();
        assert_eq!(
            km.action_for(Trigger::Key(Keysym::from_char('p'))),
            Some(Action::Pen)
        );
        assert_eq!(
            km.action_for(Trigger::Key(Keysym::from_char('w'))),
            Some(Action::Save)
        );
        assert_eq!(
            km.action_for(Trigger::Key(Keysym::plus)),
            Some(Action::WidthInc)
        );
        assert_eq!(
            km.action_for(Trigger::Key(Keysym::equal)),
            Some(Action::WidthInc)
        );
        assert_eq!(
            km.action_for(Trigger::Key(Keysym::space)),
            Some(Action::Freeze)
        );
        assert_eq!(
            km.action_for(Trigger::Key(Keysym::from_char('d'))),
            Some(Action::Snap)
        );
        assert_eq!(km.action_for(Trigger::Key(Keysym::from_char('z'))), None);
        assert_eq!(km.passthrough, Role::one(Trigger::Mod(ModKind::Caps)));
        assert_eq!(km.constrain, Role::one(Trigger::Mod(ModKind::Ctrl)));
        assert_eq!(km.spotlight, Role::one(Trigger::Mod(ModKind::Shift)));
        assert_eq!(km.snap_invert, Role::one(Trigger::Mod(ModKind::Alt)));
        assert!(km.snap_on_dwell);
        assert_eq!(km.dwell, Duration::from_millis(650));
    }

    #[test]
    fn override_replaces_and_keeps_defaults() {
        let (km, warnings) = apply(
            r#"
            [keys]
            pen = "b"
            passthrough = "alt"
            width-inc = ["plus", "equal"]
        "#,
        );
        assert!(warnings.is_empty(), "{warnings:?}");
        // pen moved to 'b', the old 'p' is freed.
        assert_eq!(
            km.action_for(Trigger::Key(Keysym::from_char('b'))),
            Some(Action::Pen)
        );
        assert_eq!(km.action_for(Trigger::Key(Keysym::from_char('p'))), None);
        // role rebound, others untouched.
        assert_eq!(km.passthrough, Role::one(Trigger::Mod(ModKind::Alt)));
        assert_eq!(km.constrain, Role::one(Trigger::Mod(ModKind::Ctrl)));
        assert_eq!(
            km.action_for(Trigger::Key(Keysym::from_char('w'))),
            Some(Action::Save)
        );
    }

    /// Both dwell settings, and the `snap` binding they belong to, override cleanly.
    #[test]
    fn dwell_settings_override() {
        let (km, _) = apply(
            r#"
            dwell = false
            dwell-ms = 1200

            [keys]
            snap = "n"
            snap-invert = "super"
        "#,
        );
        assert_eq!(
            km.action_for(Trigger::Key(Keysym::from_char('n'))),
            Some(Action::Snap)
        );
        assert_eq!(km.action_for(Trigger::Key(Keysym::from_char('d'))), None);
        assert!(!km.snap_on_dwell);
        assert_eq!(km.dwell, Duration::from_millis(1200));
        assert_eq!(km.snap_invert, Role::one(Trigger::Mod(ModKind::Logo)));
    }

    /// A key name that does not exist is named, with the binding that holds it; the
    /// binding keeps its default, and the rest of the section applies.
    #[test]
    fn an_unknown_key_name_is_named() {
        let (km, warnings) = apply("[keys]\nundo = \"wobble\"\npen = \"b\"");
        assert_eq!(
            km.action_for(Trigger::Key(Keysym::from_char('u'))),
            Some(Action::Undo)
        );
        assert_eq!(
            km.action_for(Trigger::Key(Keysym::from_char('b'))),
            Some(Action::Pen)
        );
        assert!(
            matches!(&warnings[..], [(key, message)] if key == "draw.keys.undo"
                && message.contains("wobble")),
            "{warnings:?}"
        );
    }

    /// A held role takes a list like an action does, and the rest of the section still
    /// applies: a list there once failed the whole file.
    #[test]
    fn a_role_takes_a_list() {
        let (km, warnings) = apply(
            r#"
            [keys]
            passthrough = ["caps", "F13", "wobble"]
            pen = "b"
        "#,
        );
        assert_eq!(
            km.passthrough,
            Role(vec![Trigger::Mod(ModKind::Caps), Trigger::Key(Keysym::F13)])
        );
        assert!(km.is_role_trigger(Trigger::Key(Keysym::F13)));
        assert_eq!(km.passthrough.label().as_deref(), Some("Caps / F13"));
        assert_eq!(
            km.action_for(Trigger::Key(Keysym::from_char('b'))),
            Some(Action::Pen)
        );
        assert!(
            matches!(&warnings[..], [(key, _)] if key == "draw.keys.passthrough"),
            "{warnings:?}"
        );
    }

    /// The file's bindings win over the defaults: a key it names is taken from the action
    /// that had it, which is left with its other keys or none.
    #[test]
    fn the_file_takes_a_key_from_a_default() {
        let (km, warnings) = apply("[keys]\npen = \"r\"");
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(
            km.action_for(Trigger::Key(Keysym::from_char('r'))),
            Some(Action::Pen)
        );
        assert_eq!(km.action_for(Trigger::Key(Keysym::from_char('p'))), None);
        assert_eq!(km.label_for(Action::Rect), None);

        // An action with several keys keeps the others.
        let (km, _) = apply("[keys]\nundo = \"plus\"");
        assert_eq!(
            km.action_for(Trigger::Key(Keysym::plus)),
            Some(Action::Undo)
        );
        assert_eq!(
            km.action_for(Trigger::Key(Keysym::equal)),
            Some(Action::WidthInc)
        );

        // A held role takes a key the same way.
        let (km, _) = apply("[keys]\nspotlight = \"s\"");
        assert!(km.is_role_trigger(Trigger::Key(Keysym::from_char('s'))));
        assert_eq!(km.label_for(Action::Move), None);
    }

    /// The same for the held roles' triggers: the advice for keyboards without a usable
    /// Caps Lock, `passthrough = "alt"`, takes Alt from snap-invert.
    #[test]
    fn the_file_takes_a_trigger_from_a_default_role() {
        let (km, warnings) = apply("[keys]\npassthrough = \"alt\"");
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(km.passthrough, Role::one(Trigger::Mod(ModKind::Alt)));
        assert_eq!(km.snap_invert.label(), None);

        // A role the file sets itself keeps what it names.
        let (km, _) = apply("[keys]\npassthrough = \"alt\"\nsnap-invert = \"super\"");
        assert_eq!(km.snap_invert, Role::one(Trigger::Mod(ModKind::Logo)));
    }

    /// Mouse buttons go by sway's `buttonN` names or their evdev ones, in any case; 4 to
    /// 7 are the wheel, which is no button.
    #[test]
    fn mouse_buttons_are_named_as_in_sway() {
        assert_eq!(parse_trigger("button2"), Some(Trigger::Button(0x112)));
        assert_eq!(parse_trigger("BTN_MIDDLE"), Some(Trigger::Button(0x112)));
        assert_eq!(parse_trigger("Button8"), Some(Trigger::Button(0x113)));
        assert_eq!(parse_trigger("btn_side"), Some(Trigger::Button(0x113)));
        assert_eq!(parse_trigger("button9"), Some(Trigger::Button(0x114)));
        assert_eq!(parse_trigger("BTN_EXTRA"), Some(Trigger::Button(0x114)));
        assert_eq!(parse_trigger("button4"), None);
        assert_eq!(trigger_label(Trigger::Button(0x113)), "Button8");
    }

    /// The side buttons the issue asked for: back undoes, forward redoes.
    #[test]
    fn a_mouse_button_takes_an_action() {
        let (km, warnings) = apply("[keys]\nundo = \"button8\"\nredo = [\"y\", \"button9\"]");
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(km.action_for(Trigger::Button(0x113)), Some(Action::Undo));
        assert_eq!(km.action_for(Trigger::Button(0x114)), Some(Action::Redo));
        assert_eq!(km.label_for(Action::Undo).as_deref(), Some("Button8"));
        // The key undo had is free again, as for any binding.
        assert_eq!(km.action_for(Trigger::Key(Keysym::from_char('u'))), None);
    }

    /// The left button draws and the right one moves: neither can be bound, and the rest
    /// of the binding stands.
    #[test]
    fn the_drawing_buttons_cannot_be_bound() {
        let (km, warnings) = apply("[keys]\npen = [\"button1\", \"q\"]\nundo = \"BTN_RIGHT\"");
        assert_eq!(
            km.action_for(Trigger::Key(Keysym::from_char('q'))),
            Some(Action::Pen)
        );
        assert_eq!(km.action_for(Trigger::Button(BTN_LEFT)), None);
        assert_eq!(km.action_for(Trigger::Button(BTN_RIGHT)), None);
        assert_eq!(
            km.action_for(Trigger::Key(Keysym::from_char('u'))),
            Some(Action::Undo)
        );
        let keys: Vec<&str> = warnings.iter().map(|(k, _)| k.as_str()).collect();
        assert_eq!(keys, ["draw.keys.pen", "draw.keys.undo"], "{warnings:?}");
    }

    /// A held control takes a button, except pass-through: click-through gives the overlay
    /// no clicks, so a button could never end it.
    #[test]
    fn a_held_control_takes_a_button_but_passthrough() {
        let (km, warnings) =
            apply("[keys]\nspotlight = \"button2\"\npassthrough = [\"button9\", \"caps\"]");
        assert!(km.is_role_trigger(Trigger::Button(0x112)));
        assert_eq!(km.spotlight, Role::one(Trigger::Button(0x112)));
        assert_eq!(km.passthrough, Role::one(Trigger::Mod(ModKind::Caps)));
        assert!(
            matches!(&warnings[..], [(key, message)] if key == "draw.keys.passthrough"
                && message.contains("button9")),
            "{warnings:?}"
        );
    }

    /// A button bound both to a held control and to an action is a clash like a key.
    #[test]
    fn a_button_bound_to_a_control_and_an_action_is_named() {
        let (_, warnings) = apply("[keys]\nundo = \"button8\"\nspotlight = \"button8\"");
        assert!(
            matches!(&warnings[..], [(key, message)] if key == "draw.keys"
                && message.contains("Button8")),
            "{warnings:?}"
        );
    }

    /// The file's own clashes are named, against the bindings as a whole.
    #[test]
    fn a_key_bound_twice_by_the_file_is_named() {
        let (_, warnings) = apply("[keys]\npen = \"x\"\nrect = \"x\"");
        assert!(
            matches!(&warnings[..], [(key, message)] if key == "draw.keys"
                && message.contains("`X`")),
            "{warnings:?}"
        );
    }

    /// A section that mentions neither leaves both at their default, and turning
    /// snapping off does not shorten the delay the `snap` binding turns back on.
    #[test]
    fn dwell_settings_are_independent() {
        let (km, _) = apply("[keys]\npen = \"b\"");
        assert!(km.snap_on_dwell);
        assert_eq!(km.dwell, Duration::from_millis(650));

        let (km, _) = apply("dwell = false");
        assert_eq!(km.dwell, Duration::from_millis(650));
    }

    /// `0` is refused, because it would leave the toggle with no delay to restore. The
    /// default stands, and the setting is named.
    #[test]
    fn zero_dwell_ms_keeps_the_default() {
        let (km, warnings) = apply("dwell-ms = 0");
        assert_eq!(km.dwell, Duration::from_millis(650));
        assert!(km.snap_on_dwell);
        assert!(
            matches!(&warnings[..], [(key, _)] if key == "draw.dwell-ms"),
            "{warnings:?}"
        );
    }

    /// A value of the wrong type fails the section: the configuration reports it and
    /// keeps every default, rather than applying half of it.
    #[test]
    fn non_numeric_dwell_ms_is_a_parse_error() {
        assert!(toml::from_str::<Section>(r#"dwell-ms = "slow""#).is_err());
        assert!(toml::from_str::<Section>("dwell = 650").is_err());
    }

    /// A mistake names the file and the key as the user wrote them: in `config.toml`,
    /// and in the old `keys.toml`, where the bindings sit at the top.
    #[test]
    fn a_mistake_names_its_file_and_key() {
        let (mut config, _tmp) = load(&[(
            ".config/wlr-utils/config.toml",
            "[draw.keys]\nundo = \"wobble\"\n",
        )]);
        Keymap::from_config(&mut config);
        assert!(
            config.warnings().iter().any(|w| w.contains("config.toml")
                && w.contains("`draw.keys.undo`")
                && w.contains("wobble")),
            "{:?}",
            config.warnings()
        );

        let (mut config, _tmp) = load(&[(".config/wlr-draw/keys.toml", "undo = \"wobble\"\n")]);
        Keymap::from_config(&mut config);
        assert!(
            config
                .warnings()
                .iter()
                .any(|w| w.contains("keys.toml") && w.contains("`undo`") && w.contains("wobble")),
            "{:?}",
            config.warnings()
        );
    }

    /// The example `config.toml` binds every action and held control, each to its
    /// default.
    #[test]
    fn the_example_holds_the_defaults() {
        let example = include_str!("../../../docs/config.toml");
        let section: toml::Table = example.parse::<toml::Table>().unwrap()["draw"]
            .as_table()
            .unwrap()
            .clone();
        let bindings: Bindings = section["keys"].clone().try_into().unwrap();
        assert_eq!(unset(&bindings), Vec::<&str>::new());

        let (mut config, _tmp) = load(&[(".config/wlr-utils/config.toml", example)]);
        let km = Keymap::from_config(&mut config);
        assert!(config.warnings().is_empty(), "{:?}", config.warnings());
        let default = Keymap::default();
        assert_eq!(km.keys.len(), default.keys.len());
        for binding in &default.keys {
            assert!(km.keys.contains(binding), "{binding:?}");
        }
        assert_eq!(km.roles(), default.roles());
        assert_eq!(km.snap_on_dwell, default.snap_on_dwell);
        assert_eq!(km.dwell, default.dwell);
    }

    /// The bindings a section leaves out.
    fn unset(b: &Bindings) -> Vec<&'static str> {
        [
            ("pen", b.pen.is_none()),
            ("rect", b.rect.is_none()),
            ("mask", b.mask.is_none()),
            ("arrow", b.arrow.is_none()),
            ("text", b.text.is_none()),
            ("move", b.r#move.is_none()),
            ("eraser", b.eraser.is_none()),
            ("palette", b.palette.is_none()),
            ("undo", b.undo.is_none()),
            ("redo", b.redo.is_none()),
            ("visibility", b.visibility.is_none()),
            ("save", b.save.is_none()),
            ("help", b.help.is_none()),
            ("clear", b.clear.is_none()),
            ("width-inc", b.width_inc.is_none()),
            ("width-dec", b.width_dec.is_none()),
            ("freeze", b.freeze.is_none()),
            ("snap", b.snap.is_none()),
            ("passthrough", b.passthrough.is_none()),
            ("constrain", b.constrain.is_none()),
            ("spotlight", b.spotlight.is_none()),
            ("snap-invert", b.snap_invert.is_none()),
        ]
        .into_iter()
        .filter(|&(_, unset)| unset)
        .map(|(key, _)| key)
        .collect()
    }
}
