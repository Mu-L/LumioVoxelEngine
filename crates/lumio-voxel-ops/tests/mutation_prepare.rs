//! R-00096: Prepare has no visible side effects.

use lumio_voxel_contracts::sha256;
use lumio_voxel_contracts::voxel_world as vw;
use lumio_voxel_domain::block::{BlockId, CellOffset};
use lumio_voxel_domain::config_snapshot::{
    HostCapabilitySet, VoxelConfigInput, VoxelConfigSnapshot,
};
use lumio_voxel_domain::publication::{
    PublicationAuthority, PublishedReadView, PublishedStateRoot,
};
use lumio_voxel_domain::revision::{
    PinRegistry, RevisionAllocator, RevisionStamp, WorldRevision, to_revision_stamp,
};
use lumio_voxel_domain::section::{
    DirtyFrontier, SectionDirectoryBuilder, SectionDirectoryRoot, SectionPayload, SectionSlot,
    SectionStorage,
};
use lumio_voxel_ops::mutation::{
    LookupOutcome, MutationEntry, MutationRequest, PreparedMutation, ReceiptLedger,
    canonical_fingerprint, prepare,
};
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

fn approved_snapshot(label: &str) -> Arc<VoxelConfigSnapshot> {
    let cfg = VoxelConfigInput {
        schema_id: "config-table",
        host_capability_schema_id: "host-capability",
        config_hash: hex32(&sha256(label.as_bytes())),
        host_capability: HostCapabilitySet::from_names(["Native", "ReferenceVoxel"]),
        start_capabilities: vec!["Native".into(), "ReferenceVoxel".into()],
        key_material: None,
    };
    VoxelConfigSnapshot::load(&cfg).expect("valid voxel config")
}

fn world_rev(n: u64) -> WorldRevision {
    let mut alloc = RevisionAllocator::new();
    for _ in 0..n {
        alloc.reserve_world().unwrap().abandon();
    }
    let mut reserved = alloc.reserve_world().unwrap();
    reserved.finalize().unwrap()
}

fn stamp_at(
    world_id: &str,
    context_id: &str,
    generation: u64,
    world_rev_n: u64,
    sections: &[(&str, u64)],
) -> RevisionStamp {
    let world = world_rev(world_rev_n);
    let mut pairs = Vec::new();
    for (id, rev) in sections {
        let mut section_alloc = RevisionAllocator::new();
        for _ in 0..*rev {
            section_alloc.reserve_section().unwrap().abandon();
        }
        let mut c = section_alloc.reserve_section().unwrap();
        pairs.push((id.to_string(), c.finalize().unwrap()));
    }
    to_revision_stamp(world_id, context_id, generation, world, &pairs)
}

fn payload(bytes: &[u8]) -> SectionPayload {
    let digest = sha256(bytes);
    SectionPayload::from_storage(SectionStorage::uniform(BlockId::from_raw(
        u32::from_le_bytes(digest[..4].try_into().unwrap()),
    )))
    .expect("valid dense uncompressed page")
}

fn directory_ready() -> SectionDirectoryRoot {
    let mut builder = SectionDirectoryBuilder::new();
    builder
        .insert("s:0:0:0", SectionSlot::ready(payload(b"base-ready")))
        .expect("canonical section id");
    builder
        .insert("s:1:0:0", SectionSlot::unchanged())
        .expect("canonical section id");
    builder
        .insert("s:2:0:0", SectionSlot::pending())
        .expect("canonical section id");
    builder
        .insert("s:3:0:0", SectionSlot::unavailable())
        .expect("canonical section id");
    builder.freeze()
}

fn published_world(
    world_id: &str,
    generation: u64,
) -> (PublicationAuthority, Arc<VoxelConfigSnapshot>) {
    let snap = approved_snapshot("r00096-prepare");
    let stamp = stamp_at(
        world_id,
        "ctx-1",
        generation,
        0,
        &[
            ("s:0:0:0", 0),
            ("s:1:0:0", 0),
            ("s:2:0:0", 0),
            ("s:3:0:0", 0),
        ],
    );
    let root = PublishedStateRoot::new(
        stamp,
        directory_ready(),
        DirtyFrontier::new(world_id, generation).expect("world id"),
    );
    let pins = PinRegistry::from_approved_snapshot(snap.clone(), 16, "ctx-1", generation);
    let auth = PublicationAuthority::new(world_id, "ctx-1", generation, pins, root)
        .expect("initial root matches authority");
    (auth, snap)
}

fn request(
    txn_id: &str,
    world_id: &str,
    generation: u64,
    world_revision: u64,
    extra: &[(&str, &str)],
) -> MutationRequest {
    let entries = extra
        .iter()
        .map(|(key, value)| {
            let (section, cell) = key.split_once('/').unwrap_or((key, "0"));
            MutationEntry::new(
                section,
                CellOffset::new(cell.parse().unwrap_or(0)).unwrap(),
                BlockId::from_raw(value.parse().unwrap_or_else(|_| hash_value(value))),
                world_revision,
            )
        })
        .collect();
    MutationRequest {
        txn_id: txn_id.to_string(),
        world_id: world_id.to_string(),
        generation,
        entries,
    }
}

