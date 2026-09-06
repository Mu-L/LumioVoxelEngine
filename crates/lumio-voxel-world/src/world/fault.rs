//! Target-World invariant / worker fault. Does not invent a `Faulted` session state.

#![forbid(unsafe_code)]

use super::WorldError;
use super::events::{FailureBundleFragment, WorldEvent, WorldEventSink};
use super::instance::VoxelWorld;
use super::shutdown::WorldShutdown;
use super::state::{has_session_edge, intern_session_event, intern_session_state};
use lumio_voxel_contracts::voxel_world as vw;

/// Short diagnostic label. Never a key or a raw payload.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FaultEvidence {
    diagnostic_name: &'static str,
}

impl FaultEvidence {
    pub fn new(diagnostic_name: &'static str) -> Self {
        Self { diagnostic_name }
    }

    pub fn diagnostic_name(&self) -> &'static str {
        self.diagnostic_name
    }
}

pub struct WorldFaultPort;

impl WorldFaultPort {
    /// Confine a severe fault to `world` and run Drain → FinalSnapshotTaken → Dispose.
    pub fn trip(
        world: &mut VoxelWorld,
        sink: &mut WorldEventSink,
        cause: &'static str,
        evidence: &FaultEvidence,
    ) -> Result<(), WorldError> {
        let cause = intern_cause(cause)?;
        if evidence.diagnostic_name.is_empty() {
            return Err(WorldError::invalid_handle());
        }
        if !shutdown_admissible(world) {
            return Err(WorldError::invalid_handle());
        }
        if !already_disposed(world) {
            sink.emit(WorldEvent::Failure(bundle(world, cause, evidence)));
        }
        WorldShutdown::begin(world, sink)?;
        WorldShutdown::drain(world, sink)?;
        WorldShutdown::finalize(world, sink)?;
        Ok(())
    }
}

/// 契约错误码收敛到契约表里的那一个 `'static` 实例;引擎通用的故障名本仓自持,原样透出。
/// 空串仍然是非法入参。
fn intern_cause(cause: &'static str) -> Result<&'static str, WorldError> {
    if cause.is_empty() {
        return Err(WorldError::invalid_handle());
    }
    Ok(vw::intern_error_code(cause).unwrap_or(cause))
}

fn shutdown_admissible(world: &VoxelWorld) -> bool {
    already_in_sequence(world) || can_drain(world)
}

fn already_in_sequence(world: &VoxelWorld) -> bool {
    let current = world.instance.state.current();
    current == intern_or("Draining")
        || current == intern_or("Snapshotted")
        || current == intern_or("Disposed")
}

fn already_disposed(world: &VoxelWorld) -> bool {
    world.instance.state.current() == intern_or("Disposed")
}

fn can_drain(world: &VoxelWorld) -> bool {
    let Some(event) = intern_session_event("Drain") else {
        return false;
    };
    let Some(to) = intern_session_state("Draining") else {
        return false;
    };
    has_session_edge(world.instance.state.current(), event, to)
}

fn intern_or(name: &'static str) -> &'static str {
    intern_session_state(name)
        .unwrap_or_else(|| panic!("{name} must exist as a generated SimulationSession state"))
}

fn bundle(
    world: &VoxelWorld,
    cause: &'static str,
    evidence: &FaultEvidence,
) -> FailureBundleFragment {
    let capture = world.instance.authority.capture();
    FailureBundleFragment::simulation(
        cause,
        world.instance.world_id.clone(),
        world.instance.world_context_id.clone(),
        world.instance.generation,
        world.instance.state.current(),
        capture.stamp().world_revision,
        evidence.diagnostic_name,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::port::map_internal_error;

    /// 与 `intern_cause` 内容相同但地址不同的 `'static` 实例,用来证明「收敛到契约表实例」
    /// 不是编译器把同一字面量合并的假象。
    fn detached_static(text: &str) -> &'static str {
        Box::leak(String::from(text).into_boxed_str())
    }

    #[test]
    fn empty_cause_is_rejected() {
        let err = intern_cause("").expect_err("an empty fault name is not admissible");
        assert_eq!(err.error_id(), "InvalidHandle");
    }

    #[test]
    fn engine_owned_fault_name_passes_through_and_stays_unregistered() {
        let cause =
            intern_cause("MaintenanceKick").expect("engine-owned fault names pass through as-is");
        assert_eq!(cause, "MaintenanceKick");
        assert!(
            !map_internal_error(cause).is_registered(),
            "只有活契约的 errorCodes 才算 registered"
        );
    }

    #[test]
    fn contract_error_code_converges_onto_the_contract_table_instance() {
        let detached = detached_static(vw::UNREGISTERED_BLOCK_TYPE);
        let table = vw::intern_error_code(vw::UNREGISTERED_BLOCK_TYPE)
            .expect("the live contract table owns this code");
        assert!(
            !std::ptr::eq(detached.as_ptr(), table.as_ptr()),
            "probe must start off the contract table"
        );

        let cause = intern_cause(detached).expect("contract codes are admissible causes");
        assert_eq!(cause, vw::UNREGISTERED_BLOCK_TYPE);
        assert!(
            std::ptr::eq(cause.as_ptr(), table.as_ptr()),
            "契约错误码必须收敛到契约表里的那一个实例"
        );
        assert!(map_internal_error(cause).is_registered());
    }
}
