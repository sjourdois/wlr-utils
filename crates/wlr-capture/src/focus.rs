//! Focus-aware capture helpers: "the active window" and "the current output".
//!
//! Wayland deliberately gives a regular client no way to query the global pointer
//! position or which surface/output has the focus — so, like `grimshot`, we rely
//! on the compositor's own IPC. This is a small trait with per-compositor backends
//! selected from the environment: Sway (`$SWAYSOCK`), Hyprland (`hyprctl`) and niri
//! (`niri msg`).

use crate::wl::Region;
use swayipc::{Connection, Fallible, Node, NodeType};

/// A window's identity + content geometry, for binding a region mirror to the window
/// under it (`app_id` + `title` match a `wl::Toplevel`; `rect` is its content area).
pub struct WindowRef {
    /// The window's application id (matches a [`wl::Toplevel`](crate::wl::Toplevel)).
    pub app_id: String,
    /// The window title (matches a [`wl::Toplevel`](crate::wl::Toplevel)).
    pub title: String,
    /// The window's content area in the global logical space.
    pub rect: Region,
}

/// The windows, most recently focused first, by [`crate::wl::Toplevel::identifier`].
#[derive(Default)]
pub struct FocusOrder {
    /// The window with the focus right now, if any.
    pub focused: Option<String>,
    /// The other windows, most recently focused first.
    pub unfocused: Vec<String>,
}

impl FocusOrder {
    /// All windows, most recently focused first.
    pub fn windows(&self) -> impl Iterator<Item = &str> {
        self.focused
            .iter()
            .chain(&self.unfocused)
            .map(String::as_str)
    }
}

/// A compositor-specific source of focus information.
pub trait FocusBackend {
    /// Name of the focused output, if any.
    fn focused_output(&self) -> Option<String>;
    /// Logical rectangle of the active (focused) window, if any.
    fn active_window_rect(&self) -> Option<Region>;
    /// The window under the given global logical point, if any. Used to make a region
    /// mirror follow the window beneath it. Default `None` (only Sway implements it).
    fn window_at(&self, _x: i32, _y: i32) -> Option<WindowRef> {
        None
    }
    /// The compositor's window focus history.
    fn focus_order(&self) -> Option<FocusOrder> {
        None
    }
    /// Human-readable backend name, for error messages.
    fn name(&self) -> &'static str;
}

/// Pick a focus backend from the environment. `None` if no supported compositor
/// IPC is present (Wayland has no portable fallback — see the module docs).
pub fn detect() -> Option<Box<dyn FocusBackend>> {
    if std::env::var_os("SWAYSOCK").is_some() {
        return Some(Box::new(Sway));
    }
    if std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE").is_some() {
        return Some(Box::new(Hyprland));
    }
    if std::env::var_os("NIRI_SOCKET").is_some() {
        return Some(Box::new(Niri));
    }
    None
}

/// Sway / wlroots backend, over sway's own IPC socket.
struct Sway;

impl Sway {
    fn with_connection<T>(f: impl FnOnce(&mut Connection) -> Fallible<T>) -> Option<T> {
        Connection::new().and_then(|mut c| f(&mut c)).ok()
    }

    fn tree() -> Option<Node> {
        Self::with_connection(|c| c.get_tree())
    }
}

impl FocusBackend for Sway {
    fn name(&self) -> &'static str {
        "sway"
    }

    fn focused_output(&self) -> Option<String> {
        Self::with_connection(|c| c.get_outputs())?
            .into_iter()
            .find(|o| o.focused)
            .map(|o| o.name)
    }

    fn active_window_rect(&self) -> Option<Region> {
        let tree = Self::tree()?;
        let node = find_focused(&tree)?;
        // Only windows have an app_id / window properties; a focused empty
        // workspace is not an "active window".
        let is_window = node.app_id.is_some()
            || node.window_properties.is_some()
            || (matches!(node.node_type, NodeType::Con | NodeType::FloatingCon)
                && node.name.is_some());
        if !is_window {
            return None;
        }
        Some(rect_of(node))
    }

    fn window_at(&self, x: i32, y: i32) -> Option<WindowRef> {
        sway_window_at(&Self::tree()?, x, y)
    }

    fn focus_order(&self) -> Option<FocusOrder> {
        Some(sway_focus_order(&Self::tree()?))
    }
}

fn children(node: &Node) -> impl Iterator<Item = &Node> {
    node.nodes.iter().chain(node.floating_nodes.iter())
}

