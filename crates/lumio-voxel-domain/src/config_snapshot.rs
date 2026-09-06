//! Immutable Voxel config snapshot and Capability view (R-00066).
//!
//! Numeric VOX-D defaults are not materialized here. A config document is accepted
//! only if it names the schemas this module reads, carries a well-formed config hash,
//! and starts no capability the host did not declare.
//!
//! 旧合同制的 P0 决策门(`VOX-D-00x` 证据、基线 id、schemaEpoch)已随 [ADR 0014] 整体
//! 退出:那套证据的上游生成源仓不存在,门本身校验不了任何与当前公共语义有关的事。
//!
//! [ADR 0014]: ../../../.spec/decisions/0014-exit-legacy-baseline-contract-regime.md

use std::collections::BTreeSet;
use std::fmt;
use std::sync::Arc;

/// Schema name the config document must declare for its table body.
pub const CONFIG_TABLE_SCHEMA: &str = "config-table";
/// Schema name the config document must declare for its host capability body.
pub const HOST_CAPABILITY_SCHEMA: &str = "host-capability";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HostCapabilitySet {
    capabilities: Vec<String>,
}

impl HostCapabilitySet {
    pub fn from_names(names: impl IntoIterator<Item = impl Into<String>>) -> Self {
        let mut capabilities: Vec<String> = names.into_iter().map(Into::into).collect();
        capabilities.sort();
        capabilities.dedup();
        Self { capabilities }
    }

    pub fn names(&self) -> &[String] {
        &self.capabilities
    }

    fn contains(&self, name: &str) -> bool {
        self.capabilities.iter().any(|n| n == name)
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct VoxelConfigInput {
    pub schema_id: &'static str,
    pub host_capability_schema_id: &'static str,
    pub config_hash: String,
    pub host_capability: HostCapabilitySet,
    pub start_capabilities: Vec<String>,
    pub key_material: Option<String>,
}

impl fmt::Debug for VoxelConfigInput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("VoxelConfigInput")
            .field("schema_id", &self.schema_id)
            .field("host_capability_schema_id", &self.host_capability_schema_id)
            .field("config_hash", &self.config_hash)
            .field("host_capability", &self.host_capability)
            .field("start_capabilities", &self.start_capabilities)
            .finish()
    }
}

#[derive(PartialEq, Eq)]
pub struct VoxelConfigSnapshot {
    config_hash: String,
    allow_list: Vec<String>,
}

impl fmt::Debug for VoxelConfigSnapshot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("VoxelConfigSnapshot")
            .field("config_hash", &self.config_hash)
            .field("capabilities", &self.allow_list)
            .finish()
    }
}

impl VoxelConfigSnapshot {
    /// Freeze a validated config document into an immutable snapshot.
    pub fn load(config: &VoxelConfigInput) -> Result<Arc<Self>, ConfigError> {
        require_schema(config.schema_id, CONFIG_TABLE_SCHEMA)?;
        require_schema(config.host_capability_schema_id, HOST_CAPABILITY_SCHEMA)?;
        if !is_hash256(&config.config_hash) {
            return Err(ConfigError::HashMismatch {
                error_id: "EvidenceDigestMismatch",
                gate: "configHash".to_string(),
            });
        }

        for name in &config.start_capabilities {
            if !config.host_capability.contains(name) {
                return Err(ConfigError::UnknownCapability {
                    error_id: "CapabilityMissing",
                    name: name.clone(),
                });
            }
        }

        Ok(Arc::new(Self {
            config_hash: config.config_hash.clone(),
            allow_list: config.host_capability.names().to_vec(),
        }))
    }

    pub fn config_hash(&self) -> &str {
        &self.config_hash
    }

    pub fn capabilities(&self) -> &[String] {
        &self.allow_list
    }

    pub fn audit_summary(&self) -> String {
        format!("{self:?}")
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CapabilityView {
    enabled: BTreeSet<String>,
}

impl CapabilityView {
    pub fn derive(
        declared: &HostCapabilitySet,
        snapshot: &VoxelConfigSnapshot,
    ) -> Result<Self, CapabilityError> {
        let allow: BTreeSet<&str> = snapshot.allow_list.iter().map(String::as_str).collect();
        let mut enabled = BTreeSet::new();
        for name in declared.names() {
            if !allow.contains(name.as_str()) {
                return Err(CapabilityError::Expansion {
                    error_id: "ClaimNotGranted",
                    name: name.clone(),
                });
            }
            enabled.insert(name.clone());
        }
        Ok(Self { enabled })
    }

    pub fn contains(&self, name: &str) -> bool {
        self.enabled.contains(name)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConfigError {
    HashMismatch {
        error_id: &'static str,
        gate: String,
    },
    UnknownCapability {
        error_id: &'static str,
        name: String,
    },
}

impl ConfigError {
    pub fn error_id(&self) -> &'static str {
        match self {
            Self::HashMismatch { error_id, .. } | Self::UnknownCapability { error_id, .. } => {
                error_id
            }
        }
    }
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::HashMismatch { error_id, gate } => write!(f, "{error_id}: {gate}"),
            Self::UnknownCapability { error_id, name } => write!(f, "{error_id}: {name}"),
        }
    }
}

impl std::error::Error for ConfigError {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CapabilityError {
    Expansion {
        error_id: &'static str,
        name: String,
    },
}

impl CapabilityError {
    pub fn error_id(&self) -> &'static str {
        match self {
            Self::Expansion { error_id, .. } => error_id,
        }
    }
}

impl fmt::Display for CapabilityError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Expansion { error_id, name } => write!(f, "{error_id}: {name}"),
        }
    }
}

impl std::error::Error for CapabilityError {}

fn require_schema(found: &str, expected: &str) -> Result<(), ConfigError> {
    if found == expected {
        Ok(())
    } else {
        Err(ConfigError::UnknownCapability {
            error_id: "CapabilityMissing",
            name: found.to_string(),
        })
    }
}

fn is_hash256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}
