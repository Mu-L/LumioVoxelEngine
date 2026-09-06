//! R-00081: four-state SECTION_PRESENCE mapping; absent directory id is Unchanged.

use lumio_voxel_contracts::sha256;
use lumio_voxel_contracts::voxel_world::SECTION_PRESENCE;
use lumio_voxel_domain::config_snapshot::{
    HostCapabilitySet, VoxelConfigInput, VoxelConfigSnapshot,
};
use lumio_voxel_domain::publication::{PublicationAuthority, PublishedStateRoot};
use lumio_voxel_domain::revision::{PinRegistry, REVISION_STAMP_SCHEMA, RevisionStamp};
use lumio_voxel_domain::section::{
    DirtyFrontier, SectionDirectoryBuilder, SectionPage, SectionPayload, SectionSlot,
};
use lumio_voxel_ops::query::{QueryExecutor, QueryPlanner, VoxelQueryRequest};
use std::collections::BTreeMap;
use std::sync::Arc;

fn hex32(bytes: &[u8; 32]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(64);
    for b in bytes {
        out.push(HEX[(b >> 4) as usize] as char);
        out.push(HEX[(b & 0xf) as usize] as char);
    }
    out
}

fn approved_snapshot(label: &str, capabilities: &[&str]) -> Arc<VoxelConfigSnapshot> {
    let names: Vec<String> = capabilities.iter().map(|s| (*s).to_string()).collect();
    let cfg = VoxelConfigInput {
        schema_id: "config-table",
        host_capability_schema_id: "host-capability",
        config_hash: hex32(&sha256(label.as_bytes())),
        host_capability: HostCapabilitySet::from_names(names.clone()),
        start_capabilities: names,
        key_material: None,
    };
    VoxelConfigSnapshot::load(&cfg).expect("valid voxel config")
}

fn stamp(world_id: &str, context: &str, generation: u64, world_revision: u64) -> RevisionStamp {
    RevisionStamp {
        schema_id: REVISION_STAMP_SCHEMA,
        world_id: world_id.to_string(),
        context_id: context.to_string(),
        generation,
        world_revision,
        section_revision_set: BTreeMap::new(),
    }
}

fn dummy_payload(bytes: &[u8]) -> SectionPayload {
    SectionPayload::from_pages([SectionPage::new(
        "Dense",
        "None",
        bytes.to_vec(),
        sha256(bytes),
    )])
    .expect("valid dense uncompressed page")
}

fn four_state_root(world_id: &str, context: &str, generation: u64) -> PublishedStateRoot {
    let mut builder = SectionDirectoryBuilder::new();
    builder
        .insert("s:0:0:0", SectionSlot::ready(dummy_payload(b"ready-bytes")))
        .expect("Ready");
    builder
        .insert("s:1:0:0", SectionSlot::unchanged())
        .expect("Unchanged");
    builder
        .insert("s:2:0:0", SectionSlot::pending())
        .expect("Pending");
    builder
        .insert("s:3:0:0", SectionSlot::unavailable())
        .expect("Unavailable");
    PublishedStateRoot::new(
        stamp(world_id, context, generation, 0),
        builder.freeze(),
        DirtyFrontier::new(world_id, generation).expect("world id"),
    )
}

fn authority(
    label: &str,
    world_id: &str,
    context: &str,
    generation: u64,
    initial: PublishedStateRoot,
) -> PublicationAuthority {
    let pins = PinRegistry::from_approved_snapshot(
        approved_snapshot(label, &["Native", "ReferenceVoxel"]),
        16,
        context,
        generation,
    );
    PublicationAuthority::new(world_id, context, generation, pins, initial)
        .expect("initial root matches authority")
}

fn request(sections: &[&str]) -> VoxelQueryRequest {
    VoxelQueryRequest {
        query_id: "q-1".to_string(),
        world_id: "world-a".to_string(),
        context: "ctx-1".to_string(),
        section_ids: sections.iter().map(|c| (*c).to_string()).collect(),
        cancel: false,
    }
}

#[test]
fn four_presence_states_and_absent_id_map_without_load() {
    assert_eq!(
        SECTION_PRESENCE,
        ["Ready", "Unchanged", "Pending", "Unavailable"]
    );

    let snap = approved_snapshot("r00081-missing", &["Native", "ReferenceVoxel"]);
    let planner = QueryPlanner::from_approved_snapshot(snap.clone(), 8).expect("planner");
    let auth = authority(
        "r00081-missing-view",
        "world-a",
        "ctx-1",
        1,
        four_state_root("world-a", "ctx-1", 1),
    );
    let view = auth.capture();
    let dir_before = sha256(format!("{:?}", view.directory()).as_bytes());
    let root_before = view.root().identity();

    let plan = planner
        .plan(
            &request(&["s:4:0:0", "s:3:0:0", "s:2:0:0", "s:1:0:0", "s:0:0:0"]),
            &view,
            snap.as_ref(),
        )
        .expect("plan five ids including absent");
    assert_eq!(
        plan.canonical_sections(),
        &[
            "s:0:0:0".to_string(),
            "s:1:0:0".to_string(),
            "s:2:0:0".to_string(),
            "s:3:0:0".to_string(),
            "s:4:0:0".to_string(),
        ]
    );

    let outcome = QueryExecutor::execute(&plan, &view).expect("four-state execute");
    assert_eq!(outcome.items().len(), 5);
    assert_eq!(outcome.evidence().budget_used(), 5);
    assert_eq!(outcome.evidence().read_stamp(), view.stamp());

    let expected = [
        ("s:0:0:0", "Ready", true),
        ("s:1:0:0", "Unchanged", false),
        ("s:2:0:0", "Pending", false),
        ("s:3:0:0", "Unavailable", false),
        ("s:4:0:0", "Unchanged", false),
    ];
    for (item, (id, presence, ready)) in outcome.items().iter().zip(expected) {
        assert_eq!(item.section_id(), id);
        assert_eq!(item.presence(), presence);
        assert!(
            SECTION_PRESENCE.contains(&item.presence()),
            "presence {} must be interned SECTION_PRESENCE",
            item.presence()
        );
        assert_eq!(item.presence() == "Ready", ready);
        if ready {
            item.schema_id().expect("Ready schema_id");
        } else {
            assert_eq!(item.schema_id(), None);
        }
    }

    let missing = outcome.evidence().missing_states();
    assert_eq!(missing.len(), 4);
    assert_eq!(missing[0].section_id(), "s:1:0:0");
    assert_eq!(missing[0].presence(), "Unchanged");
    assert_eq!(missing[1].section_id(), "s:2:0:0");
    assert_eq!(missing[1].presence(), "Pending");
    assert_eq!(missing[2].section_id(), "s:3:0:0");
    assert_eq!(missing[2].presence(), "Unavailable");
    assert_eq!(missing[3].section_id(), "s:4:0:0");
    assert_eq!(missing[3].presence(), "Unchanged");

    assert_eq!(
        sha256(format!("{:?}", view.directory()).as_bytes()),
        dir_before,
        "execute must not load or mutate the directory"
    );
    assert_eq!(view.root().identity(), root_before);
}
