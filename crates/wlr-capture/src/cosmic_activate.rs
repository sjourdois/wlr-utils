//! Focusing a window on cosmic-comp, over `zcosmic_toplevel_manager_v1`.
//!
//! cosmic-comp advertises no `zwlr_foreign_toplevel_manager_v1`, so the portable
//! activation path in [`crate::wl`] has nothing to talk to there. Its replacement is
//! `zcosmic_toplevel_manager_v1.activate`, which takes a `zcosmic_toplevel_handle_v1`;
//! that handle comes from `zcosmic_toplevel_info_v1.get_cosmic_toplevel`, which in turn
//! takes the window's `ext_foreign_toplevel_handle_v1`.
//!
//! The chain therefore starts at the `ext-foreign-toplevel-list-v1` handle — the same
//! object the capture engine enumerates windows with — so the target is named by its
//! exact `identifier`. None of the app-id/title matching zwlr forces on us applies here.

use wayland_client::{
    Connection, Dispatch, Proxy, QueueHandle, delegate_noop, event_created_child,
    globals::{GlobalListContents, registry_queue_init},
    protocol::{wl_registry::WlRegistry, wl_seat::WlSeat},
};
use wayland_protocols::ext::foreign_toplevel_list::v1::client::{
    ext_foreign_toplevel_handle_v1::{self, ExtForeignToplevelHandleV1},
    ext_foreign_toplevel_list_v1::{self, ExtForeignToplevelListV1},
};

use crate::cosmic_protocol::toplevel_info::v1::client::{
    zcosmic_toplevel_handle_v1::ZcosmicToplevelHandleV1,
    zcosmic_toplevel_info_v1::{self, ZcosmicToplevelInfoV1},
};
use crate::cosmic_protocol::toplevel_management::v1::client::zcosmic_toplevel_manager_v1::{
    self, ZcosmicToplelevelManagementCapabilitiesV1 as Capability, ZcosmicToplevelManagerV1,
};
use crate::error::{CaptureError, Context, Result};

/// Focus the window whose `ext-foreign-toplevel-list-v1` identifier is `identifier`.
///
/// Returns [`CaptureError::ActivationUnsupported`] when the compositor is not a COSMIC
/// one either — it advertises no toplevel manager, or the manager says it will not
/// honour `activate`.
pub fn activate(identifier: &str) -> Result<()> {
    let conn = Connection::connect_to_env().context("Wayland connection")?;
    let (globals, mut queue) =
        registry_queue_init::<Activation>(&conn).context("Wayland registry")?;
    let qh = queue.handle();

    // `get_cosmic_toplevel` — the only bridge from an ext handle to a COSMIC one —
    // arrived in `zcosmic_toplevel_info_v1` version 2. Either global missing means this
    // is not cosmic-comp (or is one too old to name our windows), which is the caller's
    // "no activation protocol here" case rather than a failure of this path.
    let (Ok(info), Ok(manager)) = (
        globals.bind::<ZcosmicToplevelInfoV1, _, _>(&qh, 2..=3, ()),
        globals.bind::<ZcosmicToplevelManagerV1, _, _>(&qh, 1..=4, ()),
    ) else {
        return Err(CaptureError::ActivationUnsupported);
    };
    // Bound but never used again: binding it is what makes the compositor advertise the
    // toplevels, and its handles have to outlive the requests made from them.
    let _list: ExtForeignToplevelListV1 = globals
        .bind(&qh, 1..=1, ())
        .context("ext_foreign_toplevel_list_v1 missing")?;
    let seat: WlSeat = globals.bind(&qh, 1..=8, ()).context("wl_seat missing")?;

    // Binding the list makes the compositor advertise the current toplevels: the first
    // roundtrip brings their handles, the second the `identifier` event naming each.
    // The manager's `capabilities` event rides along with the first.
    let mut state = Activation::default();
    queue.roundtrip(&mut state).context("Wayland roundtrip")?;
    queue.roundtrip(&mut state).context("Wayland roundtrip")?;

    if !can_activate(&state.capabilities) {
        return Err(CaptureError::ActivationUnsupported);
    }
    let i = position_of(
        state.toplevels.iter().map(|(_, id)| id.as_str()),
        identifier,
    )
    .with_context(|| format!("window to activate not found: {identifier}"))?;

    let cosmic = info.get_cosmic_toplevel(&state.toplevels[i].0, &qh, ());
    manager.activate(&cosmic, &seat);
    // Flush the request, and give the compositor the chance to report a protocol error
    // on it rather than letting the process exit before it is even sent.
    queue.roundtrip(&mut state).context("Wayland roundtrip")?;
    Ok(())
}

