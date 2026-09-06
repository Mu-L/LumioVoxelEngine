//! L0 契约绑定。
//!
//! 只有一个公共语义来源:[`voxel_world`]。它镜像 `wire/voxel-world-v1.json`
//! (`contractId: lumio.voxel-world.v1`)——体素世界数据的活契约:Block / Section / Chunk
//! 分层、规范键、限值、缺块四态与错误码。`tests/voxel_world_conformance.rs` 证明这里的
//! 每一个值都等于那份 JSON,并校验 JSON 副本自身的 SHA-256。
//!
//! [`plumbing`] 是契约无关的水管(SHA-256、Hash 链、有界缓冲),本仓自有实现。
//!
//! 死基线 `LGE-V1.4-2026-08-27` 的只读镜像(`generated/` 树)与它的 `legacy_baseline`
//! 已按 [ADR 0014] 整体删除:生成源仓 `LumioGameEngineArchitecture` 不存在、镜像永远无法
//! 重新生成,且它用 `Chunk` 指代 16×16×16 的数据单元——按活契约,那个单元叫 Section。
//!
//! 本 crate 不得定义第二套 Schema 字段、ID 或序列化器。
//!
//! [ADR 0014]: ../../../.spec/decisions/0014-exit-legacy-baseline-contract-regime.md

#![forbid(unsafe_code)]

pub mod plumbing;
pub mod voxel_world;

pub use plumbing::{
    BoundedBuffer, BufferFull, ChainBreak, Hash256, hash_chain_append, hash_chain_verify, sha256,
    sha256_hex,
};

pub const CRATE_NAME: &str = "lumio-voxel-contracts";

/// `id` 是不是本工作区可以报出的稳定错误 id?
///
/// 唯一判定依据是活契约的 `errorCodes`。死基线镜像的 `STABLE_ERROR_IDS` 那一半随
/// [ADR 0014] 的 `generated/` 树一起消失——错误 id 只剩契约这一套 snake_case 命名空间。
pub fn is_stable_error_id(id: &str) -> bool {
    voxel_world::is_error_code(id)
}
