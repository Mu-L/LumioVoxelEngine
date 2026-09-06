//! R-00068: OriginToken, bounded port from approved snapshot, completion dispositions.

use lumio_voxel_contracts::sha256;
use lumio_voxel_domain::config_snapshot::{
    HostCapabilitySet, VoxelConfigInput, VoxelConfigSnapshot,
};
use lumio_voxel_ops::async_support::{
    APPLY_PHASES, BoundedJobPort, CompletionDisposition, OriginEnvelope, OriginToken,
    full_load_action, validate_completion,
};
use lumio_voxel_test_support::deterministic_executor::{DeterministicExecutor, Schedule};
use lumio_voxel_test_support::fault_injection::{FaultInjector, FaultPoint};
use lumio_voxel_test_support::reference_harness::VoxelOperation;
use std::collections::BTreeMap;

fn hex32(bytes: &[u8; 32]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(64);
    for b in bytes {
        out.push(HEX[(b >> 4) as usize] as char);
        out.push(HEX[(b & 0xf) as usize] as char);
    }
    out
}

fn approved_snapshot() -> std::sync::Arc<VoxelConfigSnapshot> {
    let cfg = VoxelConfigInput {
        schema_id: "config-table",
        host_capability_schema_id: "host-capability",
        config_hash: hex32(&sha256(b"r00068-approved")),
        host_capability: HostCapabilitySet::from_names(["Native", "ReferenceVoxel"]),
        start_capabilities: vec!["Native".into(), "ReferenceVoxel".into()],
        key_material: None,
    };
    VoxelConfigSnapshot::load(&cfg).expect("valid voxel config")
}

fn origin(
    ctx: &str,
    generation: u64,
    req: &str,
    world_rev: u64,
    phase: &'static str,
) -> OriginToken {
    OriginToken::try_new(ctx, generation, req, world_rev, BTreeMap::new(), phase).unwrap()
}

#[test]
fn origin_requires_generated_phase_and_nonempty_fields() {
    assert!(APPLY_PHASES.contains(&"VoxelCommit"));
    assert!(OriginToken::try_new("", 1, "r", 0, BTreeMap::new(), "VoxelCommit").is_err());
    assert!(OriginToken::try_new("w", 1, "", 0, BTreeMap::new(), "VoxelCommit").is_err());
    assert!(OriginToken::try_new("w", 1, "r", 0, BTreeMap::new(), "NotAPhase").is_err());
    let ok = OriginToken::try_new("w", 1, "r", 0, BTreeMap::new(), "VoxelCommit").unwrap();
    assert_eq!(ok.apply_phase(), "VoxelCommit");
}

#[test]
fn port_from_approved_snapshot_is_bounded_queue_full() {
    let snap = approved_snapshot();
    let mut port = BoundedJobPort::from_approved_snapshot(snap.clone(), 1).unwrap();
    assert_eq!(full_load_action(), "QueueFull");
    let job = OriginEnvelope {
        origin: origin("world-a", 1, "req-1", 0, "VoxelCommit"),
        config_hash: snap.config_hash().to_string(),
        payload: 1u8,
    };
    port.try_submit(job).unwrap();
    let full = OriginEnvelope {
        origin: origin("world-a", 1, "req-2", 0, "VoxelCommit"),
        config_hash: snap.config_hash().to_string(),
        payload: 2u8,
    };
    let err = port.try_submit(full).unwrap_err();
    assert_eq!(err.error_id(), "QueueFull");
}

#[test]
fn dual_world_ports_do_not_share_queue_state() {
    let snap = approved_snapshot();
    let mut a = BoundedJobPort::from_approved_snapshot(snap.clone(), 1).unwrap();
    let mut b: BoundedJobPort<()> =
        BoundedJobPort::from_approved_snapshot(snap.clone(), 1).unwrap();
    let job = OriginEnvelope {
        origin: origin("world-a", 1, "req-a", 0, "VoxelCommit"),
        config_hash: snap.config_hash().to_string(),
        payload: (),
    };
    a.try_submit(job).unwrap();
    assert!(a.pop().is_some());
    assert!(b.pop().is_none());
}

#[test]
fn completion_dispositions_are_exclusive() {
    let basis = origin("world-a", 2, "basis", 5, "VoxelCommit");
    let ok = origin("world-a", 2, "job-1", 5, "VoxelCommit");
    assert_eq!(
        validate_completion(&basis, &ok),
        CompletionDisposition::Accept
    );
    let dup = origin("world-a", 2, "basis", 5, "VoxelCommit");
    assert_eq!(
        validate_completion(&basis, &dup),
        CompletionDisposition::Duplicate
    );
    let stale = origin("world-a", 1, "job-2", 5, "VoxelCommit");
    assert_eq!(
        validate_completion(&basis, &stale),
        CompletionDisposition::Stale
    );
    let late = origin("world-a", 2, "job-3", 4, "VoxelCommit");
    assert_eq!(
        validate_completion(&basis, &late),
        CompletionDisposition::Late
    );
    let wrong = origin("world-b", 2, "job-4", 5, "VoxelCommit");
    assert_eq!(
        validate_completion(&basis, &wrong),
        CompletionDisposition::WrongWorld
    );
}

fn envelope(snap: &VoxelConfigSnapshot, req: &str, payload: u8) -> OriginEnvelope<u8> {
    OriginEnvelope {
        origin: origin("world-a", 1, req, 0, "VoxelCommit"),
        config_hash: snap.config_hash().to_string(),
        payload,
    }
}