/// Whether a sway node is a window (vs a container/workspace/output).
fn sway_is_window(node: &Node) -> bool {
    node.app_id.is_some() || node.window_properties.is_some()
}

/// A node's content rectangle in global logical coordinates: its `rect` shifted by the
/// `window_rect` (content offset within the node), so the crop lines up with what the
/// foreign-toplevel capture actually contains (no server-side borders).
fn sway_content_rect(node: &Node) -> Region {
    let rect = rect_of(node);
    // Sway sends a zeroed `window_rect` for nodes with no content offset.
    let wr = &node.window_rect;
    if wr.width > 0 && wr.height > 0 {
        return Region {
            x: rect.x + wr.x,
            y: rect.y + wr.y,
            w: wr.width as u32,
            h: wr.height as u32,
        };
    }
    rect
}

/// The deepest window node whose `rect` contains the global logical point `(x, y)`.
fn sway_window_at(node: &Node, x: i32, y: i32) -> Option<WindowRef> {
    // Skip anything not actually on screen — sway keeps the geometry of windows on
    // hidden workspaces (and tabbed/stacked-behind windows) in the tree, so without
    // this we'd match a window the point only "contains" on a workspace you can't see.
    if node.visible == Some(false) {
        return None;
    }
    // Descend into children first so the innermost (leaf) window wins.
    for child in node.floating_nodes.iter().chain(node.nodes.iter()) {
        if contains(&rect_of(child), x, y)
            && let Some(found) = sway_window_at(child, x, y)
        {
            return Some(found);
        }
    }
    if sway_is_window(node) && contains(&rect_of(node), x, y) {
        let app_id = node
            .app_id
            .as_deref()
            .or_else(|| {
                node.window_properties
                    .as_ref()
                    .and_then(|w| w.class.as_deref())
            })
            .unwrap_or_default()
            .to_string();
        return Some(WindowRef {
            app_id,
            title: node.name.clone().unwrap_or_default(),
            rect: sway_content_rect(node),
        });
    }
    None
}

/// Whether `(x, y)` falls inside a logical region.
fn contains(r: &Region, x: i32, y: i32) -> bool {
    x >= r.x && x < r.x + r.w as i32 && y >= r.y && y < r.y + r.h as i32
}

/// The single node with `focused: true` in a sway tree (the active container).
fn find_focused(node: &Node) -> Option<&Node> {
    if node.focused {
        return Some(node);
    }
    children(node).find_map(find_focused)
}

fn sway_focus_order(root: &Node) -> FocusOrder {
    let mut windows = Vec::new();
    collect_mru_nodes(root, &mut windows);
    let has_focus = windows.first().is_some_and(|w| w.focused);
    let mut ids = windows
        .iter()
        .filter_map(|w| w.foreign_toplevel_identifier.clone());
    FocusOrder {
        focused: if has_focus { ids.next() } else { None },
        unfocused: ids.collect(),
    }
}

fn collect_mru_nodes<'a>(node: &'a Node, out: &mut Vec<&'a Node>) {
    if node.foreign_toplevel_identifier.is_some() {
        out.push(node);
    }
    // `focus` ranks tiled and floating children alike, most recently focused first.
    let mut children: Vec<&Node> = children(node).collect();
    children.sort_by_key(|c| {
        node.focus
            .iter()
            .position(|&id| id == c.id)
            .unwrap_or(usize::MAX)
    });
    for child in children {
        collect_mru_nodes(child, out);
    }
}

/// Read a sway `rect` into a logical [`Region`].
fn rect_of(node: &Node) -> Region {
    Region {
        x: node.rect.x,
        y: node.rect.y,
        w: node.rect.width.max(0) as u32,
        h: node.rect.height.max(0) as u32,
    }
}

/// Hyprland `hyprctl -j` backend.
struct Hyprland;

impl Hyprland {
    fn query(cmd: &str) -> Option<serde_json::Value> {
        let out = std::process::Command::new("hyprctl")
            .args(["-j", cmd])
            .output()
            .ok()?;
        out.status.success().then_some(())?;
        serde_json::from_slice(&out.stdout).ok()
    }
}

