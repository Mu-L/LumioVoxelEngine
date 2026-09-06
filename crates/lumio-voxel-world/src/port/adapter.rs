//! Total adapter routing generated voxel-world-port requests to WorldEndpoint/Barrier.

#![forbid(unsafe_code)]

use super::error_mapping::PortError;
use crate::world::{
    AckEvidence, AdmittedCommand, CaptureEvidence, DurabilityReceipt, RestoreReceipt,
    RuntimeSnapshotCut, VoxelWorld, WorldCommand, WorldDescriptor, WorldEventSink, WorldRouter,
    WorldShutdown,
};
use lumio_voxel_domain::config_snapshot::VoxelConfigSnapshot;
use lumio_voxel_ops::async_support::{OriginEnvelope, OriginToken};
use lumio_voxel_ops::mutation::{
    MutationReceipt, MutationRequest, PreparedMutation, ReceiptStatus,
};
use lumio_voxel_ops::query::{VoxelQueryOutcome, VoxelQueryRequest};
use lumio_voxel_ops::snapshot::{SealedRestoreCandidate, VoxelCaptureRef};
use std::collections::BTreeMap;
use std::sync::Arc;

/// `voxel-world-port` schema id. A `static`, not a `const`: consumers compare the
/// adapter's `schema_id()` by pointer to prove there is exactly one materialization,
/// and a `const` would be inlined once per crate (ADR 0008 的同一条理由)。
pub static PORT_SCHEMA: &str = "voxel-world-port";
/// Rust type name this port binds to. Same `static` reasoning as [`PORT_SCHEMA`].
pub static PORT_RUST_TYPE: &str = "VoxelWorldPort";

/// Method names frozen by the `voxel-world-port` schema.
///
/// This table is a conformance witness only; it does not define a second
/// schema or serializer. The source of truth remains the Architecture artifact.
pub const PORT_METHODS: &[&str; 11] = &[
    "createWorld",
    "query",
    "prepareMutation",
    "commit",
    "abort",
    "status",
    "capture",
    "applyDurabilityAck",
    "restore",
    "quiesce",
    "destroy",
];

/// Compatibility alias for callers that use the shorter Port terminology.
/// Mutation status projection from the mutation receipt shape.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MutationStatus {
    Unknown,
    Prepared,
    Applied,
    Aborted,
    ResultPruned,
}

impl MutationStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Unknown => "Unknown",
            Self::Prepared => "Prepared",
            Self::Applied => "Applied",
            Self::Aborted => "Aborted",
            Self::ResultPruned => "ResultPruned",
        }
    }
}

impl From<ReceiptStatus> for MutationStatus {
    fn from(status: ReceiptStatus) -> Self {
        match status {
            ReceiptStatus::Unknown => Self::Unknown,
            ReceiptStatus::Prepared => Self::Prepared,
            ReceiptStatus::Applied => Self::Applied,
        }
    }
}

/// Interned schema id plus generated rust binding name for `voxel-world-port`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PortEvidence {
    pub schema_id: &'static str,
    pub binding_rust_type: &'static str,
}

/// Total adapter over one `VoxelWorld`. No extra interior mutability and no callbacks.
pub struct VoxelWorldPortAdapter<'a> {
    world: &'a mut VoxelWorld,
}

impl<'a> VoxelWorldPortAdapter<'a> {
    /// Generated `createWorld` entry point. The returned world is owned by the
    /// caller and must subsequently be accessed through this adapter surface.
    pub fn create_world(
        descriptor: WorldDescriptor,
        snapshot: Arc<VoxelConfigSnapshot>,
    ) -> Result<VoxelWorld, PortError> {
        VoxelWorld::create(descriptor, snapshot).map_err(PortError::from)
    }

    pub fn new(world: &'a mut VoxelWorld) -> Self {
        Self { world }
    }

