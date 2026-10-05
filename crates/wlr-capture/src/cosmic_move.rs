//! Moving a window onto the current workspace on cosmic-comp, over
//! `zcosmic_toplevel_manager_v1.move_to_ext_workspace`.
//!
//! COSMIC has no IPC socket to ask, and `ext-workspace-v1` manages workspaces without
//! saying anything of the windows on them. What does is COSMIC's own pair: its toplevel
//! manager moves a window given an `ext_workspace_handle_v1` and an output, and its
//! toplevel info reports which `ext_workspace_handle_v1` each window is on.
//!
//! COSMIC names no focused workspace either. The current one is taken to be the active
//! workspace holding the active window — the window the switcher was started over — or,
//! with no window active, the only active workspace there is: one per output, so that
//! settles it on a single output and leaves it undecided on several.

use std::time::Instant;

use wayland_client::{
    Connection, Dispatch, Proxy, QueueHandle, delegate_noop, event_created_child,
    globals::{GlobalListContents, registry_queue_init},
    protocol::{wl_output::WlOutput, wl_registry::WlRegistry},
};
use wayland_protocols::ext::foreign_toplevel_list::v1::client::{
    ext_foreign_toplevel_handle_v1::{self, ExtForeignToplevelHandleV1},
    ext_foreign_toplevel_list_v1::{self, ExtForeignToplevelListV1},
};
use wayland_protocols::ext::workspace::v1::client::{
    ext_workspace_group_handle_v1::{self, ExtWorkspaceGroupHandleV1},
    ext_workspace_handle_v1::{self, ExtWorkspaceHandleV1},
    ext_workspace_manager_v1::{self, ExtWorkspaceManagerV1},
};

use crate::cosmic_activate::has_capability;
use crate::cosmic_protocol::toplevel_info::v1::client::{
    zcosmic_toplevel_handle_v1::{self, ZcosmicToplevelHandleV1},
    zcosmic_toplevel_info_v1::{self, ZcosmicToplevelInfoV1},
};
use crate::cosmic_protocol::toplevel_management::v1::client::zcosmic_toplevel_manager_v1::{
    self, ZcosmicToplelevelManagementCapabilitiesV1 as Capability, ZcosmicToplevelManagerV1,
};
use crate::focus::{COSMIC_TIMEOUT, cosmic_wait};

/// Move the window whose `ext-foreign-toplevel-list-v1` identifier is `identifier` onto
/// the current workspace. `None` if cosmic-comp lacks the protocols for it, or the
/// window or the current workspace cannot be told.
pub(crate) fn move_to_current_workspace(identifier: &str) -> Option<()> {
    let conn = Connection::connect_to_env().ok()?;
    let (globals, mut queue) = registry_queue_init::<Moving>(&conn).ok()?;
    let qh = queue.handle();
    let mut state = Moving::default();

    // The outputs first: a workspace group only reports the outputs it is on to a
    // client that has bound them.
    let outputs: Vec<(u32, u32)> = globals.contents().with_list(|list| {
        list.iter()
            .filter(|g| g.interface == WlOutput::interface().name)
            .map(|g| (g.name, g.version))
            .collect()
    });
    for (name, version) in outputs {
        let output: WlOutput = globals.registry().bind(name, version.min(4), &qh, ());
        state.outputs.push(output);
    }
    // `ext_workspace_enter` on a COSMIC toplevel arrived in toplevel info version 3,
    // `move_to_ext_workspace` in the manager's version 4.
    let info: ZcosmicToplevelInfoV1 = globals.bind(&qh, 3..=3, ()).ok()?;
    let manager: ZcosmicToplevelManagerV1 = globals.bind(&qh, 4..=4, ()).ok()?;
    let _workspaces: ExtWorkspaceManagerV1 = globals.bind(&qh, 1..=1, ()).ok()?;
    let _list: ExtForeignToplevelListV1 = globals.bind(&qh, 1..=1, ()).ok()?;

    // The first roundtrip brings the toplevel handles, the workspaces and the manager's
    // capabilities; the second the `identifier` event naming each toplevel.
    queue.roundtrip(&mut state).ok()?;
    queue.roundtrip(&mut state).ok()?;
    if !has_capability(&state.capabilities, Capability::MoveToExtWorkspace) {
        return None;
    }
    let target = state
        .toplevels
        .iter()
        .position(|(_, id)| id == identifier)?;

    for (handle, _) in state.toplevels.clone() {
        let cosmic = info.get_cosmic_toplevel(&handle, &qh, ());
        state.cosmic.push(cosmic);
    }
    // cosmic-comp describes the toplevels from its own refresh tick, not in answer to
    // `get_cosmic_toplevel` (see `CosmicSnapshot::query`).
    let deadline = Instant::now() + COSMIC_TIMEOUT;
    while state.windows.iter().any(|w| !w.described)
        && cosmic_wait(&mut queue, &mut state, deadline)
    {}

    let (workspace, output) =
        current_workspace(&state.windows, &state.workspaces_active(), &state.groups)?;
    if state.windows[target].workspaces.contains(&workspace) {
        return Some(());
    }
    manager.move_to_ext_workspace(
        &state.cosmic[target],
        &state.workspaces[workspace].0,
        &state.outputs[output],
    );
    // Flush the request, and give the compositor the chance to report a protocol error
    // on it rather than letting the process exit before it is even sent.
    queue.roundtrip(&mut state).ok()?;
    Some(())
}

