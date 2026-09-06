//! Deterministic schedule replay. Order is the `Schedule.ops` vec, never a
//! HashMap iteration.

use crate::reference_harness::{VoxelOperation, VoxelOutcome, VoxelPortHarness};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Schedule {
    pub seed: u64,
    pub ops: Vec<VoxelOperation>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Trace {
    pub seed: u64,
    pub outcomes: Vec<VoxelOutcome>,
    pub snapshot: [u8; 32],
}

pub struct DeterministicExecutor;

impl DeterministicExecutor {
    pub fn run(schedule: &Schedule) -> Trace {
        let mut port = VoxelPortHarness::new();
        let mut outcomes = Vec::with_capacity(schedule.ops.len());
        for op in &schedule.ops {
            outcomes.push(port.execute(op));
        }
        Trace {
            seed: schedule.seed,
            outcomes,
            snapshot: port.snapshot_hash(),
        }
    }

    /// Anti-pattern: fold through HashMap so order is not the schedule order.
    /// Tests use this to show why the executor must not.
    pub fn hashmap_fold_payloads(ops: &[VoxelOperation]) -> Vec<u8> {
        use std::collections::HashMap;
        let mut map = HashMap::new();
        for op in ops {
            map.insert(op.seq, op.payload.clone());
        }
        let mut out = Vec::new();
        for payload in map.into_values() {
            out.extend_from_slice(&payload);
        }
        out
    }

    pub fn vec_fold_payloads(ops: &[VoxelOperation]) -> Vec<u8> {
        let mut out = Vec::new();
        for op in ops {
            out.extend_from_slice(&op.payload);
        }
        out
    }
}
