//! Asset store — manages manifests, rfrags, and share links
//!
//! Layout:
//!   .nexus-store/
//!     shards/       ← content-addressed shard files (unchanged)
//!     manifests/    ← encrypted manifest files by asset-id
//!     rfrags/
//!       <asset-id>/
//!         <did-base58>.rfrag   ← per-user re-encryption fragments

use std::fs;
use std::path::{Path, PathBuf};
use sha2::{Sha256, Digest};

/// Manages the structured asset store layout
#[derive(Debug, Clone)]
pub struct AssetStore {
    root: PathBuf,
}

/// Info about a shared asset
#[derive(Debug, Clone)]
pub struct AssetInfo {
    pub asset_id: String,
    pub manifest_path: PathBuf,
    pub shared_with: Vec<String>, // DIDs that have rfrags
}

impl AssetStore {
    /// Open or create an asset store
    pub fn open(path: impl AsRef<Path>) -> Result<Self, String> {
        let root = path.as_ref().to_path_buf();
        fs::create_dir_all(root.join("shards"))
            .map_err(|e| format!("Failed to create shards dir: {}", e))?;
        fs::create_dir_all(root.join("manifests"))
            .map_err(|e| format!("Failed to create manifests dir: {}", e))?;
        fs::create_dir_all(root.join("rfrags"))
            .map_err(|e| format!("Failed to create rfrags dir: {}", e))?;
        Ok(Self { root })
    }

    /// Compute asset ID from manifest content (hex SHA-256)
    pub fn compute_asset_id(manifest_bytes: &[u8]) -> String {
        let hash = Sha256::digest(manifest_bytes);
        hash.iter().map(|b| format!("{:02x}", b)).collect()
    }

    /// Store a manifest, returns asset_id
    pub fn put_manifest(&self, manifest_bytes: &[u8]) -> Result<String, String> {
        let asset_id = Self::compute_asset_id(manifest_bytes);
        let path = self.manifest_path(&asset_id);
        fs::write(&path, manifest_bytes)
            .map_err(|e| format!("Failed to write manifest: {}", e))?;
        Ok(asset_id)
    }

    /// Get manifest bytes by asset_id
    pub fn get_manifest(&self, asset_id: &str) -> Result<Option<Vec<u8>>, String> {
        let path = self.manifest_path(asset_id);
        if !path.exists() {
            return Ok(None);
        }
        fs::read(&path).map(Some)
            .map_err(|e| format!("Failed to read manifest: {}", e))
    }