/// One window, by what places it.
#[derive(Default)]
struct Window {
    /// Whether the compositor reports the window as `activated`.
    activated: bool,
    /// Whether the window's initial `state` event has arrived.
    described: bool,
    /// The workspaces it is on, as indices into [`Moving::workspaces`].
    workspaces: Vec<usize>,
}

/// One workspace group, by what it holds.
#[derive(Default)]
struct Group {
    /// The outputs it is on, as indices into [`Moving::outputs`].
    outputs: Vec<usize>,
    /// Its workspaces, as indices into [`Moving::workspaces`].
    workspaces: Vec<usize>,
}

/// The current workspace and an output it is on, as indices: the active workspace
/// holding an active window, or else the one active workspace if there is only one.
/// Free function so the rule is unit-testable without a live compositor.
fn current_workspace(
    windows: &[Window],
    active: &[bool],
    groups: &[Group],
) -> Option<(usize, usize)> {
    let is_active = |&ws: &usize| active.get(ws) == Some(&true);
    let workspace = windows
        .iter()
        .filter(|w| w.activated)
        .flat_map(|w| &w.workspaces)
        .copied()
        .find(is_active)
        .or_else(|| {
            let mut all = (0..active.len()).filter(is_active);
            let only = all.next()?;
            all.next().is_none().then_some(only)
        })?;
    let output = groups
        .iter()
        .find(|g| g.workspaces.contains(&workspace))?
        .outputs
        .first()
        .copied()?;
    Some((workspace, output))
}

/// What cosmic-comp says of its windows and workspaces.
#[derive(Default)]
struct Moving {
    /// Every bound `wl_output`.
    outputs: Vec<WlOutput>,
    /// Every `ext-foreign-toplevel-list-v1` handle with the identifier naming it.
    toplevels: Vec<(ExtForeignToplevelHandleV1, String)>,
    /// The COSMIC extension object of each toplevel, in `toplevels` order.
    cosmic: Vec<ZcosmicToplevelHandleV1>,
    /// Each toplevel's placement, in `toplevels` order.
    windows: Vec<Window>,
    /// Every workspace, and whether it is active.
    workspaces: Vec<(ExtWorkspaceHandleV1, bool)>,
    /// Every workspace group, by its handle.
    groups: Vec<Group>,
    group_handles: Vec<ExtWorkspaceGroupHandleV1>,
    /// The toplevel manager's `capabilities` payload, still as the wire carried it.
    capabilities: Vec<u8>,
}

impl Moving {
    fn workspaces_active(&self) -> Vec<bool> {
        self.workspaces.iter().map(|&(_, active)| active).collect()
    }