    pub fn schema_id(&self) -> &'static str {
        PORT_SCHEMA
    }

    pub fn evidence(&self) -> PortEvidence {
        PortEvidence {
            schema_id: PORT_SCHEMA,
            binding_rust_type: PORT_RUST_TYPE,
        }
    }

    pub fn admit(&mut self, command: WorldCommand) -> Result<AdmittedCommand, PortError> {
        self.world
            .endpoint()
            .admit(command)
            .map_err(PortError::from)
    }

    pub fn query(
        &mut self,
        envelope: OriginEnvelope<VoxelQueryRequest>,
    ) -> Result<OriginEnvelope<VoxelQueryOutcome>, PortError> {
        WorldRouter::query(self.world, envelope).map_err(PortError::from)
    }

    pub fn prepare_mutation(
        &mut self,
        envelope: OriginEnvelope<MutationRequest>,
    ) -> Result<OriginEnvelope<PreparedMutation>, PortError> {
        WorldRouter::prepare(self.world, envelope).map_err(PortError::from)
    }

    pub fn commit(
        &mut self,
        envelope: OriginEnvelope<PreparedMutation>,
    ) -> Result<OriginEnvelope<MutationReceipt>, PortError> {
        WorldRouter::commit(self.world, envelope).map_err(PortError::from)
    }

    pub fn abort(
        &mut self,
        envelope: OriginEnvelope<MutationRequest>,
    ) -> Result<OriginEnvelope<()>, PortError> {
        WorldRouter::abort(self.world, envelope).map_err(PortError::from)
    }

    /// Generated `status` entry point for a mutation transaction.
    pub fn status(&self, txn_id: &str) -> Result<MutationStatus, PortError> {
        if txn_id.is_empty() {
            return Err(super::error_mapping::map_internal_error("InvalidHandle"));
        }
        Ok(self.world.instance.ledger.status(txn_id).into())
    }

    pub fn capture(
        &mut self,
        cut: &RuntimeSnapshotCut,
    ) -> Result<(VoxelCaptureRef, CaptureEvidence), PortError> {
        crate::world::capture(self.world, cut).map_err(PortError::from)
    }

    pub fn restore(
        &mut self,
        candidate: SealedRestoreCandidate,
    ) -> Result<RestoreReceipt, PortError> {
        crate::world::restore(self.world, candidate).map_err(PortError::from)
    }

    pub fn apply_durability_ack(
        &mut self,
        ack: AckEvidence,
    ) -> Result<DurabilityReceipt, PortError> {
        crate::world::apply_durability_ack(self.world, ack).map_err(PortError::from)
    }

    /// Quiesce ingress by applying the generated SimulationSession pause edge.
    /// The reason is an audit-side input and is intentionally not persisted in
    /// the frozen Port payload.
    pub fn quiesce(&mut self, reason: impl AsRef<str>) -> Result<(), PortError> {
        let reason = reason.as_ref();
        if reason.is_empty() {
            return Err(super::error_mapping::map_internal_error("InvalidArgument"));
        }
        if self.world.state_view().lifecycle() == "Paused" {
            return Ok(());
        }
        let guard = self.world.generation_guard();
        let origin = OriginToken::try_new(
            guard.world_context_id(),
            guard.generation(),
            format!("quiesce:{reason}"),
            0,
            BTreeMap::new(),
            "VoxelCommit",
        )
        .map_err(|err| super::error_mapping::map_internal_error(err.error_id()))?;
        self.world
            .endpoint()
            .admit(WorldCommand::Lifecycle {
                event: "Pause",
                to: "Paused",
                origin,
            })
            .map(|_| ())
            .map_err(PortError::from)
    }

    /// Generated `destroy` entry point. Destruction is the ordered shutdown
    /// sequence; a bounded sink keeps event delivery outside the caller's API.
    pub fn destroy(&mut self) -> Result<(), PortError> {
        let mut sink = WorldEventSink::bounded(16);
        self.shutdown(&mut sink)
    }

    pub fn shutdown(&mut self, sink: &mut WorldEventSink) -> Result<(), PortError> {
        WorldShutdown::begin(self.world, sink).map_err(PortError::from)?;
        WorldShutdown::drain(self.world, sink).map_err(PortError::from)?;
        WorldShutdown::finalize(self.world, sink).map_err(PortError::from)?;
        Ok(())
    }
}