    /// List all asset IDs (manifests)
    pub fn list_assets(&self) -> Result<Vec<String>, String> {
        let dir = self.root.join("manifests");
        let entries = fs::read_dir(&dir)
            .map_err(|e| format!("Failed to read manifests dir: {}", e))?;
        let mut ids = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|e| format!("Dir error: {}", e))?;
            let name = entry.file_name().to_string_lossy().to_string();
            if let Some(id) = name.strip_suffix(".nexus") {
                ids.push(id.to_string());
            }
        }
        Ok(ids)
    }

    /// Store an rfrag for a recipient
    pub fn put_rfrag(&self, asset_id: &str, recipient_did: &str, rfrag_bytes: &[u8]) -> Result<(), String> {
        let dir = self.rfrag_dir(asset_id);
        fs::create_dir_all(&dir)
            .map_err(|e| format!("Failed to create rfrag dir: {}", e))?;
        let filename = Self::did_to_filename(recipient_did);
        let path = dir.join(format!("{}.rfrag", filename));
        fs::write(&path, rfrag_bytes)
            .map_err(|e| format!("Failed to write rfrag: {}", e))
    }

    /// Get an rfrag for a recipient
    pub fn get_rfrag(&self, asset_id: &str, recipient_did: &str) -> Result<Option<Vec<u8>>, String> {
        let filename = Self::did_to_filename(recipient_did);
        let path = self.rfrag_dir(asset_id).join(format!("{}.rfrag", filename));
        if !path.exists() {
            return Ok(None);
        }
        fs::read(&path).map(Some)
            .map_err(|e| format!("Failed to read rfrag: {}", e))
    }

    /// Check if a recipient has an rfrag for an asset
    pub fn has_rfrag(&self, asset_id: &str, recipient_did: &str) -> bool {
        let filename = Self::did_to_filename(recipient_did);
        self.rfrag_dir(asset_id).join(format!("{}.rfrag", filename)).exists()
    }

    /// Remove an rfrag (revoke access)
    pub fn remove_rfrag(&self, asset_id: &str, recipient_did: &str) -> Result<bool, String> {
        let filename = Self::did_to_filename(recipient_did);
        let path = self.rfrag_dir(asset_id).join(format!("{}.rfrag", filename));
        if !path.exists() {
            return Ok(false);
        }
        fs::remove_file(&path)
            .map_err(|e| format!("Failed to remove rfrag: {}", e))?;
        Ok(true)
    }

    /// List all recipients for an asset (DIDs that have rfrags)
    pub fn list_shared_users(&self, asset_id: &str) -> Result<Vec<String>, String> {
        let dir = self.rfrag_dir(asset_id);
        if !dir.exists() {
            return Ok(vec![]);
        }
        let entries = fs::read_dir(&dir)
            .map_err(|e| format!("Failed to read rfrag dir: {}", e))?;
        let mut dids = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|e| format!("Dir error: {}", e))?;
            let name = entry.file_name().to_string_lossy().to_string();
            if let Some(did_encoded) = name.strip_suffix(".rfrag") {
                dids.push(Self::filename_to_did(did_encoded));
            }
        }
        Ok(dids)
    }

    /// Get full asset info
    pub fn asset_info(&self, asset_id: &str) -> Result<Option<AssetInfo>, String> {
        let manifest_path = self.manifest_path(asset_id);
        if !manifest_path.exists() {
            return Ok(None);
        }
        let shared_with = self.list_shared_users(asset_id)?;
        Ok(Some(AssetInfo {
            asset_id: asset_id.to_string(),
            manifest_path,
            shared_with,
        }))
    }

    /// Generate a share link
    pub fn share_link(peer_id: &str, asset_id: &str) -> String {
        format!("nexus://{}/asset/{}", peer_id, asset_id)
    }

    /// Parse a share link into (peer_id, asset_id)
    pub fn parse_share_link(link: &str) -> Option<(String, String)> {
        let stripped = link.strip_prefix("nexus://")?;
        let parts: Vec<&str> = stripped.splitn(3, '/').collect();
        if parts.len() == 3 && parts[1] == "asset" {
            Some((parts[0].to_string(), parts[2].to_string()))
        } else {
            None
        }
    }

    /// Get the shards directory path
    pub fn shards_dir(&self) -> PathBuf {
        self.root.join("shards")
    }

    /// Root path
    pub fn root(&self) -> &Path {
        &self.root
    }

    // Internal helpers

    fn manifest_path(&self, asset_id: &str) -> PathBuf {
        self.root.join("manifests").join(format!("{}.nexus", asset_id))
    }

    fn rfrag_dir(&self, asset_id: &str) -> PathBuf {
        self.root.join("rfrags").join(asset_id)
    }

    /// Encode DID for use as filename (replace : with _)
    fn did_to_filename(did: &str) -> String {
        did.replace(':', "_")
    }

    /// Decode filename back to DID
    fn filename_to_did(filename: &str) -> String {
        // did_nexus_xxx -> did:nexus:xxx
        // Simple heuristic: replace first two _ with :
        let mut result = filename.to_string();
        if let Some(pos) = result.find('_') {
            result.replace_range(pos..pos+1, ":");
            if let Some(pos2) = result[pos+1..].find('_') {
                result.replace_range(pos+1+pos2..pos+1+pos2+1, ":");
            }
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_asset_id_deterministic() {
        let data = b"test manifest content";
        let id1 = AssetStore::compute_asset_id(data);
        let id2 = AssetStore::compute_asset_id(data);
        assert_eq!(id1, id2);
        assert_eq!(id1.len(), 64); // SHA-256 hex
    }

    #[test]
    fn test_put_get_manifest() {
        let tmp = TempDir::new().unwrap();
        let store = AssetStore::open(tmp.path()).unwrap();

        let manifest = b"encrypted manifest data here";
        let asset_id = store.put_manifest(manifest).unwrap();

        let retrieved = store.get_manifest(&asset_id).unwrap().unwrap();
        assert_eq!(retrieved, manifest);
    }

    #[test]
    fn test_rfrag_lifecycle() {
        let tmp = TempDir::new().unwrap();
        let store = AssetStore::open(tmp.path()).unwrap();

        let asset_id = "abc123def456";
        let did = "did:nexus:2SxWkuQjHUYW2CHaXrKiympDgVbcoqz3dYgmxevYH2rK";
        let rfrag = b"re-encryption fragment bytes";

        // Put
        store.put_rfrag(asset_id, did, rfrag).unwrap();
        assert!(store.has_rfrag(asset_id, did));

        // Get
        let retrieved = store.get_rfrag(asset_id, did).unwrap().unwrap();
        assert_eq!(retrieved, rfrag);

        // List
        let users = store.list_shared_users(asset_id).unwrap();
        assert_eq!(users.len(), 1);
        assert_eq!(users[0], did);

        // Remove
        assert!(store.remove_rfrag(asset_id, did).unwrap());
        assert!(!store.has_rfrag(asset_id, did));
        assert!(!store.remove_rfrag(asset_id, did).unwrap());
    }

    #[test]
    fn test_list_assets() {
        let tmp = TempDir::new().unwrap();
        let store = AssetStore::open(tmp.path()).unwrap();

        store.put_manifest(b"manifest one").unwrap();
        store.put_manifest(b"manifest two").unwrap();
        store.put_manifest(b"manifest three").unwrap();

        let assets = store.list_assets().unwrap();
        assert_eq!(assets.len(), 3);
    }

    #[test]
    fn test_share_link() {
        let link = AssetStore::share_link("12D3KooWABC", "deadbeef1234");
        assert_eq!(link, "nexus://12D3KooWABC/asset/deadbeef1234");

        let (peer, asset) = AssetStore::parse_share_link(&link).unwrap();
        assert_eq!(peer, "12D3KooWABC");
        assert_eq!(asset, "deadbeef1234");
    }

    #[test]
    fn test_did_filename_roundtrip() {
        let did = "did:nexus:2SxWkuQjHUYW2CHa";
        let filename = AssetStore::did_to_filename(did);
        assert!(!filename.contains(':'));
        let back = AssetStore::filename_to_did(&filename);
        assert_eq!(back, did);
    }
}