    fn workspace(&self, handle: &ExtWorkspaceHandleV1) -> Option<usize> {
        self.workspaces.iter().position(|(h, _)| h == handle)
    }

    fn output(&self, output: &WlOutput) -> Option<usize> {
        self.outputs.iter().position(|o| o == output)
    }

    /// The window whose COSMIC extension object is `handle`. `get_cosmic_toplevel` is
    /// issued in `toplevels` order, so the lists stay aligned.
    fn window_of(&mut self, handle: &ZcosmicToplevelHandleV1) -> Option<&mut Window> {
        let i = self.cosmic.iter().position(|h| h == handle)?;
        self.windows.get_mut(i)
    }

    fn group_of(&mut self, handle: &ExtWorkspaceGroupHandleV1) -> Option<&mut Group> {
        let i = self.group_handles.iter().position(|h| h == handle)?;
        self.groups.get_mut(i)
    }
}

impl Dispatch<WlRegistry, GlobalListContents> for Moving {
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

impl Dispatch<ExtForeignToplevelListV1, ()> for Moving {
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
            state.windows.push(Window::default());
        }
    }

    event_created_child!(Moving, ExtForeignToplevelListV1, [
        ext_foreign_toplevel_list_v1::EVT_TOPLEVEL_OPCODE => (ExtForeignToplevelHandleV1, ()),
    ]);
}

impl Dispatch<ExtForeignToplevelHandleV1, ()> for Moving {
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

impl Dispatch<ZcosmicToplevelInfoV1, ()> for Moving {
    fn event(
        _: &mut Self,
        _: &ZcosmicToplevelInfoV1,
        _: zcosmic_toplevel_info_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }

    event_created_child!(Moving, ZcosmicToplevelInfoV1, [
        zcosmic_toplevel_info_v1::EVT_TOPLEVEL_OPCODE => (ZcosmicToplevelHandleV1, ()),
    ]);
}

impl Dispatch<ZcosmicToplevelHandleV1, ()> for Moving {
    fn event(
        state: &mut Self,
        handle: &ZcosmicToplevelHandleV1,
        event: zcosmic_toplevel_handle_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        use zcosmic_toplevel_handle_v1::Event;
        match event {
            Event::State { state: states } => {
                let activated = crate::focus::cosmic_is_activated(&states);
                if let Some(w) = state.window_of(handle) {
                    w.described = true;
                    w.activated = activated;
                }
            }
            Event::ExtWorkspaceEnter { workspace } => {
                let Some(ws) = state.workspace(&workspace) else {
                    return;
                };
                if let Some(w) = state.window_of(handle) {
                    w.workspaces.push(ws);
                }
            }
            Event::ExtWorkspaceLeave { workspace } => {
                let Some(ws) = state.workspace(&workspace) else {
                    return;
                };
                if let Some(w) = state.window_of(handle) {
                    w.workspaces.retain(|&w| w != ws);
                }
            }
            _ => {}
        }
    }
}

impl Dispatch<ZcosmicToplevelManagerV1, ()> for Moving {
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

impl Dispatch<ExtWorkspaceManagerV1, ()> for Moving {
    fn event(
        state: &mut Self,
        _: &ExtWorkspaceManagerV1,
        event: ext_workspace_manager_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        use ext_workspace_manager_v1::Event;
        match event {
            Event::WorkspaceGroup { workspace_group } => {
                state.group_handles.push(workspace_group);
                state.groups.push(Group::default());
            }
            Event::Workspace { workspace } => state.workspaces.push((workspace, false)),
            _ => {}
        }
    }