/// Where the window named `identifier` sits among the advertised ones.
///
/// `ext-foreign-toplevel-list-v1` identifiers are opaque, unique per window and stable
/// for its lifetime: the match is exact, never a prefix or a position. Free function so
/// the rule is unit-testable without a live compositor.
fn position_of<'a>(identifiers: impl Iterator<Item = &'a str>, identifier: &str) -> Option<usize> {
    identifiers
        .enumerate()
        .find(|(_, id)| *id == identifier)
        .map(|(i, _)| i)
}

/// Whether the compositor advertises `zcosmic_toplevel_manager_v1` with a version that
/// can name our windows, without binding anything. Used by `doctor` to report the
/// activation path in use.
pub fn manager_advertised(globals: &[(String, u32)]) -> bool {
    let named = |name: &str, floor: u32| {
        globals
            .iter()
            .any(|(n, version)| n == name && *version >= floor)
    };
    named(ZcosmicToplevelManagerV1::interface().name, 1)
        && named(ZcosmicToplevelInfoV1::interface().name, 2)
}

/// Whether a `zcosmic_toplevel_manager_v1.capabilities` array contains `activate`. The
/// array is a raw sequence of 32-bit enum values, in host byte order.
///
/// A manager that does not advertise the capability ignores `activate`, so the caller
/// has to say the window cannot be focused instead of silently doing nothing.
fn can_activate(capabilities: &[u8]) -> bool {
    capabilities
        .as_chunks::<4>()
        .0
        .iter()
        .copied()
        .map(u32::from_ne_bytes)
        .any(|c| c == Capability::Activate as u32)
}

/// The windows cosmic-comp advertises, and what its manager will honour.
#[derive(Default)]
struct Activation {
    /// Every `ext-foreign-toplevel-list-v1` handle with the identifier naming it.
    toplevels: Vec<(ExtForeignToplevelHandleV1, String)>,
    /// The manager's `capabilities` payload, still as the wire carried it.
    capabilities: Vec<u8>,
}

