//! B0 matrix: drive shipped Artifact / DAG / Revision / Section / Publication / Port APIs.

#![forbid(unsafe_code)]

use crate::crate_dag::{self, FROZEN_CRATES};
use crate::deterministic_executor::{DeterministicExecutor, Schedule};
use crate::reference_harness::VoxelOperation;
use crate::workspace_root_from_manifest;
use lumio_voxel_contracts::sha256;
use lumio_voxel_contracts::voxel_world as vw;
use lumio_voxel_contracts::voxel_world::SECTION_PRESENCE;
use lumio_voxel_domain::config_snapshot::{
    HostCapabilitySet, VoxelConfigInput, VoxelConfigSnapshot,
};
use lumio_voxel_domain::publication::{
    PublicationAuthority, PublishedReadView, PublishedStateRoot,
};
use lumio_voxel_domain::revision::{
    PinRegistry, RetentionFrontier, RevisionAllocator, WorldRevision, to_revision_stamp,
};
use lumio_voxel_domain::section::{
    CoveredSectionAck, DirtyFrontier, DurabilityAckContext, DurabilityAckEvidence,
    SectionDeltaBuilder, SectionDirectoryBuilder, SectionPage, SectionPayload, SectionSlot,
};
use lumio_voxel_ops::SNAPSHOT_FEATURE;
use lumio_voxel_world::port::{PORT_RUST_TYPE, PORT_SCHEMA, VoxelWorldPortAdapter};
use lumio_voxel_world::world::{
    VoxelWorld, WorldConfigAdapter, WorldDescriptor, intern_local_embedded_pair,
};
use std::sync::{Arc, Barrier};
use std::thread;

pub const MATRIX_ROWS: usize = 9;

