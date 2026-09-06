//! L0 契约绑定。
//!
//! 只有一个公共语义来源:[`voxel_world`]。它镜像 `wire/voxel-world-v1.json`
//! (`contractId: lumio.voxel-world.v1`)——体素世界数据的活契约:Block / Section / Chunk
//! 分层、规范键、限值、缺块四态与错误码。`tests/voxel_world_conformance.rs` 证明这里的
//! 每一个值都等于那份 JSON,并校验 JSON 副本自身的 SHA-256。
//!
//! [`plumbing`] 是契约无关的水管(SHA-256、Hash 链、有界缓冲),本仓自有实现。
//!
//! 旧合同制的那套东西——一份死基线的只读产物镜像(`generated/` 树)和它的
//! `legacy_baseline`——已按 [ADR 0014] 整体删除:生成源仓早已不存在、镜像永远无法重新
//! 生成,且它用 `Chunk` 指代 16×16×16 的数据单元,而按活契约那个单元叫 Section。
//! 分层语义、字段、错误码一律只能从 [`voxel_world`] 取,不得再从任何镜像取。
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