impl FocusBackend for Hyprland {
    fn name(&self) -> &'static str {
        "Hyprland"
    }

    fn focused_output(&self) -> Option<String> {
        hypr_focused_output(&Self::query("monitors")?)
    }

    fn active_window_rect(&self) -> Option<Region> {
        hypr_active_window_rect(&Self::query("activewindow")?)
    }

    fn focus_order(&self) -> Option<FocusOrder> {
        // A missing `activewindow` only costs the `focused` field, so it must not
        // sink the whole history.
        let active = Self::query("activewindow").unwrap_or_default();
        hypr_focus_order(&Self::query("clients")?, &active)
    }
}

/// Pick the focused monitor's name from `hyprctl -j monitors` (an array of monitors,
/// one with `"focused": true`).
fn hypr_focused_output(monitors: &serde_json::Value) -> Option<String> {
    monitors
        .as_array()?
        .iter()
        .find(|m| m["focused"].as_bool() == Some(true))?
        .get("name")?
        .as_str()
        .map(String::from)
}

/// Read the active window's rectangle from `hyprctl -j activewindow`: `at: [x, y]`
/// and `size: [w, h]` in global logical coordinates. An empty object (`{}`) — nothing
/// focused — yields `None`.
fn hypr_active_window_rect(w: &serde_json::Value) -> Option<Region> {
    let at = w.get("at")?.as_array()?;
    let size = w.get("size")?.as_array()?;
    Some(Region {
        x: at.first()?.as_i64()? as i32,
        y: at.get(1)?.as_i64()? as i32,
        w: size.first()?.as_i64()? as u32,
        h: size.get(1)?.as_i64()? as u32,
    })
}

/// Rank the windows of `hyprctl -j clients` by Hyprland's own focus history.
///
/// `focusHistoryID` ranks every mapped window, `0` being the one focused most
/// recently, and `stableId` is exactly the `ext-foreign-toplevel-list-v1`
/// identifier — so, unlike the announcement order of that list, it also tells
/// apart two windows sharing an app id and a title.
///
/// `active` is `hyprctl -j activewindow`, `{}` when nothing holds the focus: the
/// head of the history counts as focused only when it is that window, since the
/// history keeps ranking windows after the focus has left them all.
fn hypr_focus_order(clients: &serde_json::Value, active: &serde_json::Value) -> Option<FocusOrder> {
    let mut windows: Vec<(u64, &str, Option<&str>)> = clients
        .as_array()?
        .iter()
        .filter_map(|c| {
            Some((
                c.get("focusHistoryID")?.as_u64()?,
                c.get("stableId")?.as_str()?,
                c.get("address").and_then(serde_json::Value::as_str),
            ))
        })
        .collect();
    windows.sort_by_key(|&(rank, ..)| rank);
    let active_address = active.get("address").and_then(serde_json::Value::as_str);
    let has_focus = windows
        .first()
        .is_some_and(|&(_, _, address)| address.is_some() && address == active_address);
    let mut ids = windows.iter().map(|&(_, id, _)| id.to_string());
    Some(FocusOrder {
        focused: if has_focus { ids.next() } else { None },
        unfocused: ids.collect(),
    })
}

/// niri `niri msg --json` backend.
struct Niri;

impl Niri {
    fn query(action: &str) -> Option<serde_json::Value> {
        let out = std::process::Command::new("niri")
            .args(["msg", "--json", action])
            .output()
            .ok()?;
        out.status.success().then_some(())?;
        serde_json::from_slice(&out.stdout).ok()
    }
}

impl FocusBackend for Niri {
    fn name(&self) -> &'static str {
        "niri"
    }

    fn focused_output(&self) -> Option<String> {
        niri_focused_output(&Self::query("focused-output")?)
    }

    fn active_window_rect(&self) -> Option<Region> {
        // niri's IPC does not expose a window's rectangle in global logical
        // coordinates (scrollable tiling lets windows extend off-screen), so the
        // active-window source is unavailable — callers get a clear error and can
        // use `--current-output` or `-g` instead.
        None
    }

    fn focus_order(&self) -> Option<FocusOrder> {
        niri_focus_order(&Self::query("windows")?)
    }
}

/// Pick the focused output's name from `niri msg --json focused-output` (the Output
/// object, or `null` when none).
fn niri_focused_output(o: &serde_json::Value) -> Option<String> {
    o.get("name")?.as_str().map(String::from)
}

/// One entry of `niri msg --json windows`, reduced to what ranks it.
struct NiriWindow {
    /// `focus_timestamp`, as a `(secs, nanos)` pair; `None` when the window has
    /// never been focused since the compositor started.
    stamp: Option<(u64, u64)>,
    /// The window `id`, whose decimal form is the ext-foreign-toplevel identifier.
    id: u64,
    focused: bool,
}

