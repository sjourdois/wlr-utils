//! Focus-aware capture helpers: "the active window" and "the current output".
//!
//! Wayland deliberately gives a regular client no way to query the global pointer
//! position or which surface/output has the focus — so, like `grimshot`, we rely
//! on the compositor's own IPC. This is a small trait with per-compositor backends
//! selected from the environment; Sway is implemented today, Hyprland / niri are
//! natural additions.

use crate::wl::Region;

/// A compositor-specific source of focus information.
pub trait FocusBackend {
    /// Name of the focused output, if any.
    fn focused_output(&self) -> Option<String>;
    /// Logical rectangle of the active (focused) window, if any.
    fn active_window_rect(&self) -> Option<Region>;
    /// Human-readable backend name, for error messages.
    fn name(&self) -> &'static str;
}

/// Pick a focus backend from the environment. `None` if no supported compositor
/// IPC is present (Wayland has no portable fallback — see the module docs).
pub fn detect() -> Option<Box<dyn FocusBackend>> {
    if std::env::var_os("SWAYSOCK").is_some() {
        return Some(Box::new(Sway));
    }
    // Future modules, keyed off their well-known env vars:
    //   HYPRLAND_INSTANCE_SIGNATURE -> Hyprland (`hyprctl -j`)
    //   NIRI_SOCKET                 -> niri (`niri msg --json`)
    None
}

/// Sway / wlroots `swaymsg` backend.
struct Sway;

impl Sway {
    fn query(kind: &str) -> Option<serde_json::Value> {
        let out = std::process::Command::new("swaymsg")
            .args(["-t", kind, "-r"])
            .output()
            .ok()?;
        out.status.success().then_some(())?;
        serde_json::from_slice(&out.stdout).ok()
    }
}

impl FocusBackend for Sway {
    fn name(&self) -> &'static str {
        "sway"
    }

    fn focused_output(&self) -> Option<String> {
        let outputs = Self::query("get_outputs")?;
        outputs
            .as_array()?
            .iter()
            .find(|o| o["focused"].as_bool() == Some(true))?["name"]
            .as_str()
            .map(String::from)
    }

    fn active_window_rect(&self) -> Option<Region> {
        let tree = Self::query("get_tree")?;
        let node = find_focused(&tree)?;
        // Only windows have an app_id / window properties; a focused empty
        // workspace is not an "active window".
        let is_window = node.get("app_id").is_some_and(|a| !a.is_null())
            || node.get("window_properties").is_some()
            || (matches!(
                node.get("type").and_then(|t| t.as_str()),
                Some("con") | Some("floating_con")
            ) && node.get("name").is_some_and(|n| !n.is_null()));
        if !is_window {
            return None;
        }
        rect_of(node)
    }
}

/// The single node with `"focused": true` in a sway tree (the active container).
fn find_focused(node: &serde_json::Value) -> Option<&serde_json::Value> {
    if node.get("focused").and_then(|f| f.as_bool()) == Some(true) {
        return Some(node);
    }
    for key in ["nodes", "floating_nodes"] {
        if let Some(children) = node.get(key).and_then(|c| c.as_array()) {
            for child in children {
                if let Some(found) = find_focused(child) {
                    return Some(found);
                }
            }
        }
    }
    None
}

/// Read a sway `rect` object into a logical [`Region`].
fn rect_of(node: &serde_json::Value) -> Option<Region> {
    let r = node.get("rect")?;
    Some(Region {
        x: r["x"].as_i64()? as i32,
        y: r["y"].as_i64()? as i32,
        w: r["width"].as_u64()? as u32,
        h: r["height"].as_u64()? as u32,
    })
}
