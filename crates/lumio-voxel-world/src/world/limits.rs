//! Host-selected resource limits. These are implementation policy, not wire fields.

use super::WorldError;

/// Finite admission limits for one world. Capacity is not eagerly allocated.
/// Receipt capacity includes prepared and applied transactions. Applied receipts
/// are never silently evicted: exhaustion rejects new transactions without losing
/// the evidence needed to prevent duplicate application.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WorldLimits {
    pub max_pinned_revisions: usize,
    pub max_receipts: usize,
    pub max_query_sections: usize,
}

impl Default for WorldLimits {
    fn default() -> Self {
        Self {
            max_pinned_revisions: 64,
            max_receipts: 4096,
            max_query_sections: 256,
        }
    }
}

impl WorldLimits {
    pub(crate) fn validate(self) -> Result<Self, WorldError> {
        // Publication must hold the current root and the sealed next root together.
        if self.max_pinned_revisions < 2 || self.max_receipts == 0 || self.max_query_sections == 0 {
            return Err(WorldError::mapped("BudgetExceeded"));
        }
        Ok(self)
    }
}
