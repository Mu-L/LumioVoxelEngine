//! Per-instance VoxelWorld lifecycle, generation, and typed command admission.

#![forbid(unsafe_code)]

mod admission;
mod barrier;
mod capture;
mod capture_admission;
mod command;
mod diagnostics;
mod durability_ack;
mod events;
mod fault;
mod instance;
mod residency;
mod restore;
mod routing;
mod shutdown;
mod state;
mod write_lane;

pub use admission::{AdmittedCommand, WorldCommand, WorldEndpoint};
pub use barrier::{BarrierScope, ForbiddenWork, reject_forbidden};
pub use capture::{CaptureEvidence, capture};
pub use capture_admission::RuntimeSnapshotCut;
pub use diagnostics::{DiagnosticsView, WorldDiagnostics};
pub use durability_ack::{AckEvidence, DurabilityReceipt, apply_durability_ack};
pub use events::{FailureBundleFragment, WorldEvent, WorldEventSink};
pub use fault::{FaultEvidence, WorldFaultPort};
pub use instance::{
    InstanceGenerationGuard, VOXEL_WORLD_ROLES, VoxelWorld, WorldConfigAdapter, WorldDescriptor,
    WorldStateView, intern_local_embedded_pair, intern_role,
};
pub use residency::{
    NoPinExemption, PinBudget, PinExemptionError, PinExemptionHook, PinHandle, PinId, PinReadiness,
    PinStatus, RegionPinError, RegionPinManager, RegionPinStatus, ResidencyPinError, UnloadReceipt,
    request_unload, section_keys_for_region, unload, unload_section,
};
pub use restore::{RestoreReceipt, restore};
pub use routing::WorldRouter;
pub use shutdown::WorldShutdown;
pub use state::{SESSION_TRANSITIONS, SIMULATION_SESSION_MACHINE};
pub use write_lane::{WorldWriteLane, WriteLease};

use lumio_voxel_contracts::voxel_world as vw;

/// Stable world error. Contract codes are interned; engine-generic ids pass through.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorldError {
    error_id: &'static str,
}

impl WorldError {
    pub fn error_id(&self) -> &'static str {
        self.error_id
    }

    pub(crate) fn mapped(id: &'static str) -> Self {
        Self {
            error_id: intern_stable(id),
        }
    }

    pub(crate) fn invalid_handle() -> Self {
        Self::mapped("InvalidHandle")
    }

    pub(crate) fn stale_epoch() -> Self {
        Self::mapped("StaleEpoch")
    }

    pub(crate) fn session_mismatch() -> Self {
        Self::mapped("SessionMismatch")
    }

    pub(crate) fn role_mismatch() -> Self {
        Self::mapped("RoleMismatch")
    }

    pub(crate) fn claim_not_granted() -> Self {
        Self::mapped("ClaimNotGranted")
    }
}

impl std::fmt::Display for WorldError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.error_id)
    }
}

impl std::error::Error for WorldError {}

/// 收敛到单一 `'static` 实例。错误 id 只有一套命名空间——活契约的 `errorCodes`;
/// 契约不定义的引擎通用失败(句柄 / 会话 / 预算 / 队列)由本仓自行命名,原样透出,
/// 不再有第二张表可以对照(见 ADR 0014)。
pub(crate) fn intern_stable(id: &'static str) -> &'static str {
    vw::intern_error_code(id).unwrap_or(id)
}
