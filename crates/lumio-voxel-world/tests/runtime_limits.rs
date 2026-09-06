//! Regression coverage for host budgets using real structured mutations.

use lumio_voxel_contracts::sha256_hex;
use lumio_voxel_domain::block::{BlockId, CellOffset};
use lumio_voxel_domain::config_snapshot::{
    HostCapabilitySet, VoxelConfigInput, VoxelConfigSnapshot,
};
use lumio_voxel_domain::publication::PublishedStateRoot;
use lumio_voxel_domain::revision::WorldRevision;
use lumio_voxel_domain::section::{
    SectionDeltaBuilder, SectionDirectoryBuilder, SectionPayload, SectionSlot, SectionStorage,
};
use lumio_voxel_ops::async_support::{OriginEnvelope, OriginToken};
use lumio_voxel_ops::mutation::{MutationEntry, MutationRequest};
use lumio_voxel_world::world::{
    VoxelWorld, WorldCommand, WorldConfigAdapter, WorldDescriptor, WorldLimits, WorldRouter,
};
use std::collections::BTreeMap;
use std::sync::Arc;

fn snapshot() -> Arc<VoxelConfigSnapshot> {
    VoxelConfigSnapshot::load(&VoxelConfigInput {
        schema_id: "config-table",
        host_capability_schema_id: "host-capability",
        config_hash: sha256_hex(b"runtime-limits-regression"),
        host_capability: HostCapabilitySet::from_names(["Native", "ReferenceVoxel"]),
        start_capabilities: vec!["Native".into(), "ReferenceVoxel".into()],
        key_material: None,
    })
    .expect("config")
}

fn descriptor() -> WorldDescriptor {
    WorldDescriptor {
        role: "Authority".into(),
        world_context_id: "limits-context".into(),
        capabilities: vec!["Native".into(), "ReferenceVoxel".into()],
        config: WorldConfigAdapter {
            world_id: "limits-world".into(),
        },
    }
}

fn origin(world: &VoxelWorld, id: &str) -> OriginToken {
    let guard = world.generation_guard();
    OriginToken::try_new(
        guard.world_context_id(),
        guard.generation(),
        id,
        0,
        BTreeMap::new(),
        "VoxelCommit",
    )
    .expect("origin")
}

fn start_and_seed(world: &mut VoxelWorld) {
    for (event, to) in [
        ("Initialize", "Initialized"),
        ("Prime", "Ready"),
        ("Start", "Running"),
    ] {
        let origin = origin(world, event);
        world
            .endpoint()
            .admit(WorldCommand::Lifecycle { event, to, origin })
            .expect("start");
    }
    let before = world.publication_authority().capture();
    let mut stamp = before.stamp().clone();
    stamp.world_revision = 1;
    stamp.section_revision_set.insert("s:0:0:0".into(), 1);
    let mut directory = SectionDirectoryBuilder::new();
    let payload = SectionPayload::from_storage(SectionStorage::uniform(BlockId::from_raw(0)))
        .expect("structured payload");
    directory
        .insert("s:0:0:0", SectionSlot::ready(payload))
        .expect("section");
    let root = PublishedStateRoot::new(stamp, directory.freeze(), before.dirty_frontier().clone());
    let replacement = SectionDeltaBuilder::new(before.directory())
        .freeze()
        .expect("delta");
    let mut prepared = world
        .publication_authority()
        .prepare(WorldRevision::from_raw(1), root, replacement)
        .expect("seed prepare");
    world
        .publication_authority()
        .publish_once(prepared.seal().expect("seed seal"))
        .expect("seed publish");
}

fn request(world: &VoxelWorld, number: u32) -> MutationRequest {
    let view = world.publication_authority().capture();
    MutationRequest {
        txn_id: format!("limits-txn-{number}"),
        world_id: view.stamp().world_id.clone(),
        generation: view.stamp().generation,
        entries: vec![MutationEntry::new(
            "s:0:0:0",
            CellOffset::new(0).expect("offset"),
            BlockId::from_raw(number),
            view.stamp().section_revision_set["s:0:0:0"],
        )],
    }
}

fn envelope(world: &VoxelWorld, request: MutationRequest) -> OriginEnvelope<MutationRequest> {
    OriginEnvelope {
        origin: origin(world, &request.txn_id),
        config_hash: world.config_hash().to_string(),
        payload: request,
    }
}