    event_created_child!(Moving, ExtWorkspaceManagerV1, [
        ext_workspace_manager_v1::EVT_WORKSPACE_GROUP_OPCODE => (ExtWorkspaceGroupHandleV1, ()),
        ext_workspace_manager_v1::EVT_WORKSPACE_OPCODE => (ExtWorkspaceHandleV1, ()),
    ]);
}

impl Dispatch<ExtWorkspaceGroupHandleV1, ()> for Moving {
    fn event(
        state: &mut Self,
        handle: &ExtWorkspaceGroupHandleV1,
        event: ext_workspace_group_handle_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        use ext_workspace_group_handle_v1::Event;
        match event {
            Event::OutputEnter { output } => {
                let Some(o) = state.output(&output) else {
                    return;
                };
                if let Some(g) = state.group_of(handle) {
                    g.outputs.push(o);
                }
            }
            Event::OutputLeave { output } => {
                let Some(o) = state.output(&output) else {
                    return;
                };
                if let Some(g) = state.group_of(handle) {
                    g.outputs.retain(|&g| g != o);
                }
            }
            Event::WorkspaceEnter { workspace } => {
                let Some(ws) = state.workspace(&workspace) else {
                    return;
                };
                if let Some(g) = state.group_of(handle) {
                    g.workspaces.push(ws);
                }
            }
            Event::WorkspaceLeave { workspace } => {
                let Some(ws) = state.workspace(&workspace) else {
                    return;
                };
                if let Some(g) = state.group_of(handle) {
                    g.workspaces.retain(|&g| g != ws);
                }
            }
            _ => {}
        }
    }
}

impl Dispatch<ExtWorkspaceHandleV1, ()> for Moving {
    fn event(
        state: &mut Self,
        handle: &ExtWorkspaceHandleV1,
        event: ext_workspace_handle_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let ext_workspace_handle_v1::Event::State { state: flags } = event
            && let Some((_, active)) = state.workspaces.iter_mut().find(|(h, _)| h == handle)
        {
            *active = flags
                .into_result()
                .is_ok_and(|f| f.contains(ext_workspace_handle_v1::State::Active));
        }
    }
}

delegate_noop!(Moving: ignore WlOutput);

#[cfg(test)]
mod tests {
    use super::*;

    fn window(activated: bool, workspaces: &[usize]) -> Window {
        Window {
            activated,
            described: true,
            workspaces: workspaces.to_vec(),
        }
    }

    /// Two outputs, each with a group of two workspaces: 0 and 1 on output 0, 2 and 3
    /// on output 1. Workspaces 0 and 3 are the ones shown.
    fn two_outputs() -> (Vec<bool>, Vec<Group>) {
        let active = vec![true, false, false, true];
        let groups = vec![
            Group {
                outputs: vec![0],
                workspaces: vec![0, 1],
            },
            Group {
                outputs: vec![1],
                workspaces: vec![2, 3],
            },
        ];
        (active, groups)
    }

    #[test]
    fn the_current_workspace_is_the_active_windows() {
        let (active, groups) = two_outputs();
        let windows = [window(false, &[0]), window(true, &[3]), window(false, &[1])];
        assert_eq!(current_workspace(&windows, &active, &groups), Some((3, 1)));
    }

    #[test]
    fn an_active_window_on_several_workspaces_counts_on_the_shown_one() {
        // A sticky window is on every workspace of its output.
        let (active, groups) = two_outputs();
        let windows = [window(true, &[2, 3])];
        assert_eq!(current_workspace(&windows, &active, &groups), Some((3, 1)));
    }

    #[test]
    fn with_no_active_window_only_a_lone_active_workspace_will_do() {
        let (active, groups) = two_outputs();
        let windows = [window(false, &[0]), window(false, &[3])];
        // One active workspace per output: nothing tells which output is "here".
        assert_eq!(current_workspace(&windows, &active, &groups), None);

        let one_output = [Group {
            outputs: vec![0],
            workspaces: vec![0, 1],
        }];
        assert_eq!(
            current_workspace(&windows, &[false, true], &one_output),
            Some((1, 0))
        );
    }

    #[test]
    fn a_workspace_on_no_output_cannot_be_moved_to() {
        let groups = [Group {
            outputs: vec![],
            workspaces: vec![0],
        }];
        assert_eq!(
            current_workspace(&[window(true, &[0])], &[true], &groups),
            None
        );
    }
}
