//! `voxel-world-port` total adapter. No second ABI, no FFI.

#![forbid(unsafe_code)]

mod adapter;
mod error_mapping;
mod ownership;

pub use adapter::{
    MutationStatus, PORT_METHODS, PORT_RUST_TYPE, PORT_SCHEMA, PortEvidence, VoxelWorldPortAdapter,
};
pub use error_mapping::{
    PortError, map_internal_error, map_mutation_error, map_query_error, map_world_error,
};
pub use ownership::OwnedResultBuffer;
