//! `SimulationSession` state machine. Not `WorldSlotHost` or the section residency machine.
//!
//! 本仓自持这张表。它过去从旧合同制那份死基线镜像的 `state_transition_table()` 里取,
//! 镜像随 [ADR 0014] 整棵删除;活契约 `lumio.voxel-world.v1` 不定义会话生命周期,所以
//! 这九条边留在这里自己维护,值与被删镜像逐条一致(行为不变)。
//!
//! [ADR 0014]: ../../../.spec/decisions/0014-exit-legacy-baseline-contract-regime.md

#![forbid(unsafe_code)]

use super::WorldError;

/// Machine id this module drives.
pub const SIMULATION_SESSION_MACHINE: &str = "SimulationSession";

/// `(from, event, to)`. The single source of session edges in this workspace.
pub const SESSION_TRANSITIONS: &[(&str, &str, &str)] = &[
    ("Created", "Initialize", "Initialized"),
    ("Initialized", "Prime", "Ready"),
    ("Ready", "Start", "Running"),
    ("Running", "Pause", "Paused"),
    ("Paused", "Resume", "Running"),
    ("Running", "Drain", "Draining"),
    ("Paused", "Drain", "Draining"),
    ("Draining", "FinalSnapshotTaken", "Snapshotted"),
    ("Snapshotted", "Dispose", "Disposed"),
];

pub(crate) struct WorldState {
    current: &'static str,
}

impl WorldState {
    pub(crate) fn created() -> Self {
        Self {
            current: intern_session_state("Created").expect("Created is a SimulationSession state"),
        }
    }

    pub(crate) fn current(&self) -> &'static str {
        self.current
    }

    pub(crate) fn apply(
        &mut self,
        event: &'static str,
        to: &'static str,
    ) -> Result<(), WorldError> {
        if !has_session_edge(self.current, event, to) {
            return Err(WorldError::invalid_handle());
        }
        self.current = to;
        Ok(())
    }
}

pub(crate) fn simulation_session_machine() -> &'static str {
    SIMULATION_SESSION_MACHINE
}

pub(crate) fn intern_session_state(name: &str) -> Option<&'static str> {
    SESSION_TRANSITIONS.iter().find_map(|(from, _, to)| {
        if *from == name {
            Some(*from)
        } else if *to == name {
            Some(*to)
        } else {
            None
        }
    })
}

pub(crate) fn intern_session_event(name: &str) -> Option<&'static str> {
    SESSION_TRANSITIONS
        .iter()
        .find_map(|(_, event, _)| if *event == name { Some(*event) } else { None })
}

pub(crate) fn has_session_edge(from: &'static str, event: &'static str, to: &'static str) -> bool {
    SESSION_TRANSITIONS
        .iter()
        .any(|(f, e, t)| *f == from && *e == event && *t == to)
}

pub(crate) fn query_admissible(state: &'static str) -> bool {
    state == intern_or_bug("Ready")
        || state == intern_or_bug("Running")
        || state == intern_or_bug("Paused")
}

pub(crate) fn write_admissible(state: &'static str) -> bool {
    state == intern_or_bug("Running")
}

fn intern_or_bug(name: &'static str) -> &'static str {
    intern_session_state(name).unwrap_or_else(|| panic!("{name} must be a SimulationSession state"))
}