/// `pop` must return the slot to the bounded budget, otherwise a `slots = 1`
/// port can only ever be submitted to once in its lifetime.
#[test]
fn pop_returns_slot_to_bounded_budget() {
    let snap = approved_snapshot();
    let mut port = BoundedJobPort::from_approved_snapshot(snap.clone(), 1).unwrap();

    port.try_submit(envelope(&snap, "req-1", 1)).unwrap();
    assert_eq!(port.pop().map(|j| j.payload), Some(1));

    // The slot is free again, so the port must accept another job.
    port.try_submit(envelope(&snap, "req-2", 2))
        .expect("drained port must accept a new job");
    assert_eq!(port.pop().map(|j| j.payload), Some(2));
}

/// Surplus `pop`s on an empty queue must not underflow the budget into a
/// negative/wrapped occupancy that would inflate the port past `slots`.
#[test]
fn surplus_pops_do_not_inflate_bounded_budget() {
    let snap = approved_snapshot();
    let mut port = BoundedJobPort::from_approved_snapshot(snap.clone(), 2).unwrap();

    port.try_submit(envelope(&snap, "req-1", 1)).unwrap();
    assert!(port.pop().is_some());
    // Two surplus pops beyond what was ever pushed.
    assert!(port.pop().is_none());
    assert!(port.pop().is_none());

    // Capacity must still be exactly 2 — not 4, and not a wrapped huge value.
    port.try_submit(envelope(&snap, "req-2", 2)).unwrap();
    port.try_submit(envelope(&snap, "req-3", 3)).unwrap();
    let err = port.try_submit(envelope(&snap, "req-4", 4)).unwrap_err();
    assert_eq!(err.error_id(), "QueueFull");
}

/// Drive one submit with a fault point armed.
///
/// `PrePublication` fires before the job reaches the queue, so nothing becomes
/// visible and the budget is untouched; any later point fires after the job is
/// queued, so the write is already visible and must not be rolled back.
fn submit_under_fault(
    port: &mut BoundedJobPort<u8>,
    injector: &mut FaultInjector,
    job: OriginEnvelope<u8>,
) -> Result<(), &'static str> {
    match injector.take() {
        Some(point @ FaultPoint::PrePublication) => Err(FaultInjector::error_id(point)),
        Some(point) => {
            port.try_submit(job).map_err(|e| e.error_id())?;
            Err(FaultInjector::error_id(point))
        }
        None => port.try_submit(job).map_err(|e| e.error_id()),
    }
}

#[test]
fn injected_pre_publication_fault_leaves_the_port_reusable() {
    let snap = approved_snapshot();
    let mut port = BoundedJobPort::from_approved_snapshot(snap.clone(), 1).unwrap();
    let mut injector = FaultInjector::new();

    injector.arm(FaultPoint::PrePublication);
    let err = submit_under_fault(&mut port, &mut injector, envelope(&snap, "req-1", 1))
        .expect_err("armed pre-publication fault must abort the submit");
    assert_eq!(err, "InvalidHandle");
    assert!(FaultInjector::recoverable(FaultPoint::PrePublication));

    // Nothing was published, so the slot was never consumed.
    assert!(port.pop().is_none());
    submit_under_fault(&mut port, &mut injector, envelope(&snap, "req-1-retry", 1))
        .expect("recoverable fault must leave the port usable");
    assert_eq!(port.pop().map(|j| j.payload), Some(1));
}

#[test]
fn injected_post_publication_fault_keeps_the_visible_write() {
    let snap = approved_snapshot();
    let mut port = BoundedJobPort::from_approved_snapshot(snap.clone(), 1).unwrap();
    let mut injector = FaultInjector::new();

    injector.arm(FaultPoint::PostPublication);
    let err = submit_under_fault(&mut port, &mut injector, envelope(&snap, "req-1", 7))
        .expect_err("armed post-publication fault must report");
    assert_eq!(err, "PartialLoadRolledBack");
    // An already-visible write is never recoverable and must not be undone.
    assert!(!FaultInjector::recoverable(FaultPoint::PostPublication));

    assert_eq!(port.pop().map(|j| j.payload), Some(7));
    // Draining the published job returns its slot to the budget.
    submit_under_fault(&mut port, &mut injector, envelope(&snap, "req-2", 8))
        .expect("drained slot is available again");
}

/// Submit a whole schedule through a `slots = 1` port, draining after each
/// submit. This only terminates because `pop` returns the slot to the budget.
fn drain_through_port(schedule: &Schedule) -> Vec<u8> {
    let snap = approved_snapshot();
    let mut port = BoundedJobPort::from_approved_snapshot(snap.clone(), 1).unwrap();
    let mut drained = Vec::new();
    for op in &schedule.ops {
        let req = format!("req-{}", op.seq);
        port.try_submit(envelope(&snap, &req, op.payload[0]))
            .expect("slots=1 port accepts each job once the previous one is drained");
        drained.push(port.pop().expect("submitted job").payload);
    }
    drained
}

#[test]
fn deterministic_schedule_replays_identically_through_the_bounded_port() {
    let schedule = Schedule {
        seed: 7,
        ops: (0..4u64)
            .map(|seq| VoxelOperation {
                schema_id: "voxel-revision-stamp",
                seq,
                payload: vec![seq as u8],
            })
            .collect(),
    };

    // Differential: the reference executor is replay-stable for this schedule.
    assert_eq!(
        DeterministicExecutor::run(&schedule),
        DeterministicExecutor::run(&schedule),
        "reference replay must be deterministic"
    );

    // ...and so is the ops bounded port driven by the same schedule.
    let first = drain_through_port(&schedule);
    let second = drain_through_port(&schedule);
    assert_eq!(first, second, "port drain order must be deterministic");
    assert_eq!(
        first,
        vec![0u8, 1, 2, 3],
        "drain order is the schedule order"
    );
}
