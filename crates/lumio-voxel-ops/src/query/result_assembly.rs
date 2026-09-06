//! Canonical-order items plus missing-state evidence. No payload pointers.

#![forbid(unsafe_code)]

use super::section_access::SectionAccessResult;
use lumio_voxel_contracts::Hash256;
use lumio_voxel_domain::revision::RevisionStamp;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QueryEvidence {
    read_stamp: RevisionStamp,
    budget_used: usize,
    missing_states: Vec<SectionAccessResult>,
    plan_hash: Hash256,
}

impl QueryEvidence {
    pub fn read_stamp(&self) -> &RevisionStamp {
        &self.read_stamp
    }

    pub fn budget_used(&self) -> usize {
        self.budget_used
    }

    pub fn missing_states(&self) -> &[SectionAccessResult] {
        &self.missing_states
    }

    pub fn plan_hash(&self) -> Hash256 {
        self.plan_hash
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VoxelQueryOutcome {
    items: Vec<SectionAccessResult>,
    evidence: QueryEvidence,
}

impl VoxelQueryOutcome {
    pub fn items(&self) -> &[SectionAccessResult] {
        &self.items
    }

    pub fn evidence(&self) -> &QueryEvidence {
        &self.evidence
    }
}

pub(super) fn assemble(
    items: Vec<SectionAccessResult>,
    read_stamp: RevisionStamp,
    budget_used: usize,
    plan_hash: Hash256,
) -> VoxelQueryOutcome {
    let missing_states = items
        .iter()
        .filter(|item| item.presence() != "Ready")
        .cloned()
        .collect();
    VoxelQueryOutcome {
        items,
        evidence: QueryEvidence {
            read_stamp,
            budget_used,
            missing_states,
            plan_hash,
        },
    }
}