#[test]
fn default_world_accepts_more_than_sixteen_distinct_real_mutations() {
    let mut world = VoxelWorld::create(descriptor(), snapshot()).expect("world");
    start_and_seed(&mut world);
    let old = world.publication_authority().capture();
    let first_request = request(&world, 1);
    let mut first_receipt = None;
    for number in 1..=64 {
        let command = envelope(&world, request(&world, number));
        let prepared = WorldRouter::prepare(&mut world, command)
            .unwrap_or_else(|error| panic!("transaction {number}: {}", error.error_id()));
        let receipt = WorldRouter::commit(&mut world, prepared)
            .expect("commit")
            .payload;
        if number == 1 {
            first_receipt = Some(receipt);
        }
    }
    let current = world.publication_authority().capture();
    let read_cell = |view: &lumio_voxel_domain::publication::PublishedReadView| {
        view.directory()
            .lookup("s:0:0:0")
            .expect("key")
            .expect("slot")
            .payload()
            .expect("payload")
            .storage()
            .expect("storage")
            .read(CellOffset::new(0).expect("offset"))
    };
    assert_eq!(read_cell(&old), BlockId::from_raw(0));
    assert_eq!(read_cell(&current), BlockId::from_raw(64));
    assert_eq!(current.stamp().world_revision, 65);
    let before_replay = current.root().identity();
    let replay = envelope(&world, first_request);
    let prepared = WorldRouter::prepare(&mut world, replay).expect("duplicate prepare");
    let replayed = WorldRouter::commit(&mut world, prepared).expect("duplicate commit");
    assert_eq!(replayed.payload, first_receipt.expect("first receipt"));
    assert_eq!(
        world.publication_authority().capture().root().identity(),
        before_replay
    );
}

#[test]
fn explicit_receipt_budget_rejects_new_work_but_preserves_duplicate_receipts() {
    let limits = WorldLimits {
        max_receipts: 2,
        ..WorldLimits::default()
    };
    let mut world = VoxelWorld::create_with_limits(descriptor(), snapshot(), limits).unwrap();
    assert_eq!(world.limits(), limits);
    start_and_seed(&mut world);
    let original = request(&world, 1);
    let mut first = None;
    for number in 1..=2 {
        let command = envelope(&world, request(&world, number));
        let prepared = WorldRouter::prepare(&mut world, command).unwrap();
        let receipt = WorldRouter::commit(&mut world, prepared).unwrap().payload;
        if number == 1 {
            first = Some(receipt);
        }
    }
    assert_eq!(world.retained_receipt_count(), 2);
    let before = world.publication_authority().capture();
    let rejected = envelope(&world, request(&world, 3));
    let error = WorldRouter::prepare(&mut world, rejected).expect_err("capacity must refuse");
    assert_eq!(error.error_id(), "BudgetExceeded");
    assert_eq!(
        world.publication_authority().capture().root().identity(),
        before.root().identity()
    );
    assert_eq!(world.retained_receipt_count(), 2);
    let replay = envelope(&world, original.clone());
    let prepared = WorldRouter::prepare(&mut world, replay).expect("replay while full");
    assert_eq!(
        WorldRouter::commit(&mut world, prepared).unwrap().payload,
        first.unwrap()
    );
    let mut forged = original;
    forged.entries[0] = MutationEntry::new(
        "s:0:0:0",
        CellOffset::new(0).unwrap(),
        BlockId::from_raw(999),
        1,
    );
    let forged = envelope(&world, forged);
    assert_eq!(
        WorldRouter::prepare(&mut world, forged)
            .unwrap_err()
            .error_id(),
        "RevisionConflict"
    );
    assert_eq!(
        world.publication_authority().capture().root().identity(),
        before.root().identity()
    );
}

#[test]
fn invalid_limits_are_rejected_at_creation() {
    for limits in [
        WorldLimits {
            max_pinned_revisions: 0,
            ..WorldLimits::default()
        },
        WorldLimits {
            max_pinned_revisions: 1,
            ..WorldLimits::default()
        },
        WorldLimits {
            max_receipts: 0,
            ..WorldLimits::default()
        },
        WorldLimits {
            max_query_sections: 0,
            ..WorldLimits::default()
        },
    ] {
        let error = VoxelWorld::create_with_limits(descriptor(), snapshot(), limits).unwrap_err();
        assert_eq!(error.error_id(), "BudgetExceeded");
    }
}

