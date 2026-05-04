//! Storage module — content-addressed sharding
//!
//! Files are split into fixed-size shards, each identified by its content hash (CID).

pub mod shard;

pub use shard::{Shard, ShardManifest, shard_data};
