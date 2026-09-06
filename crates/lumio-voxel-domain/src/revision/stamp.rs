//! Map internal revisions onto the published VoxelRevisionStamp field names.

use super::allocator::{SectionRevision, WorldRevision};
use std::collections::BTreeMap;

/// Schema id this mapping wraps. Frozen wire identity, not a layering name.
pub const REVISION_STAMP_SCHEMA: &str = "voxel-revision-stamp";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RevisionStamp {
    pub schema_id: &'static str,
    pub world_id: String,
    pub context_id: String,
    pub generation: u64,
    pub world_revision: u64,
    pub section_revision_set: BTreeMap<String, u64>,
}

pub fn to_revision_stamp(
    world_id: impl Into<String>,
    context_id: impl Into<String>,
    generation: u64,
    world: WorldRevision,
    sections: &[(String, SectionRevision)],
) -> RevisionStamp {
    let mut section_revision_set = BTreeMap::new();
    for (id, rev) in sections {
        section_revision_set.insert(id.clone(), rev.value());
    }
    RevisionStamp {
        schema_id: REVISION_STAMP_SCHEMA,
        world_id: world_id.into(),
        context_id: context_id.into(),
        generation,
        world_revision: world.value(),
        section_revision_set,
    }
}