#[test]
fn local_mutation_preserves_unrelated_presence_entries_without_revisions() {
    let mut world = VoxelWorld::create(descriptor(), snapshot()).unwrap();
    start_and_seed(&mut world);
    let before = world.publication_authority().capture();
    let mut directory = SectionDirectoryBuilder::from_root(before.directory());
    directory.insert("s:1:0:0", SectionSlot::pending()).unwrap();
    directory
        .insert("s:2:0:0", SectionSlot::unchanged())
        .unwrap();
    directory
        .insert("s:3:0:0", SectionSlot::unavailable())
        .unwrap();
    let mut stamp = before.stamp().clone();
    stamp.world_revision += 1;
    let next = WorldRevision::from_raw(stamp.world_revision);
    let root = PublishedStateRoot::new(stamp, directory.freeze(), before.dirty_frontier().clone());
    let replacement = SectionDeltaBuilder::new(before.directory())
        .freeze()
        .unwrap();
    let mut prepared = world
        .publication_authority()
        .prepare(next, root, replacement)
        .unwrap();
    world
        .publication_authority()
        .publish_once(prepared.seal().unwrap())
        .unwrap();
    let old = world.publication_authority().capture();
    let command = envelope(&world, request(&world, 1));
    let prepared = WorldRouter::prepare(&mut world, command).unwrap();
    WorldRouter::commit(&mut world, prepared).unwrap();
    let current = world.publication_authority().capture();
    for (key, presence) in [
        ("s:1:0:0", "Pending"),
        ("s:2:0:0", "Unchanged"),
        ("s:3:0:0", "Unavailable"),
    ] {
        assert_eq!(
            current.directory().lookup(key).unwrap().unwrap().presence(),
            presence
        );
        assert_eq!(
            old.directory().lookup(key).unwrap(),
            current.directory().lookup(key).unwrap()
        );
        assert!(!current.stamp().section_revision_set.contains_key(key));
    }
}

#[test]
fn port_constructor_honors_query_section_budget() {
    use lumio_voxel_ops::query::VoxelQueryRequest;
    use lumio_voxel_world::port::VoxelWorldPortAdapter;
    let limits = WorldLimits {
        max_query_sections: 1,
        ..WorldLimits::default()
    };
    let mut world =
        VoxelWorldPortAdapter::create_world_with_limits(descriptor(), snapshot(), limits).unwrap();
    start_and_seed(&mut world);
    let state = world.state_view();
    let mut query = OriginEnvelope {
        origin: origin(&world, "budgeted-query"),
        config_hash: world.config_hash().to_string(),
        payload: VoxelQueryRequest {
            query_id: "budgeted-query".into(),
            world_id: state.world_id().into(),
            context: state.world_context_id().into(),
            section_ids: vec!["s:0:0:0".into(), "s:1:0:0".into()],
            cancel: false,
        },
    };
    let before = world.publication_authority().capture().root().identity();
    assert_eq!(
        WorldRouter::query(&mut world, query.clone())
            .unwrap_err()
            .error_id(),
        "BudgetExceeded"
    );
    query.payload.section_ids.pop();
    WorldRouter::query(&mut world, query).expect("one section fits");
    assert_eq!(
        world.publication_authority().capture().root().identity(),
        before
    );
}

#[test]
fn pin_budget_recovers_after_old_read_view_is_released() {
    let limits = WorldLimits {
        max_pinned_revisions: 2,
        ..WorldLimits::default()
    };
    let mut world = VoxelWorld::create_with_limits(descriptor(), snapshot(), limits).unwrap();
    start_and_seed(&mut world);
    let old = world.publication_authority().capture();
    let first = envelope(&world, request(&world, 1));
    let prepared = WorldRouter::prepare(&mut world, first).unwrap();
    WorldRouter::commit(&mut world, prepared).unwrap();
    let before = world.publication_authority().capture().root().identity();
    let second = request(&world, 2);
    let command = envelope(&world, second.clone());
    let prepared = WorldRouter::prepare(&mut world, command).unwrap();
    assert_eq!(
        WorldRouter::commit(&mut world, prepared)
            .unwrap_err()
            .error_id(),
        "BudgetExceeded"
    );
    assert_eq!(
        world.publication_authority().capture().root().identity(),
        before
    );
    let abort = envelope(&world, second.clone());
    WorldRouter::abort(&mut world, abort).unwrap();
    drop(old);
    let retry = envelope(&world, second);
    let prepared = WorldRouter::prepare(&mut world, retry).unwrap();
    WorldRouter::commit(&mut world, prepared).expect("released pin restores admission");
    assert_eq!(world.retained_receipt_count(), 2);
}