const SEED_A: u64 = 0x00A1_1CE0;
const SEED_B: u64 = 0x00B0_5EED;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct B0CaseResult {
    pub id: &'static str,
    pub name: &'static str,
    pub ok: bool,
    pub detail: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct B0VerificationReport {
    pub commit: String,
    pub dag_ok: bool,
    pub cases: Vec<B0CaseResult>,
}

impl B0VerificationReport {
    pub fn all_ok(&self) -> bool {
        self.dag_ok && self.cases.len() == MATRIX_ROWS && self.cases.iter().all(|case| case.ok)
    }
}

pub fn run_b0_matrix() -> B0VerificationReport {
    let cases = vec![
        case_frozen_crate_dag(),
        case_revision_monotonic(),
        case_pin_reclaim(),
        case_section_four_state(),
        case_dirty_frontier_pure(),
        case_publication_old_or_new(),
        case_dual_voxel_world(),
        case_port_schema_intern(),
        case_deterministic_executor(),
    ];
    let dag_ok = cases.first().is_some_and(|c| c.ok);
    B0VerificationReport {
        commit: git_head(),
        dag_ok,
        cases,
    }
}

pub fn case_frozen_crate_dag() -> B0CaseResult {
    wrap("2", "frozen crate DAG", frozen_crate_dag)
}

pub fn case_revision_monotonic() -> B0CaseResult {
    wrap("3", "revision monotonic + abandon hole", revision_monotonic)
}

pub fn case_pin_reclaim() -> B0CaseResult {
    wrap("4", "pin / reclaim", pin_reclaim)
}

pub fn case_section_four_state() -> B0CaseResult {
    wrap(
        "5",
        "section four-state + illegal convert",
        section_four_state,
    )
}

pub fn case_dirty_frontier_pure() -> B0CaseResult {
    wrap(
        "6",
        "DirtyFrontier::covered_by is pure",
        dirty_frontier_pure,
    )
}

pub fn case_publication_old_or_new() -> B0CaseResult {
    wrap(
        "7",
        "publication capture old-or-new",
        publication_old_or_new,
    )
}

pub fn case_dual_voxel_world() -> B0CaseResult {
    wrap(
        "8",
        "dual VoxelWorld independent captures",
        dual_voxel_world,
    )
}

pub fn case_port_schema_intern() -> B0CaseResult {
    wrap("9", "VoxelWorldPortAdapter intern", port_schema_intern)
}

pub fn case_deterministic_executor() -> B0CaseResult {
    wrap(
        "10",
        "DeterministicExecutor two seeds",
        deterministic_two_seeds,
    )
}

fn wrap(id: &'static str, name: &'static str, f: fn() -> Result<String, String>) -> B0CaseResult {
    match f() {
        Ok(detail) => B0CaseResult {
            id,
            name,
            ok: true,
            detail,
        },
        Err(detail) => B0CaseResult {
            id,
            name,
            ok: false,
            detail,
        },
    }
}

fn frozen_crate_dag() -> Result<String, String> {
    if FROZEN_CRATES.len() != 6 {
        return Err(format!("FROZEN_CRATES len {}", FROZEN_CRATES.len()));
    }
    let legal = crate_dag::parse_fixture_graph(include_str!(
        "../../../tools/architecture/fixtures/dag-legal.json"
    ));
    let legal_v = crate_dag::violations(&legal);
    if !legal_v.is_empty() {
        return Err(format!("legal fixture violations: {legal_v:?}"));
    }
    let extra = crate_dag::parse_fixture_graph(include_str!(
        "../../../tools/architecture/fixtures/dag-forbidden-persistence.json"
    ));
    let extra_v = crate_dag::violations(&extra);
    let forbidden = extra_v.iter().any(|s| s.contains("禁止的额外 crate 名"));
    let unlisted = extra_v
        .iter()
        .any(|s| s.contains("lumio-voxel-persistence"));
    if !forbidden || !unlisted {
        return Err(format!(
            "expected forbidden extra crate token, got {extra_v:?}"
        ));
    }
    Ok(format!(
        "legal violations empty; extra crate token rejected ({})",
        extra_v.join("; ")
    ))
}

fn revision_monotonic() -> Result<String, String> {
    let mut alloc = RevisionAllocator::new();
    let mut w0 = alloc
        .reserve_world()
        .map_err(|err| format!("reserve_world 0: {}", err.error_id()))?;
    let mut hole = alloc
        .reserve_world()
        .map_err(|err| format!("reserve_world hole: {}", err.error_id()))?;
    hole.abandon();
    let hole_err = hole
        .finalize()
        .err()
        .ok_or_else(|| "abandoned reservation finalized".to_string())?;
    if hole_err.error_id() != "InvalidHandle" {
        return Err(format!("abandon error {}", hole_err.error_id()));
    }
    let mut w2 = alloc
        .reserve_world()
        .map_err(|err| format!("reserve_world after hole: {}", err.error_id()))?;
    if w2.value().value() != 2 {
        return Err(format!("abandoned 1 reused as {}", w2.value().value()));
    }
    let fw0 = w0
        .finalize()
        .map_err(|err| format!("finalize 0: {}", err.error_id()))?;
    let fw2 = w2
        .finalize()
        .map_err(|err| format!("finalize 2: {}", err.error_id()))?;
    if fw2 <= fw0 {
        return Err("world revisions are not monotonic".into());
    }
    let double = w0
        .finalize()
        .err()
        .ok_or_else(|| "double finalize succeeded".to_string())?;
    if double.error_id() != "HandleDoubleRelease" {
        return Err(format!("double finalize {}", double.error_id()));
    }
    let mut c0 = alloc
        .reserve_section()
        .map_err(|err| format!("reserve_section: {}", err.error_id()))?;
    let fc0 = c0
        .finalize()
        .map_err(|err| format!("finalize section: {}", err.error_id()))?;
    if fc0.value() != 0 {
        return Err("section domain must start at 0".into());
    }
    Ok("world 0 then hole 1 then 2; section domain independent".into())
}

fn pin_reclaim() -> Result<String, String> {
    let snap = approved_snapshot("b0-pin");
    let registry = PinRegistry::from_approved_snapshot(Arc::clone(&snap), 1, "ctx-b0-pin", 4);
    if registry.config_hash() != snap.config_hash() {
        return Err("pin registry config_hash != snapshot".into());
    }
    let stamp = stamp_at("world-b0-pin", "ctx-b0-pin", 4, 0, &[("s:0:0:0", 0)]);
    let held = registry
        .try_pin(stamp.clone())
        .map_err(|err| format!("try_pin: {}", err.error_id()))?;
    if held.stamp() != &stamp {
        return Err("pin stamp mismatch".into());
    }
    let clone = held.clone();
    let over = registry
        .try_pin(stamp.clone())
        .err()
        .ok_or_else(|| "second pin at capacity succeeded".to_string())?;
    if over.error_id() != "BudgetExceeded" {
        return Err(format!("capacity error {}", over.error_id()));
    }
    drop(clone);
    let still = registry
        .try_pin(stamp.clone())
        .err()
        .ok_or_else(|| "clone drop reclaimed early".to_string())?;
    if still.error_id() != "BudgetExceeded" {
        return Err("refcount clone/drop advanced reclaim".into());
    }
    drop(held);
    if RetentionFrontier::from_registry(&registry)
        .oldest_live()
        .is_some()
    {
        return Err("live pin remains after last drop".into());
    }
    let reused = registry
        .try_pin(stamp.clone())
        .map_err(|err| format!("reclaim: {}", err.error_id()))?;
    if reused.stamp() != &stamp {
        return Err("reclaimed pin stamp mismatch".into());
    }
    Ok("from_approved_snapshot; last drop reclaims slot".into())
}

fn section_four_state() -> Result<String, String> {
    if SECTION_PRESENCE != ["Ready", "Unchanged", "Pending", "Unavailable"] {
        return Err(format!("SECTION_PRESENCE {SECTION_PRESENCE:?}"));
    }
    let ready = SectionSlot::ready(payload(b"b0-ready"));
    let unchanged = SectionSlot::unchanged();
    let pending = SectionSlot::pending();
    let unavailable = SectionSlot::unavailable();
    let names = [
        ready.presence(),
        unchanged.presence(),
        pending.presence(),
        unavailable.presence(),
    ];
    if names.as_slice() != SECTION_PRESENCE {
        return Err(format!("slot presence {names:?}"));
    }
    for name in names {
        intern_presence(name)?;
        if !SECTION_PRESENCE
            .iter()
            .any(|item| std::ptr::eq(*item, name))
        {
            return Err(format!("{name} is not interned from SECTION_PRESENCE"));
        }
    }
    let before = unavailable.clone();
    let err = unavailable
        .try_convert("Ready", None)
        .err()
        .ok_or_else(|| "Unavailable -> Ready succeeded".to_string())?;
    if err.error_id() != vw::SECTION_UNAVAILABLE {
        return Err(format!("illegal convert {}", err.error_id()));
    }
    if unavailable != before || unavailable.presence() != "Unavailable" {
        return Err("illegal convert mutated the source slot".into());
    }
    let mut builder = SectionDirectoryBuilder::new();
    builder
        .insert("s:0:0:0", unavailable.clone())
        .map_err(|err| format!("insert: {}", err.error_id()))?;
    let frozen = builder.freeze();
    let convert_err = builder
        .convert("s:0:0:0", "Ready", None)
        .err()
        .ok_or_else(|| "directory convert succeeded".to_string())?;
    if convert_err.error_id() != vw::SECTION_UNAVAILABLE {
        return Err(format!("directory convert {}", convert_err.error_id()));
    }
    let looked = frozen
        .lookup("s:0:0:0")
        .map_err(|err| err.error_id().to_string())?
        .ok_or_else(|| "frozen slot missing".to_string())?;
    if looked.presence() != "Unavailable" {
        return Err("frozen root mutated by failed convert".into());
    }
    Ok("four SECTION_PRESENCE names interned; illegal convert leaves slot".into())
}

fn dirty_frontier_pure() -> Result<String, String> {
    let frontier = DirtyFrontier::new("world-b0-dirty", 7)
        .map_err(|err| format!("new frontier: {}", err.error_id()))?;
    let dirty = frontier
        .record("s:0:0:0", 5, "AuthoritativeWrite")
        .map_err(|err| format!("record: {}", err.error_id()))?;
    if frontier
        .latest_revision("s:0:0:0")
        .map_err(|err| err.error_id().to_string())?
        .is_some()
    {
        return Err("record mutated the original frontier".into());
    }
    let ack = DurabilityAckEvidence {
        kind: "DurabilityAck".to_string(),
        world_id: "world-b0-dirty".to_string(),
        context: DurabilityAckContext {
            context_id: "ctx-b0-dirty".to_string(),
            generation: 7,
        },
        covered_world_revision: 8,
        covered_sections: vec![CoveredSectionAck {
            section_id: "s:0:0:0".to_string(),
            up_to_section_revision: 5,
        }],
    };
    let covered = dirty
        .covered_by(&ack)
        .map_err(|err| format!("covered_by: {}", err.error_id()))?;
    if !covered
        .contains("s:0:0:0")
        .map_err(|err| err.error_id().to_string())?
    {
        return Err("ack did not cover c:0:0:0".into());
    }
    if dirty
        .latest_revision("s:0:0:0")
        .map_err(|err| err.error_id().to_string())?
        != Some(5)
    {
        return Err("covered_by cleared the frontier".into());
    }
    let cleared = dirty.except_covered(&covered);
    if cleared
        .latest_revision("s:0:0:0")
        .map_err(|err| err.error_id().to_string())?
        .is_some()
    {
        return Err("except_covered did not drop covered entry".into());
    }
    if dirty
        .latest_revision("s:0:0:0")
        .map_err(|err| err.error_id().to_string())?
        != Some(5)
    {
        return Err("except_covered mutated self".into());
    }
    Ok("covered_by is pure; except_covered returns a new frontier".into())
}

fn publication_old_or_new() -> Result<String, String> {
    let initial = root_at(
        "world-b0-pub",
        "ctx-b0-pub",
        1,
        0,
        SectionSlot::unchanged(),
        None,
    );
    let auth = Arc::new(authority(
        "b0-pub",
        "world-b0-pub",
        "ctx-b0-pub",
        1,
        initial,
    )?);
    let before = auth.capture();
    assert_cut(&before, 0, "Unchanged")?;
    let hash0 = before.root().identity();

    let start = Arc::new(Barrier::new(5));
    let mut readers = Vec::new();
    for _ in 0..4 {
        let auth = Arc::clone(&auth);
        let start = Arc::clone(&start);
        readers.push(thread::spawn(move || {
            start.wait();
            let mut seen = Vec::new();
            for _ in 0..64 {
                let view = auth.capture();
                cut_identity(&view)?;
                seen.push((view.stamp().world_revision, view.root().identity()));
            }
            Ok::<_, String>(seen)
        }));
    }
    let writer = {
        let auth = Arc::clone(&auth);
        let start = Arc::clone(&start);
        thread::spawn(move || {
            start.wait();
            let mut prepared = auth
                .prepare(
                    world_rev(1),
                    root_at(
                        "world-b0-pub",
                        "ctx-b0-pub",
                        1,
                        1,
                        SectionSlot::ready(payload(b"b0-pub-1")),
                        Some("mutation"),
                    ),
                    empty_replacement(auth.capture().directory()),
                )
                .map_err(|err| format!("prepare: {}", err.error_id()))?;
            let token = prepared
                .seal()
                .map_err(|err| format!("seal: {}", err.error_id()))?;
            auth.publish_once(token)
                .map_err(|err| format!("publish_once: {}", err.error_id()))
        })
    };
    let published = writer.join().map_err(join_err)??;
    assert_cut(&published, 1, "Ready")?;
    let hash1 = published.root().identity();
    if hash1 == hash0 {
        return Err("publish did not change identity".into());
    }
    assert_cut(&before, 0, "Unchanged")?;
    if before.root().identity() != hash0 {
        return Err("pre-publish capture mixed with the new cut".into());
    }
    let mut saw_old = false;
    let mut saw_new = false;
    for handle in readers {
        let seen = handle.join().map_err(join_err)??;
        for (rev, identity) in seen {
            match rev {
                0 => {
                    if identity != hash0 {
                        return Err("revision 0 with mixed identity".into());
                    }
                    saw_old = true;
                }
                1 => {
                    if identity != hash1 {
                        return Err("revision 1 with mixed identity".into());
                    }
                    saw_new = true;
                }
                other => return Err(format!("mixed stamp revision {other}")),
            }
        }
    }
    let _ = (saw_old, saw_new);
    let after = auth.capture();
    assert_cut(&after, 1, "Ready")?;
    Ok("concurrent capture saw complete old or complete new identity".into())
}

fn dual_voxel_world() -> Result<String, String> {
    if !SNAPSHOT_FEATURE {
        return Err("lumio-voxel-ops snapshot feature is off".into());
    }
    let (authority_role, replica_role) = intern_local_embedded_pair("Authority", "Replica")
        .map_err(|err| format!("intern_local_embedded_pair: {}", err.error_id()))?;
    let authority = create_world(authority_role, "ctx-b0-auth", "world-b0-auth", "b0-auth")?;
    let replica = create_world(replica_role, "ctx-b0-repl", "world-b0-repl", "b0-repl")?;
    if authority.generation_guard().generation() == replica.generation_guard().generation() {
        return Err("worlds share instance generation".into());
    }
    let id_auth_0 = authority
        .publication_authority()
        .capture()
        .root()
        .identity();
    let id_repl_0 = replica.publication_authority().capture().root().identity();
    if id_auth_0 == id_repl_0 {
        return Err("independent worlds published the same identity".into());
    }
    let view = authority.state_view();
    let before = authority.publication_authority().capture();
    let mut prepared = authority
        .publication_authority()
        .prepare(
            world_rev(1),
            root_at(
                view.world_id(),
                view.world_context_id(),
                view.instance_generation(),
                1,
                SectionSlot::ready(payload(b"b0-auth-cut")),
                Some("mutation"),
            ),
            empty_replacement(before.directory()),
        )
        .map_err(|err| format!("prepare authority: {}", err.error_id()))?;
    let token = prepared
        .seal()
        .map_err(|err| format!("seal authority: {}", err.error_id()))?;
    authority
        .publication_authority()
        .publish_once(token)
        .map_err(|err| format!("publish authority: {}", err.error_id()))?;
    let id_auth_1 = authority
        .publication_authority()
        .capture()
        .root()
        .identity();
    let id_repl_1 = replica.publication_authority().capture().root().identity();
    if id_auth_1 == id_auth_0 {
        return Err("authority capture did not advance".into());
    }
    if id_repl_1 != id_repl_0 {
        return Err("replica capture changed after authority publish".into());
    }
    Ok("Authority/Replica captures stay independent".into())
}

fn port_schema_intern() -> Result<String, String> {
    let interned_schema = PORT_SCHEMA;
    let interned_binding = PORT_RUST_TYPE;
    let mut world = create_world("Authority", "ctx-b0-port", "world-b0-port", "b0-port")?;
    let adapter = VoxelWorldPortAdapter::new(&mut world);
    if !std::ptr::eq(adapter.schema_id(), interned_schema) {
        return Err("adapter.schema_id is not the interned PORT_SCHEMA".into());
    }
    let evidence = adapter.evidence();
    if !std::ptr::eq(evidence.schema_id, interned_schema) {
        return Err("PortEvidence.schema_id is not interned".into());
    }
    if !std::ptr::eq(evidence.binding_rust_type, interned_binding) {
        return Err("PortEvidence.binding_rust_type is not the interned PORT_RUST_TYPE".into());
    }
    Ok(format!(
        "schema_id={interned_schema} binding={interned_binding}"
    ))
}

fn deterministic_two_seeds() -> Result<String, String> {
    let ops: Vec<_> = (0..32)
        .map(|i| VoxelOperation {
            schema_id: "voxel-query",
            seq: i,
            payload: vec![i as u8, 7],
        })
        .collect();
    let vec_fold = DeterministicExecutor::vec_fold_payloads(&ops);
    let map_fold = DeterministicExecutor::hashmap_fold_payloads(&ops);
    if vec_fold == map_fold {
        return Err("hashmap fold matched vec fold".into());
    }
    let mut hashes = Vec::new();
    for seed in [SEED_A, SEED_B] {
        let schedule = Schedule {
            seed,
            ops: ops.clone(),
        };
        let a = DeterministicExecutor::run(&schedule);
        let b = DeterministicExecutor::run(&schedule);
        if a != b || a.snapshot != b.snapshot {
            return Err(format!("seed {seed:#x} replay diverged"));
        }
        hashes.push(a.snapshot);
    }
    if hashes[0] != hashes[1] {
        return Err("same ops produced different snapshot hashes across seeds".into());
    }
    Ok(format!(
        "seeds {SEED_A:#x}/{SEED_B:#x} same snapshot; hashmap fold != vec fold"
    ))
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

fn create_world(
    role: &str,
    context: &str,
    world_id: &str,
    label: &str,
) -> Result<VoxelWorld, String> {
    VoxelWorld::create(
        WorldDescriptor {
            role: role.to_string(),
            world_context_id: context.to_string(),
            capabilities: vec!["Native".into(), "ReferenceVoxel".into()],
            config: WorldConfigAdapter {
                world_id: world_id.to_string(),
            },
        },
        approved_snapshot(label),
    )
    .map_err(|err| format!("VoxelWorld::create {role}: {}", err.error_id()))
}

fn authority(
    label: &str,
    world_id: &str,
    context_id: &str,
    generation: u64,
    initial: PublishedStateRoot,
) -> Result<PublicationAuthority, String> {
    let pins =
        PinRegistry::from_approved_snapshot(approved_snapshot(label), 16, context_id, generation);
    PublicationAuthority::new(world_id, context_id, generation, pins, initial)
        .map_err(|err| format!("PublicationAuthority::new: {}", err.error_id()))
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
) -> lumio_voxel_domain::revision::RevisionStamp {
    let world = world_rev(world_rev_n);
    let mut pairs = Vec::new();
    for (id, rev) in sections {
        let mut section_alloc = RevisionAllocator::new();
        for _ in 0..*rev {
            section_alloc.reserve_section().unwrap().abandon();
        }
        let mut reserved = section_alloc.reserve_section().unwrap();
        pairs.push((id.to_string(), reserved.finalize().unwrap()));
    }
    to_revision_stamp(world_id, context_id, generation, world, &pairs)
}

fn payload(bytes: &[u8]) -> SectionPayload {
    SectionPayload::from_pages([SectionPage::new(
        "Dense",
        "None",
        bytes.to_vec(),
        sha256(bytes),
    )])
    .expect("valid dense uncompressed page")
}

fn empty_replacement(
    base: &lumio_voxel_domain::section::SectionDirectoryRoot,
) -> lumio_voxel_domain::section::SectionReplacement {
    SectionDeltaBuilder::new(base)
        .freeze()
        .expect("empty replacement")
}

fn root_at(
    world_id: &str,
    context_id: &str,
    generation: u64,
    world_rev_n: u64,
    slot: SectionSlot,
    dirty_reason: Option<&str>,
) -> PublishedStateRoot {
    let mut builder = SectionDirectoryBuilder::new();
    builder
        .insert("s:0:0:0", slot)
        .expect("canonical section id");
    let directory = builder.freeze();
    let stamp = stamp_at(
        world_id,
        context_id,
        generation,
        world_rev_n,
        &[("s:0:0:0", world_rev_n)],
    );
    let dirty = match dirty_reason {
        Some(reason) => DirtyFrontier::new(world_id, generation)
            .expect("world id")
            .record("s:0:0:0", world_rev_n, reason)
            .expect("record dirty"),
        None => DirtyFrontier::new(world_id, generation).expect("world id"),
    };
    PublishedStateRoot::new(stamp, directory, dirty)
}

fn assert_cut(view: &PublishedReadView, world_revision: u64, presence: &str) -> Result<(), String> {
    cut_identity(view)?;
    if view.stamp().world_revision != world_revision {
        return Err(format!(
            "expected world_revision {world_revision}, got {}",
            view.stamp().world_revision
        ));
    }
    let got = view
        .directory()
        .lookup("s:0:0:0")
        .map_err(|err| err.error_id().to_string())?
        .ok_or_else(|| "published slot missing".to_string())?
        .presence();
    if got != presence {
        return Err(format!("expected presence {presence}, got {got}"));
    }
    intern_presence(got)?;
    Ok(())
}

fn cut_identity(view: &PublishedReadView) -> Result<(), String> {
    if view.stamp() != view.root().stamp() || view.stamp() != view.lease().stamp() {
        return Err("stamp/lease/root mixed".into());
    }
    if view.directory() != view.root().directory() {
        return Err("directory mixed with another cut".into());
    }
    if view.dirty_frontier() != view.root().dirty_frontier() {
        return Err("dirty frontier mixed with another cut".into());
    }
    let presence = view
        .directory()
        .lookup("s:0:0:0")
        .map_err(|err| err.error_id().to_string())?
        .ok_or_else(|| "published slot missing".to_string())?
        .presence();
    match view.stamp().world_revision {
        0 if presence == "Unchanged" => Ok(()),
        1 if presence == "Ready" => Ok(()),
        other => Err(format!(
            "mixed stamp/dir: revision {other} presence {presence}"
        )),
    }
}

fn intern_presence(name: &str) -> Result<&'static str, String> {
    SECTION_PRESENCE
        .iter()
        .copied()
        .find(|item| *item == name)
        .ok_or_else(|| format!("{name} missing from SECTION_PRESENCE"))
}

fn hex32(bytes: &[u8; 32]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(64);
    for b in bytes {
        out.push(HEX[(b >> 4) as usize] as char);
        out.push(HEX[(b & 0xf) as usize] as char);
    }
    out
}

fn git_head() -> String {
    let root = workspace_root_from_manifest(env!("CARGO_MANIFEST_DIR"));
    std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(root)
        .output()
        .ok()
        .and_then(|out| String::from_utf8(out.stdout).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "unknown".to_string())
}

fn join_err(err: Box<dyn std::any::Any + Send>) -> String {
    if let Some(s) = err.downcast_ref::<String>() {
        s.clone()
    } else if let Some(s) = err.downcast_ref::<&str>() {
        (*s).to_string()
    } else {
        "thread panicked".to_string()
    }
}
