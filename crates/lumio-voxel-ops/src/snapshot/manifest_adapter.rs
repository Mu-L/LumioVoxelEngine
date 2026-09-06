//! Manifest object builder. Does not invent a second serializer.

#![forbid(unsafe_code)]

use super::capture_ref::VoxelCaptureRef;
use super::hex32;
use super::restore_preflight::RestoreError;
use crate::canonical::{CanonicalObject, CanonicalValue};

/// Snapshot container magic. 本仓自持:活契约 `lumio.voxel-world.v1` 不定义存档面
/// (`magic` / `checksum` / `snapshot` 在契约里零命中),架构仓 `native-abi.json` 也没有
/// snapshot 槽。将来存档要跨仓互操作时必须另开契约卡,不得就这么扩散出去。
pub const SNAPSHOT_MAGIC: &str = "LUMIOSNP1";
/// `snapshot-header.checksum` 覆盖的是省掉这两个成员之后的规范对象。见上,本仓自持。
pub const SNAPSHOT_CHECKSUM_OMIT: &[&str] = &["checksum", "hash"];
/// 本仓存档容器的格式代数。它进 canonical object 的 `schemaEpoch` 成员,改动即改已发布
/// 的存档身份;与任何上游基线无关。
pub const SNAPSHOT_SCHEMA_EPOCH: u64 = 1;

/// Schema id this mapping wraps. Frozen wire identity, not a layering name.
pub const SNAPSHOT_HEADER_SCHEMA: &str = "snapshot-header";
/// Schema id this mapping wraps. Frozen wire identity, not a layering name.
pub const SNAPSHOT_PAYLOAD_SCHEMA: &str = "voxel-snapshot-payload";

/// Adapter-internal canonical object for one captured cut.
pub struct ManifestAdapter;

impl ManifestAdapter {
    pub fn object(capture: &VoxelCaptureRef) -> Result<CanonicalObject, RestoreError> {
        let header = SNAPSHOT_HEADER_SCHEMA;
        let payload = SNAPSHOT_PAYLOAD_SCHEMA;
        let stamp = capture.stamp();
        let mut object = CanonicalObject::new();
        let mut members: Vec<(String, CanonicalValue)> = vec![
            ("schemaId".into(), CanonicalValue::text(payload)),
            ("headerSchemaId".into(), CanonicalValue::text(header)),
            ("magic".into(), CanonicalValue::text(SNAPSHOT_MAGIC)),
            (
                "schemaEpoch".into(),
                CanonicalValue::Uint(SNAPSHOT_SCHEMA_EPOCH),
            ),
            ("worldId".into(), CanonicalValue::text(&stamp.world_id)),
            ("contextId".into(), CanonicalValue::text(&stamp.context_id)),
            ("generation".into(), CanonicalValue::Uint(stamp.generation)),
            (
                "worldRevision".into(),
                CanonicalValue::Uint(stamp.world_revision),
            ),
        ];
        for (id, rev) in &stamp.section_revision_set {
            members.push((format!("sectionRevision.{id}"), CanonicalValue::Uint(*rev)));
        }
        members.push((
            "configHash".into(),
            CanonicalValue::text(capture.config_hash()),
        ));
        members.push((
            "rootIdentity".into(),
            CanonicalValue::text(hex32(&capture.root_identity())),
        ));
        for (key, value) in members {
            object
                .insert(key, value)
                .map_err(|_| RestoreError::invalid_handle())?;
        }
        debug_assert!(
            SNAPSHOT_CHECKSUM_OMIT
                .iter()
                .all(|omit| !object.contains_key(omit)),
            "snapshot-header checksum omits checksum/hash"
        );
        Ok(object)
    }
}
