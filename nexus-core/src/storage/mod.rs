//! Storage module — content-addressed sharding and local store
//!
//! Files are split into fixed-size shards, each identified by its content hash (CID).
//! The ShardStore provides disk-backed storage for serving shards over P2P.

pub mod shard;
pub mod store;
pub mod net_store;

pub use shard::{Shard, ShardManifest, shard_data, reassemble, compute_cid, DEFAULT_SHARD_SIZE};
pub use store::{ShardStore, StoreStats};
pub use net_store::{NetworkStore, FetchResult};
