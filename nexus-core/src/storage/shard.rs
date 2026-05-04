//! Content-addressed sharding
//!
//! Split data into fixed-size chunks, each identified by SHA2-256 hash.

use multihash::Multihash;
use sha2::{Sha256, Digest};
use serde::{Deserialize, Serialize};

/// Default shard size: 256 KB
pub const DEFAULT_SHARD_SIZE: usize = 256 * 1024;

/// SHA2-256 multihash code
const SHA2_256_CODE: u64 = 0x12;


/// A single content-addressed shard
#[derive(Debug, Clone)]
pub struct Shard {
    /// Content identifier (multihash of shard data)
    pub cid: Vec<u8>,
    /// The shard data
    pub data: Vec<u8>,
}

/// Manifest describing a sharded file
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShardManifest {
    /// Original filename (optional)
    pub filename: Option<String>,
    /// Total size of original data
    pub total_size: u64,
    /// Shard size used
    pub shard_size: usize,
    /// Ordered list of shard CIDs (hex-encoded multihash)
    pub shards: Vec<String>,
}

/// Compute CID for a chunk of data (SHA2-256 multihash)
pub fn compute_cid(data: &[u8]) -> Vec<u8> {
    let hash = Sha256::digest(data);
    let mh = Multihash::<64>::wrap(SHA2_256_CODE, &hash).expect("valid multihash");
    mh.to_bytes()
}

/// Split data into content-addressed shards
pub fn shard_data(data: &[u8], shard_size: usize) -> (ShardManifest, Vec<Shard>) {
    let chunks: Vec<&[u8]> = data.chunks(shard_size).collect();
    let mut shards = Vec::with_capacity(chunks.len());
    let mut cid_list = Vec::with_capacity(chunks.len());

    for chunk in chunks {
        let cid = compute_cid(chunk);
        cid_list.push(hex_encode(&cid));
        shards.push(Shard {
            cid: cid.clone(),
            data: chunk.to_vec(),
        });
    }

    let manifest = ShardManifest {
        filename: None,
        total_size: data.len() as u64,
        shard_size,
        shards: cid_list,
    };

    (manifest, shards)
}

/// Reassemble shards into original data (ordered by manifest)
pub fn reassemble(manifest: &ShardManifest, shards: &[Shard]) -> Option<Vec<u8>> {
    let mut result = Vec::with_capacity(manifest.total_size as usize);

    for expected_cid in &manifest.shards {
        let shard = shards.iter().find(|s| hex_encode(&s.cid) == *expected_cid)?;
        result.extend_from_slice(&shard.data);
    }

    if result.len() as u64 != manifest.total_size {
        return None;
    }

    Some(result)
}

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_shard_small_data() {
        let data = b"Hello, NEXUS!";
        let (manifest, shards) = shard_data(data, DEFAULT_SHARD_SIZE);

        assert_eq!(manifest.shards.len(), 1);
        assert_eq!(shards.len(), 1);
        assert_eq!(manifest.total_size, data.len() as u64);
    }

    #[test]
    fn test_shard_exact_boundary() {
        let data = vec![0xAB; 512]; // Exactly 2 shards at 256 bytes each
        let (manifest, shards) = shard_data(&data, 256);

        assert_eq!(manifest.shards.len(), 2);
        assert_eq!(shards.len(), 2);
    }

    #[test]
    fn test_shard_reassemble() {
        let data = b"The quick brown fox jumps over the lazy dog. This needs to be long enough for multiple shards.";
        let (manifest, shards) = shard_data(data, 20);

        let reassembled = reassemble(&manifest, &shards).unwrap();
        assert_eq!(data.as_slice(), reassembled.as_slice());
    }

    #[test]
    fn test_cid_deterministic() {
        let data = b"same input, same hash";
        let cid1 = compute_cid(data);
        let cid2 = compute_cid(data);
        assert_eq!(cid1, cid2);
    }

    #[test]
    fn test_cid_different_for_different_data() {
        let cid1 = compute_cid(b"hello");
        let cid2 = compute_cid(b"world");
        assert_ne!(cid1, cid2);
    }

    #[test]
    fn test_manifest_serialization() {
        let data = vec![0u8; 1000];
        let (manifest, _) = shard_data(&data, 300);

        let json = serde_json::to_string(&manifest).unwrap();
        let deserialized: ShardManifest = serde_json::from_str(&json).unwrap();

        assert_eq!(manifest.shards, deserialized.shards);
        assert_eq!(manifest.total_size, deserialized.total_size);
    }
}
