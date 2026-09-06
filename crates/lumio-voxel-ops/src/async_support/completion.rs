//! Completion envelope. Completions cannot publish.

use super::origin::OriginToken;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CompletionDisposition {
    Accept,
    Stale,
    Duplicate,
    Late,
    Cancelled,
    WrongWorld,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompletionEnvelope<R> {
    pub origin: OriginToken,
    pub outcome: Result<R, &'static str>,
}

pub fn validate_completion(expected: &OriginToken, current: &OriginToken) -> CompletionDisposition {
    if expected.world_context_id() != current.world_context_id() {
        return CompletionDisposition::WrongWorld;
    }
    if current.instance_generation() < expected.instance_generation() {
        return CompletionDisposition::Stale;
    }
    if current.instance_generation() != expected.instance_generation() {
        return CompletionDisposition::WrongWorld;
    }
    if current.input_world_revision() < expected.input_world_revision() {
        return CompletionDisposition::Late;
    }
    if expected.apply_phase() != current.apply_phase() {
        return CompletionDisposition::Late;
    }
    if expected.request_id() == current.request_id() {
        return CompletionDisposition::Duplicate;
    }
    CompletionDisposition::Accept
}
