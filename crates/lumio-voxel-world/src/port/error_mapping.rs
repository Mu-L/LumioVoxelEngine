//! Map World / Query / Mutation error ids onto interned stable identifiers.
//!
//! 只有一套错误 id 命名空间:活契约的 `errorCodes`(`voxel_world::VOXEL_WORLD_ERROR_CODES`),
//! 原样透传。契约不定义的引擎通用失败(句柄 / 会话 / 预算 / 队列)由本仓自行命名,仍然
//! 可观测,但 [`PortError::is_registered`] 对它们返回 `false`——判定谓词只认契约。

#![forbid(unsafe_code)]

use crate::world::{WorldError, intern_stable};
use lumio_voxel_contracts::voxel_world as vw;
use lumio_voxel_ops::mutation::MutationError;
use lumio_voxel_ops::query::QueryError;
use std::borrow::Cow;

/// Port-facing error. Known ids borrow the contract table; an id introduced by
/// an upstream producer is retained as an owned value instead of being disguised
/// as an unrelated known error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PortError {
    error_id: Cow<'static, str>,
}

impl PortError {
    pub fn error_id(&self) -> &str {
        self.error_id.as_ref()
    }

    pub fn is_registered(&self) -> bool {
        vw::is_error_code(self.error_id())
    }
}

impl std::fmt::Display for PortError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.error_id)
    }
}

impl std::error::Error for PortError {}

impl From<WorldError> for PortError {
    fn from(err: WorldError) -> Self {
        map_world_error(err)
    }
}

/// Exhaustive mapping of known ids. Unknown ids remain observable at
/// the Port boundary so callers cannot confuse a new producer error with
/// `InvalidHandle`.
pub fn map_internal_error(error_id: &str) -> PortError {
    // 契约错误码原样透传,只做 interning——它们已经是公共语义的稳定标识。
    if let Some(code) = vw::intern_error_code(error_id) {
        return PortError {
            error_id: Cow::Borrowed(code),
        };
    }
    let mapped = match error_id {
        "RevisionConflict" => "RevisionConflict",
        "MaintenanceKick" => "MaintenanceKick",
        "ReleaseMismatch" => "ReleaseMismatch",
        "NativeAbiMismatch" => "NativeAbiMismatch",
        "StaleEpoch" => "StaleEpoch",
        "FencingTokenStale" => "FencingTokenStale",
        "ManifestMalformed" => "ManifestMalformed",
        "ManifestUnsupportedVersion" => "ManifestUnsupportedVersion",
        "ManifestDigestMismatch" => "ManifestDigestMismatch",
        "ArtifactMissing" => "ArtifactMissing",
        "ArtifactDigestMismatch" => "ArtifactDigestMismatch",
        "SignatureMissing" => "SignatureMissing",
        "SignatureInvalid" => "SignatureInvalid",
        "TrustRootUnknown" => "TrustRootUnknown",
        "TrustPolicyRejected" => "TrustPolicyRejected",
        "KeyRevoked" => "KeyRevoked",
        "EvidenceMissing" => "EvidenceMissing",
        "EvidenceDigestMismatch" => "EvidenceDigestMismatch",
        "TargetProfileMismatch" => "TargetProfileMismatch",
        "CapabilityMissing" => "CapabilityMissing",
        "SymbolMissing" => "SymbolMissing",
        "SymbolCollision" => "SymbolCollision",
        "PackageIdentityConflict" => "PackageIdentityConflict",
        "WorkerPoolDuplicate" => "WorkerPoolDuplicate",
        "LoaderTimeout" => "LoaderTimeout",
        "LoaderCancelled" => "LoaderCancelled",
        "LoaderOutOfMemory" => "LoaderOutOfMemory",
        "PartialLoadRolledBack" => "PartialLoadRolledBack",
        "InvalidHandle" => "InvalidHandle",
        "HandleDoubleRelease" => "HandleDoubleRelease",
        "MessagePermissionDenied" => "MessagePermissionDenied",
        "StaleConnectionGeneration" => "StaleConnectionGeneration",
        "TargetRevisionUnavailable" => "TargetRevisionUnavailable",
        "BudgetExceeded" => "BudgetExceeded",
        "QueueFull" => "QueueFull",
        "CoordinateOutOfBounds" => "CoordinateOutOfBounds",
        "SnapshotBaseMismatch" => "SnapshotBaseMismatch",
        "SessionMismatch" => "SessionMismatch",
        "RoleMismatch" => "RoleMismatch",
        "ClaimNotGranted" => "ClaimNotGranted",
        "SessionAntiReplay" => "SessionAntiReplay",
        "InvalidArgument" => "InvalidArgument",
        "WrongContext" => "WrongContext",
        "BufferTooSmall" => "BufferTooSmall",
        "CapacityExceeded" => "CapacityExceeded",
        "Cancelled" => "Cancelled",
        "TimedOut" => "TimedOut",
        "ContextClosing" => "ContextClosing",
        "ContextDestroyed" => "ContextDestroyed",
        "PanicBoundary" => "PanicBoundary",
        "InternalInvariant" => "InternalInvariant",
        _ => {
            return PortError {
                error_id: Cow::Owned(error_id.to_owned()),
            };
        }
    };
    PortError {
        error_id: Cow::Borrowed(intern_stable(mapped)),
    }
}

pub fn map_world_error(err: WorldError) -> PortError {
    map_internal_error(err.error_id())
}

pub fn map_query_error(err: QueryError) -> PortError {
    map_internal_error(err.error_id())
}

pub fn map_mutation_error(err: MutationError) -> PortError {
    map_internal_error(err.error_id())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unregistered_block_type_passes_through_as_registered_contract_error() {
        let mapped = map_internal_error(vw::UNREGISTERED_BLOCK_TYPE);
        assert_eq!(mapped.error_id(), vw::UNREGISTERED_BLOCK_TYPE);
        assert!(mapped.is_registered());
    }
}