/// Rank the windows of `niri msg --json windows` by niri's own focus history.
///
/// `focus_timestamp` is an object (`{"secs": …, "nanos": …}`, monotonic since the
/// compositor started), not a scalar, so it is compared as a pair; a window never
/// focused since then simply has none, and goes after every window that has one.
/// The window `id`, in decimal, is the `ext-foreign-toplevel-list-v1` identifier —
/// niri announces that list in neither creation nor focus order, so correlating by
/// position would be wrong.
fn niri_focus_order(windows: &serde_json::Value) -> Option<FocusOrder> {
    let mut windows: Vec<NiriWindow> = windows
        .as_array()?
        .iter()
        .filter_map(|w| {
            Some(NiriWindow {
                stamp: w
                    .get("focus_timestamp")
                    .and_then(|t| Some((t.get("secs")?.as_u64()?, t.get("nanos")?.as_u64()?))),
                id: w.get("id")?.as_u64()?,
                focused: w.get("is_focused")?.as_bool()?,
            })
        })
        .collect();
    // `Reverse` on the `Option` gives both halves at once: newest first, and the
    // windows with no timestamp at the end.
    windows.sort_by_key(|w| std::cmp::Reverse(w.stamp));
    let has_focus = windows.first().is_some_and(|w| w.focused);
    let mut ids = windows.iter().map(|w| w.id.to_string());
    Some(FocusOrder {
        focused: if has_focus { ids.next() } else { None },
        unfocused: ids.collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};

    fn rect(x: i64, y: i64, width: i64, height: i64) -> Value {
        json!({"x": x, "y": y, "width": width, "height": height})
    }

    // A trimmed but faithful `hyprctl -j monitors` sample (two monitors, the second
    // focused) — locks the field names (`focused`, `name`) the parser relies on.
    const HYPR_MONITORS: &str = r#"[
        {"id":0,"name":"DP-1","make":"Dell","model":"X","width":2560,"height":1440,
         "x":0,"y":0,"refreshRate":59.95,"scale":1.0,"focused":false},
        {"id":1,"name":"HDMI-A-1","make":"LG","model":"Y","width":1920,"height":1080,
         "x":2560,"y":0,"refreshRate":60.0,"scale":1.0,"focused":true}
    ]"#;

    // `hyprctl -j activewindow` gives `at`/`size` pairs in global logical coords.
    const HYPR_ACTIVEWINDOW: &str =
        r#"{"address":"0x55","class":"foot","title":"foot","at":[120,340],"size":[800,600]}"#;

    // A trimmed `hyprctl -j clients`, from a Hyprland 0.56.2 run where five windows
    // were focused in the order A, C, B, D, B — so `focusHistoryID` ranks them B, D,
    // C, A, then the one opened last and never focused again. The array is in
    // creation order, not rank order, and the last two windows here share an app id
    // and a title: only `stableId` tells them apart.
    const HYPR_CLIENTS: &str = r#"[
        {"address":"0x557fa45bb650","class":"foot","title":"WIN-A","mapped":true,
         "focusHistoryID":3,"stableId":"18000002"},
        {"address":"0x557fa6decc90","class":"foot","title":"WIN-B","mapped":true,
         "focusHistoryID":0,"stableId":"18000003"},
        {"address":"0x557fa70d91b0","class":"foot","title":"WIN-C","mapped":true,
         "focusHistoryID":2,"stableId":"18000004"},
        {"address":"0x557fa6e6efc0","class":"foot","title":"twin","mapped":true,
         "focusHistoryID":1,"stableId":"18000005"},
        {"address":"0x557fa739a510","class":"foot","title":"twin","mapped":true,
         "focusHistoryID":4,"stableId":"18000006"}
    ]"#;

    // The `activewindow` that goes with `HYPR_CLIENTS`: the head of the history.
    const HYPR_ACTIVE_OF_CLIENTS: &str = r#"{"address":"0x557fa6decc90","class":"foot","title":"WIN-B","at":[646,21],
            "size":[613,333]}"#;

    // A trimmed `niri msg --json windows`, from a niri 26.04 run with the same focus
    // sequence: B focused, then D, C, A by `focus_timestamp`, and E last. The array
    // order is niri's own and matches neither creation nor focus order.
    const NIRI_WINDOWS: &str = r#"[
        {"id":5,"title":"WIN-D","app_id":"foot","is_focused":false,
         "focus_timestamp":{"secs":352,"nanos":526156848}},
        {"id":6,"title":"WIN-E","app_id":"foot","is_focused":false,
         "focus_timestamp":{"secs":346,"nanos":526223482}},
        {"id":2,"title":"WIN-A","app_id":"foot","is_focused":false,
         "focus_timestamp":{"secs":349,"nanos":732056369}},
        {"id":4,"title":"WIN-C","app_id":"foot","is_focused":false,
         "focus_timestamp":{"secs":350,"nanos":660808585}},
        {"id":3,"title":"WIN-B","app_id":"foot","is_focused":true,
         "focus_timestamp":{"secs":353,"nanos":459040460}}
    ]"#;

    /// A trimmed sway `get_tree`: one output with a visible workspace and a hidden one.
    fn sway_tree() -> Node {
        /// A sway tree node: `fields` over the ones sway always sends but no test here
        /// is about, so the fixture carries only what it is testing.
        fn node(fields: Value) -> Value {
            let zero = rect(0, 0, 0, 0);
            let mut v = json!({
                "id": 0,
                "type": "con",
                "border": "none",
                "current_border_width": 0,
                "layout": "none",
                "orientation": "none",
                "rect": zero,
                "window_rect": zero,
                "deco_rect": zero,
                "geometry": zero,
                "urgent": false,
                "focused": false,
                "focus": [],
                "sticky": false,
                "floating_nodes": [],
            });
            let obj = v.as_object_mut().expect("a node is a JSON object");
            for (key, value) in fields.as_object().expect("a node is a JSON object") {
                obj.insert(key.clone(), value.clone());
            }
            v
        }

        let screen = rect(0, 0, 3840, 1440);
        let v = node(json!({
            "type": "root",
            "rect": screen,
            "nodes": [node(json!({
                "type": "output", "name": "DP-4",
                "rect": screen,
                "nodes": [
                    node(json!({
                        "type": "workspace", "name": "1", "visible": true,
                        "rect": screen,
                        // Firefox was focused more recently, against the tree's order.
                        "focus": [11, 12],
                        "nodes": [
                            node(json!({
                                "id": 12, "app_id": "foot", "name": "term", "visible": true,
                                "foreign_toplevel_identifier": "ext-foot",
                                "rect": rect(1000, 100, 800, 600),
                            })),
                            node(json!({
                                "id": 11, "app_id": "firefox", "name": "Page Title",
                                "visible": true, "focused": true,
                                "foreign_toplevel_identifier": "ext-firefox",
                                "rect": rect(100, 100, 800, 600),
                                // A 20px title bar, so the content rect is not the node's.
                                "window_rect": rect(0, 20, 800, 580),
                            })),
                        ],
                    })),
                    node(json!({
                        "type": "workspace", "name": "2", "visible": false,
                        "rect": screen,
                        // Covers the same coordinates as firefox — sway keeps the
                        // geometry of a window even while its workspace is off screen.
                        "nodes": [node(json!({
                            "app_id": "vim", "name": "editor", "visible": false,
                            "foreign_toplevel_identifier": "ext-vim",
                            "rect": rect(100, 100, 800, 600),
                        }))],
                    })),
                ],
            }))],
        }));
        serde_json::from_value(v).expect("fixture should be a valid sway node")
    }

    #[test]
    fn sway_window_at_finds_visible_window_and_content_rect() {
        let v = sway_tree();
        let w = sway_window_at(&v, 200, 200).expect("window under the point");
        // The visible window wins, never the one on the hidden workspace.
        assert_eq!(w.app_id, "firefox");
        assert_eq!(w.title, "Page Title");
        // Content rect = node rect shifted by the 20px title bar.
        assert_eq!(
            w.rect,
            Region {
                x: 100,
                y: 120,
                w: 800,
                h: 580
            }
        );
        // A point on the empty desktop hits no window.
        assert!(sway_window_at(&v, 2000, 1300).is_none());
    }

    #[test]
    fn sway_focus_order_follows_focus_arrays() {
        let tree = sway_tree();
        let order = sway_focus_order(&tree);
        assert_eq!(order.focused.as_deref(), Some("ext-firefox"));
        assert_eq!(order.unfocused, ["ext-foot", "ext-vim"]);

        // Workspace 2 alone: its window is ranked, but not focused.
        let order = sway_focus_order(&tree.nodes[0].nodes[1]);
        assert_eq!(order.focused, None);
        assert_eq!(order.unfocused, ["ext-vim"]);
    }

    #[test]
    fn hypr_focused_output_picks_focused_monitor() {
        let v: serde_json::Value = serde_json::from_str(HYPR_MONITORS).unwrap();
        assert_eq!(hypr_focused_output(&v).as_deref(), Some("HDMI-A-1"));
    }

    #[test]
    fn hypr_active_window_rect_reads_at_and_size() {
        let v: serde_json::Value = serde_json::from_str(HYPR_ACTIVEWINDOW).unwrap();
        assert_eq!(
            hypr_active_window_rect(&v),
            Some(Region {
                x: 120,
                y: 340,
                w: 800,
                h: 600
            })
        );
    }

    #[test]
    fn hypr_no_active_window_is_none() {
        // Hyprland returns `{}` when nothing is focused.
        let v: serde_json::Value = serde_json::from_str("{}").unwrap();
        assert!(hypr_active_window_rect(&v).is_none());
    }

    #[test]
    fn hypr_focus_order_follows_focus_history_ids() {
        let clients: Value = serde_json::from_str(HYPR_CLIENTS).unwrap();
        let active: Value = serde_json::from_str(HYPR_ACTIVE_OF_CLIENTS).unwrap();
        let order = hypr_focus_order(&clients, &active).expect("an array of clients");
        assert_eq!(order.focused.as_deref(), Some("18000003"));
        assert_eq!(
            order.unfocused,
            ["18000005", "18000004", "18000002", "18000006"]
        );
    }

    #[test]
    fn hypr_focus_order_without_an_active_window_ranks_but_does_not_focus() {
        let clients: Value = serde_json::from_str(HYPR_CLIENTS).unwrap();
        // Hyprland returns `{}` when nothing is focused, while the history still
        // ranks every window.
        let order = hypr_focus_order(&clients, &json!({})).expect("an array of clients");
        assert_eq!(order.focused, None);
        assert_eq!(
            order.unfocused,
            ["18000003", "18000005", "18000004", "18000002", "18000006"]
        );
        // No windows at all: an empty order, not a failure.
        let empty = hypr_focus_order(&json!([]), &json!({})).expect("an array of clients");
        assert_eq!(empty.focused, None);
        assert!(empty.unfocused.is_empty());
        // Anything that is not an array is a failure, though.
        assert!(hypr_focus_order(&json!({}), &json!({})).is_none());
    }

    #[test]
    fn niri_focus_order_sorts_by_focus_timestamp() {
        let windows: Value = serde_json::from_str(NIRI_WINDOWS).unwrap();
        let order = niri_focus_order(&windows).expect("an array of windows");
        assert_eq!(order.focused.as_deref(), Some("3"));
        assert_eq!(order.unfocused, ["5", "4", "2", "6"]);
    }

    #[test]
    fn niri_focus_order_puts_never_focused_windows_last() {
        // `focus_timestamp` is absent for a window never focused since the
        // compositor started, and `is_focused` can be false for every window.
        let windows = json!([
            {"id": 7, "title": "fresh", "app_id": "foot", "is_focused": false},
            {"id": 8, "title": "old", "app_id": "foot", "is_focused": false,
             "focus_timestamp": {"secs": 12, "nanos": 5}},
            {"id": 9, "title": "newer", "app_id": "foot", "is_focused": false,
             // Same second, later nanos: the pair has to be compared as a pair.
             "focus_timestamp": {"secs": 12, "nanos": 900}},
        ]);
        let order = niri_focus_order(&windows).expect("an array of windows");
        assert_eq!(order.focused, None);
        assert_eq!(order.unfocused, ["9", "8", "7"]);
    }

    #[test]
    fn niri_focused_output_reads_name() {
        // Shape per niri's `focused-output` (the Output object). Unverified live.
        let v: serde_json::Value = serde_json::from_str(
            r#"{"name":"eDP-1","make":"BOE","model":"Z",
                "logical":{"x":0,"y":0,"width":1920,"height":1080,"scale":1.0}}"#,
        )
        .unwrap();
        assert_eq!(niri_focused_output(&v).as_deref(), Some("eDP-1"));
        // `null` (no focused output) → None.
        assert!(niri_focused_output(&serde_json::Value::Null).is_none());
    }
}
