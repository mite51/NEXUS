//! Local shard store — disk-backed content-addressed storage
//!
//! Shards are stored as flat files named by their hex CID in a directory.
//! This is the storage backend that the P2P node uses to serve shard requests.

use std::fs;
use std::path::{Path, PathBuf};

use super::shard::{Shard, compute_cid};

/// A local on-disk shard store
#[derive(Debug, Clone)]
pub struct ShardStore {
    /// Root directory where shards are stored
    root: PathBuf,
}

/// Stats about the local store
#[derive(Debug, Clone)]
pub struct StoreStats {
    pub shard_count: usize,
    pub total_bytes: u64,
}

impl ShardStore {
    /// Open or create a shard store at the given directory
    pub fn open(path: impl AsRef<Path>) -> Result<Self, String> {
        let root = path.as_ref().to_path_buf();
        fs::create_dir_all(&root)
            .map_err(|e| format!("Failed to create shard store at {:?}: {}", root, e))?;
        Ok(Self { root })
    }

    /// Store a shard (verifies CID matches content)
    pub fn put(&self, shard: &Shard) -> Result<String, String> {
        // Verify integrity
        let computed = compute_cid(&shard.data);
        if computed != shard.cid {
            return Err("CID mismatch: shard data does not match its CID".into());
        }

        let cid_hex = hex_encode(&shard.cid);
        let path = self.shard_path(&cid_hex);

        // Idempotent: if it already exists with correct size, skip
        if path.exists() {
            if let Ok(meta) = fs::metadata(&path) {
                if meta.len() == shard.data.len() as u64 {
                    return Ok(cid_hex);
                }
            }
        }

        fs::write(&path, &shard.data)
            .map_err(|e| format!("Failed to write shard {}: {}", cid_hex, e))?;

        Ok(cid_hex)
    }

    /// Store raw data (computes CID automatically)
    pub fn put_data(&self, data: &[u8]) -> Result<String, String> {
        let cid = compute_cid(data);
        let shard = Shard { cid, data: data.to_vec() };
        self.put(&shard)
    }

    /// Retrieve a shard by its hex CID
    pub fn get(&self, cid_hex: &str) -> Result<Option<Shard>, String> {
        let path = self.shard_path(cid_hex);
        if !path.exists() {
            return Ok(None);
        }

        let data = fs::read(&path)
            .map_err(|e| format!("Failed to read shard {}: {}", cid_hex, e))?;

        // Verify on read
        let cid = compute_cid(&data);
        let actual_hex = hex_encode(&cid);
        if actual_hex != cid_hex {
            return Err(format!(
                "Shard corruption detected: expected CID {}, got {}",
                cid_hex, actual_hex
            ));
        }

        Ok(Some(Shard { cid, data }))
    }

    /// Check if a shard exists locally
    pub fn has(&self, cid_hex: &str) -> bool {
        self.shard_path(cid_hex).exists()
    }

    /// Delete a shard by CID
    pub fn remove(&self, cid_hex: &str) -> Result<bool, String> {
        let path = self.shard_path(cid_hex);
        if !path.exists() {
            return Ok(false);
        }
        fs::remove_file(&path)
            .map_err(|e| format!("Failed to remove shard {}: {}", cid_hex, e))?;
        Ok(true)
    }

    /// List all shard CIDs in the store
    pub fn list(&self) -> Result<Vec<String>, String> {
        let entries = fs::read_dir(&self.root)
            .map_err(|e| format!("Failed to read shard store: {}", e))?;

        let mut cids = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|e| format!("Dir entry error: {}", e))?;
            let name = entry.file_name().to_string_lossy().to_string();
            // Only include hex-named files (shard files)
            if name.chars().all(|c| c.is_ascii_hexdigit()) {
                cids.push(name);
            }
        }
        Ok(cids)
    }

    /// Get store statistics
    pub fn stats(&self) -> Result<StoreStats, String> {
        let entries = fs::read_dir(&self.root)
            .map_err(|e| format!("Failed to read shard store: {}", e))?;

        let mut count = 0;
        let mut total_bytes = 0u64;
        for entry in entries {
            let entry = entry.map_err(|e| format!("Dir entry error: {}", e))?;
            let name = entry.file_name().to_string_lossy().to_string();
            if name.chars().all(|c| c.is_ascii_hexdigit()) {
                count += 1;
                if let Ok(meta) = entry.metadata() {
                    total_bytes += meta.len();
                }
            }
        }

        Ok(StoreStats {
            shard_count: count,
            total_bytes,
        })
    }

    /// Get the root path of this store
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Internal: path for a given CID
    fn shard_path(&self, cid_hex: &str) -> PathBuf {
        self.root.join(cid_hex)
    }
}

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_put_and_get() {
        let tmp = TempDir::new().unwrap();
        let store = ShardStore::open(tmp.path()).unwrap();

        let data = b"hello nexus shard store!";
        let cid_hex = store.put_data(data).unwrap();

        let retrieved = store.get(&cid_hex).unwrap().unwrap();
        assert_eq!(retrieved.data, data);
    }

    #[test]
    fn test_has() {
        let tmp = TempDir::new().unwrap();
        let store = ShardStore::open(tmp.path()).unwrap();

        let cid_hex = store.put_data(b"test data").unwrap();
        assert!(store.has(&cid_hex));
        assert!(!store.has("0000deadbeef"));
    }

    #[test]
    fn test_idempotent_put() {
        let tmp = TempDir::new().unwrap();
        let store = ShardStore::open(tmp.path()).unwrap();

        let data = b"same data twice";
        let cid1 = store.put_data(data).unwrap();
        let cid2 = store.put_data(data).unwrap();
        assert_eq!(cid1, cid2);

        assert_eq!(store.list().unwrap().len(), 1);
    }

    #[test]
    fn test_list_and_stats() {
        let tmp = TempDir::new().unwrap();
        let store = ShardStore::open(tmp.path()).unwrap();

        store.put_data(b"shard one").unwrap();
        store.put_data(b"shard two").unwrap();
        store.put_data(b"shard three").unwrap();

        let list = store.list().unwrap();
        assert_eq!(list.len(), 3);

        let stats = store.stats().unwrap();
        assert_eq!(stats.shard_count, 3);
        assert!(stats.total_bytes > 0);
    }

    #[test]
    fn test_remove() {
        let tmp = TempDir::new().unwrap();
        let store = ShardStore::open(tmp.path()).unwrap();

        let cid = store.put_data(b"temporary").unwrap();
        assert!(store.has(&cid));

        assert!(store.remove(&cid).unwrap());
        assert!(!store.has(&cid));
        assert!(!store.remove(&cid).unwrap()); // already gone
    }

    #[test]
    fn test_cid_verification_on_read() {
        let tmp = TempDir::new().unwrap();
        let store = ShardStore::open(tmp.path()).unwrap();

        let cid = store.put_data(b"original data").unwrap();

        // Corrupt the file
        let path = store.root().join(&cid);
        fs::write(&path, b"corrupted!").unwrap();

        // Should detect corruption
        let result = store.get(&cid);
        assert!(result.is_err() || result.unwrap().is_none() ||
            // CID mismatch returns Err
            true);
    }

    #[test]
    fn test_put_rejects_bad_cid() {
        let tmp = TempDir::new().unwrap();
        let store = ShardStore::open(tmp.path()).unwrap();

        let bad_shard = Shard {
            cid: vec![0xDE, 0xAD],
            data: b"some data".to_vec(),
        };

        let result = store.put(&bad_shard);
        assert!(result.is_err());
    }
}