impl Dispatch<WlRegistry, GlobalListContents> for Activation {
    fn event(
        _: &mut Self,
        _: &WlRegistry,
        _: <WlRegistry as Proxy>::Event,
        _: &GlobalListContents,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ExtForeignToplevelListV1, ()> for Activation {
    fn event(
        state: &mut Self,
        _: &ExtForeignToplevelListV1,
        event: ext_foreign_toplevel_list_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let ext_foreign_toplevel_list_v1::Event::Toplevel { toplevel } = event {
            state.toplevels.push((toplevel, String::new()));
        }
    }

    event_created_child!(Activation, ExtForeignToplevelListV1, [
        ext_foreign_toplevel_list_v1::EVT_TOPLEVEL_OPCODE => (ExtForeignToplevelHandleV1, ()),
    ]);
}

impl Dispatch<ExtForeignToplevelHandleV1, ()> for Activation {
    fn event(
        state: &mut Self,
        handle: &ExtForeignToplevelHandleV1,
        event: ext_foreign_toplevel_handle_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let ext_foreign_toplevel_handle_v1::Event::Identifier { identifier } = event
            && let Some((_, slot)) = state.toplevels.iter_mut().find(|(h, _)| h == handle)
        {
            *slot = identifier;
        }
    }
}

impl Dispatch<ZcosmicToplevelInfoV1, ()> for Activation {
    fn event(
        _: &mut Self,
        _: &ZcosmicToplevelInfoV1,
        _: zcosmic_toplevel_info_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }

    event_created_child!(Activation, ZcosmicToplevelInfoV1, [
        zcosmic_toplevel_info_v1::EVT_TOPLEVEL_OPCODE => (ZcosmicToplevelHandleV1, ()),
    ]);
}

impl Dispatch<ZcosmicToplevelManagerV1, ()> for Activation {
    fn event(
        state: &mut Self,
        _: &ZcosmicToplevelManagerV1,
        event: zcosmic_toplevel_manager_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let zcosmic_toplevel_manager_v1::Event::Capabilities { capabilities } = event;
        state.capabilities = capabilities;
    }
}

delegate_noop!(Activation: ignore WlSeat);
delegate_noop!(Activation: ignore ZcosmicToplevelHandleV1);

#[cfg(test)]
mod tests {
    use super::*;

    /// A `capabilities` array as the wire carries it: 32-bit values, host byte order.
    fn caps(values: &[Capability]) -> Vec<u8> {
        values
            .iter()
            .flat_map(|c| (*c as u32).to_ne_bytes())
            .collect()
    }

    #[test]
    fn activate_is_found_among_the_capabilities() {
        // What cosmic-comp 1.8.0 answers: close, activate, maximize, minimize,
        // fullscreen, move_to_workspace, sticky, move_to_ext_workspace.
        let all = caps(&[
            Capability::Close,
            Capability::Activate,
            Capability::Maximize,
            Capability::Minimize,
            Capability::Fullscreen,
            Capability::MoveToWorkspace,
            Capability::Sticky,
            Capability::MoveToExtWorkspace,
        ]);
        assert!(can_activate(&all));
        assert!(can_activate(&caps(&[Capability::Activate])));
    }

    #[test]
    fn a_manager_without_activate_cannot_focus() {
        assert!(!can_activate(&caps(&[
            Capability::Close,
            Capability::Maximize
        ])));
        // No `capabilities` event at all: nothing says the request is honoured.
        assert!(!can_activate(&[]));
        // A truncated trailing value is not a capability; only whole ones count.
        assert!(!can_activate(&[2, 0, 0]));
    }

    #[test]
    fn the_target_window_is_matched_on_its_exact_identifier() {
        // Two windows of the same app with the same title — the case zwlr can only
        // resolve by creation order, and COSMIC by the identifier itself.
        let ids = ["toplevel-4", "toplevel-12", "toplevel-1"];
        let find = |wanted| position_of(ids.iter().copied(), wanted);

        assert_eq!(find("toplevel-4"), Some(0));
        assert_eq!(find("toplevel-12"), Some(1));
        assert_eq!(find("toplevel-1"), Some(2));
        // A prefix of a real identifier is a different window, not that one.
        assert_eq!(find("toplevel-"), None);
        assert_eq!(find("toplevel-123"), None);
        assert_eq!(find(""), None);
        assert_eq!(position_of(std::iter::empty(), "toplevel-1"), None);
    }

    #[test]
    fn a_compositor_advertising_neither_global_has_no_cosmic_manager() {
        let info = ZcosmicToplevelInfoV1::interface().name.to_string();
        let manager = ZcosmicToplevelManagerV1::interface().name.to_string();

        assert!(manager_advertised(&[
            (info.clone(), 2),
            (manager.clone(), 4)
        ]));
        // Version 1 of the info protocol has no `get_cosmic_toplevel`, so a COSMIC
        // handle can never be tied to the ext handle the caller names.
        assert!(!manager_advertised(&[
            (info.clone(), 1),
            (manager.clone(), 4)
        ]));
        assert!(!manager_advertised(&[(info, 3)]));
        assert!(!manager_advertised(&[(manager, 4)]));
        assert!(!manager_advertised(&[]));
    }
}
