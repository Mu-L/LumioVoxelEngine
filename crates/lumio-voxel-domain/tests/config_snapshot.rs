//! R-00066: immutable VoxelConfigSnapshot and CapabilityView.

use lumio_voxel_contracts::sha256;
use lumio_voxel_domain::config_snapshot::{
    CapabilityView, HostCapabilitySet, VoxelConfigInput, VoxelConfigSnapshot,
};
use std::sync::Arc;

const SECRET: &str = "test-key-material-do-not-log";

fn hex32(bytes: &[u8; 32]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(64);
    for b in bytes {
        out.push(HEX[(b >> 4) as usize] as char);
        out.push(HEX[(b & 0xf) as usize] as char);
    }
    out
}

fn labeled_hash(label: &str) -> String {
    hex32(&sha256(label.as_bytes()))
}

fn allow_list() -> HostCapabilitySet {
    HostCapabilitySet::from_names(["Native", "ReferenceVoxel", "VoxelSnapshot"])
}

fn config_with(
    hash_label: &str,
    start: &[&str],
    allow: HostCapabilitySet,
    secret: Option<&str>,
) -> VoxelConfigInput {
    VoxelConfigInput {
        schema_id: "config-table",
        host_capability_schema_id: "host-capability",
        config_hash: labeled_hash(hash_label),
        host_capability: allow,
        start_capabilities: start.iter().map(|s| (*s).to_string()).collect(),
        key_material: secret.map(str::to_string),
    }
}

fn start_config(hash_label: &str) -> VoxelConfigInput {
    config_with(
        hash_label,
        &["Native", "ReferenceVoxel"],
        allow_list(),
        Some(SECRET),
    )
}

#[test]
fn a_malformed_config_hash_is_refused_without_a_snapshot() {
    let mut cfg = start_config("malformed-config-hash");
    cfg.config_hash = "not-a-sha256".to_string();
    let err = VoxelConfigSnapshot::load(&cfg).expect_err("malformed configHash must be refused");
    assert_eq!(err.error_id(), "EvidenceDigestMismatch");
    assert!(err.to_string().contains("configHash"));
}

#[test]
fn an_unknown_schema_id_is_refused_without_a_snapshot() {
    let mut cfg = start_config("unknown-schema");
    cfg.schema_id = "voxel-query";
    let err = VoxelConfigSnapshot::load(&cfg).expect_err("wrong schema id must be refused");
    assert_eq!(err.error_id(), "CapabilityMissing");
    assert!(err.to_string().contains("voxel-query"));
}

#[test]
fn unknown_capability_name_cannot_expand() {
    let cfg = config_with(
        "unknown-cap",
        &["Native", "NotARegisteredCapability"],
        allow_list(),
        None,
    );
    let err = VoxelConfigSnapshot::load(&cfg)
        .expect_err("unknown capability must not expand the declared allow-list");
    assert_eq!(err.error_id(), "CapabilityMissing");
    assert!(err.to_string().contains("NotARegisteredCapability"));
}

#[test]
fn capability_view_can_only_narrow_the_declared_allow_list() {
    let cfg = start_config("narrow-ok");
    let snapshot = VoxelConfigSnapshot::load(&cfg).expect("valid voxel config");

    let narrowed = HostCapabilitySet::from_names(["Native"]);
    let view = CapabilityView::derive(&narrowed, &snapshot).expect("narrowing must succeed");
    assert!(view.contains("Native"));
    assert!(!view.contains("ReferenceVoxel"));

    let expanded = HostCapabilitySet::from_names(["Native", "ReferenceVoxel", "VoxelSpatial"]);
    let err = CapabilityView::derive(&expanded, &snapshot).expect_err("expansion must fail");
    assert_eq!(err.error_id(), "ClaimNotGranted");
    assert!(err.to_string().contains("VoxelSpatial"));
}

#[test]
fn snapshot_is_immutable_and_distinct_hashes_are_independent() {
    let src = include_str!("../src/config_snapshot.rs");
    assert!(
        !src.contains("fn set_"),
        "VoxelConfigSnapshot must not expose setters"
    );
    for needle in ["section_size", "page_size", "batch_limit", "lease_ms"] {
        assert!(
            !src.contains(needle),
            "must not invent VOX-D numeric default field {needle}"
        );
    }

    let mut cfg_a = start_config("config-a");
    let cfg_b = start_config("config-b");

    let snap_a = VoxelConfigSnapshot::load(&cfg_a).unwrap();
    let snap_b = VoxelConfigSnapshot::load(&cfg_b).unwrap();
    assert_ne!(snap_a.config_hash(), snap_b.config_hash());
    assert!(!Arc::ptr_eq(&snap_a, &snap_b));

    let original_a = snap_a.config_hash().to_string();
    cfg_a.config_hash = labeled_hash("mutated-after-capture");
    assert_eq!(
        snap_a.config_hash(),
        original_a,
        "mutating the input must not write back into a captured snapshot"
    );

    let debug = format!("{snap_a:?}");
    let audit = snap_a.audit_summary();
    assert!(
        !debug.contains(SECRET),
        "Debug must redact key material: {debug}"
    );
    assert!(
        !audit.contains(SECRET),
        "audit must redact key material: {audit}"
    );
    assert!(
        !debug.to_lowercase().contains("key_material"),
        "Debug must not expose a key material field: {debug}"
    );

    let ha = snap_a.config_hash().to_string();
    let hb = snap_b.config_hash().to_string();
    let ta = std::thread::spawn(move || snap_a.config_hash().to_string());
    let tb = std::thread::spawn(move || snap_b.config_hash().to_string());
    assert_eq!(ta.join().unwrap(), ha);
    assert_eq!(tb.join().unwrap(), hb);
}