fn hash_value(value: &str) -> u32 {
    let digest = sha256(value.as_bytes());
    u32::from_le_bytes(digest[..4].try_into().unwrap())
}

fn visible_cut(view: &PublishedReadView) -> ([u8; 32], DirtyFrontier) {
    (view.root().identity(), view.dirty_frontier().clone())
}

#[test]
fn failed_precondition_wrong_world_leaves_ledger_vacant_and_root_unchanged() {
    let (auth, snap) = published_world("world-a", 1);
    let mut ledger = ReceiptLedger::from_approved_snapshot(snap, 4).unwrap();
    let view = auth.capture();
    let (hash_before, dirty_before) = visible_cut(&view);
    let req = request("txn-1", "world-b", 1, 0, &[("s:0:0:0", "edit")]);

    let err = prepare(&req, &view, &mut ledger).unwrap_err();
    assert_eq!(err.error_id(), "SessionMismatch");
    assert_eq!(ledger.lookup(&req).unwrap(), LookupOutcome::Vacant);

    let after = auth.capture();
    let (hash_after, dirty_after) = visible_cut(&after);
    assert_eq!(hash_after, hash_before);
    assert_eq!(dirty_after, dirty_before);
    assert_eq!(after.root().identity(), view.root().identity());
}

#[test]
fn successful_prepare_is_move_only_and_does_not_publish() {
    let (auth, snap) = published_world("world-a", 1);
    let mut ledger = ReceiptLedger::from_approved_snapshot(snap, 4).unwrap();
    let view = auth.capture();
    let (hash_before, dirty_before) = visible_cut(&view);
    let req = request("txn-1", "world-a", 1, 0, &[("s:0:0:0", "edit")]);
    let expected_fp = canonical_fingerprint(&req).expect("no duplicate member");
    let config_hash = ledger.config_hash().to_string();

    let token = prepare(&req, &view, &mut ledger).expect("prepare succeeds");
    assert_eq!(token.txn_id(), "txn-1");
    assert_eq!(token.fingerprint(), expected_fp);
    assert_eq!(token.base_identity(), hash_before);
    assert_eq!(token.generation(), 1);
    assert_eq!(token.config_hash(), config_hash);
    match ledger.lookup(&req).unwrap() {
        LookupOutcome::InFlight => {}
        other => panic!("successful prepare may reserve InFlight, got {other:?}"),
    }

    let after = auth.capture();
    assert_eq!(after.root().identity(), hash_before);
    assert_eq!(after.dirty_frontier(), &dirty_before);

    fn take_moved(token: PreparedMutation) -> String {
        token.txn_id().to_string()
    }
    assert_eq!(take_moved(token), "txn-1");
}

#[test]
fn conflict_fingerprint_leaves_first_reservation_receipt_unchanged() {
    let (auth, snap) = published_world("world-a", 1);
    let mut ledger = ReceiptLedger::from_approved_snapshot(snap, 4).unwrap();
    let view = auth.capture();
    let (hash_before, dirty_before) = visible_cut(&view);
    let first = request("txn-1", "world-a", 1, 0, &[("s:0:0:0", "edit-a")]);
    let token = prepare(&first, &view, &mut ledger).expect("first prepare");
    let receipt = b"receipt-first".to_vec();
    ledger
        .finalize(&first, receipt.clone())
        .expect("test harness may finalize via public ledger API");
    drop(token);

    let conflict = request("txn-1", "world-a", 1, 0, &[("s:0:0:0", "edit-b")]);
    let err = prepare(&conflict, &view, &mut ledger).unwrap_err();
    assert_eq!(err.error_id(), "RevisionConflict");
    assert_eq!(
        err.disposition(),
        Some(lumio_voxel_ops::mutation::ReplayDisposition::Conflict)
    );

    match ledger.lookup(&first).unwrap() {
        LookupOutcome::Duplicate { receipt: stored } => assert_eq!(stored, receipt),
        other => panic!("first receipt must be unchanged, got {other:?}"),
    }
    let after = auth.capture();
    assert_eq!(after.root().identity(), hash_before);
    assert_eq!(after.dirty_frontier(), &dirty_before);
}

#[test]
fn failed_stage_invalid_section_id_aborts_without_publish() {
    let (auth, snap) = published_world("world-a", 1);
    let mut ledger = ReceiptLedger::from_approved_snapshot(snap, 4).unwrap();
    let view = auth.capture();
    let (hash_before, dirty_before) = visible_cut(&view);
    let req = request("txn-1", "world-a", 1, 0, &[("s:01:0:0", "edit")]);

    let err = prepare(&req, &view, &mut ledger).unwrap_err();
    // 前导零不是规范写法:契约 key.canonical → unknown_section_key。
    assert_eq!(err.error_id(), vw::UNKNOWN_SECTION_KEY);
    assert_eq!(ledger.lookup(&req).unwrap(), LookupOutcome::Vacant);

    let after = auth.capture();
    assert_eq!(after.root().identity(), hash_before);
    assert_eq!(after.dirty_frontier(), &dirty_before);
}
