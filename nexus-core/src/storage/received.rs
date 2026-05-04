//! Received files — tracks manifests pushed to us by other peers
//!
//! When a peer sends us a file (manifest + shards), we store it here
//! so the "Shared With Me" view can display and decrypt them.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// A received file entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReceivedFile {
    /// Unique ID
    pub id: String,
    /// Sender's DID (from manifest owner field)
    pub sender_did: String,
    /// Sender's PeerId (from network event)
    pub sender_peer_id: String,
    /// Original filename
    pub filename: String,
    /// Path where the manifest is stored
    pub manifest_path: String,
    /// Share grant JSON (if this is a PRE-shared file)
    pub share_grant_json: Option<String>,
    /// When we received it (unix timestamp ms)
    pub received_at: u64,
    /// Whether the user has decrypted it yet
    pub decrypted: bool,
}

/// Persisted list of received files
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ReceivedFilesStore {
    pub files: Vec<ReceivedFile>,
}

/// Manager for received files
pub struct ReceivedFiles {
    path: PathBuf,
}

impl ReceivedFiles {
    pub fn open(path: impl AsRef<Path>) -> Self {
        Self {
            path: path.as_ref().to_path_buf(),
        }
    }

    pub fn load(&self) -> ReceivedFilesStore {
        fs::read_to_string(&self.path)
            .ok()
            .and_then(|json| serde_json::from_str(&json).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, store: &ReceivedFilesStore) -> Result<(), String> {
        let json = serde_json::to_string_pretty(store)
            .map_err(|e| format!("Serialization failed: {}", e))?;
        fs::write(&self.path, json)
            .map_err(|e| format!("Failed to write received files: {}", e))?;
        Ok(())
    }

    /// Record a new received file
    pub fn add(
        &self,
        sender_did: String,
        sender_peer_id: String,
        filename: String,
        manifest_path: String,
        share_grant_json: Option<String>,
    ) -> Result<ReceivedFile, String> {
        let mut store = self.load();

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        let id = format!("{:x}", now);

        let file = ReceivedFile {
            id,
            sender_did,
            sender_peer_id,
            filename,
            manifest_path,
            share_grant_json,
            received_at: now,
            decrypted: false,
        };

        store.files.push(file.clone());
        self.save(&store)?;
        Ok(file)
    }

    /// Mark a file as decrypted
    pub fn mark_decrypted(&self, id: &str) -> Result<(), String> {
        let mut store = self.load();
        if let Some(f) = store.files.iter_mut().find(|f| f.id == id) {
            f.decrypted = true;
        }
        self.save(&store)
    }

    /// Remove a received file entry
    pub fn remove(&self, id: &str) -> Result<(), String> {
        let mut store = self.load();
        store.files.retain(|f| f.id != id);
        self.save(&store)
    }

    /// Get all received files
    pub fn all(&self) -> Vec<ReceivedFile> {
        self.load().files
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_store() -> (ReceivedFiles, PathBuf) {
        let path = std::env::temp_dir().join(format!(
            "nexus-received-test-{}-{:?}.json",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = fs::remove_file(&path);
        (ReceivedFiles::open(&path), path)
    }

    #[test]
    fn test_add_and_list() {
        let (store, path) = temp_store();

        let file = store.add(
            "did:nexus:alice".into(),
            "12D3KooWAlice".into(),
            "secret.pdf".into(),
            "./received/secret.nexus".into(),
            Some("{\"cfrags\":[]}".into()),
        ).unwrap();

        assert_eq!(file.filename, "secret.pdf");
        assert!(!file.decrypted);
        assert_eq!(store.all().len(), 1);

        let _ = fs::remove_file(path);
    }

    #[test]
    fn test_mark_decrypted() {
        let (store, path) = temp_store();

        let file = store.add(
            "did:nexus:bob".into(),
            "12D3KooWBob".into(),
            "data.zip".into(),
            "./received/data.nexus".into(),
            None,
        ).unwrap();

        store.mark_decrypted(&file.id).unwrap();
        let all = store.all();
        assert!(all[0].decrypted);

        let _ = fs::remove_file(path);
    }

    #[test]
    fn test_remove() {
        let (store, path) = temp_store();

        let file = store.add(
            "did:nexus:carol".into(),
            "12D3KooWCarol".into(),
            "notes.md".into(),
            "./received/notes.nexus".into(),
            None,
        ).unwrap();

        assert_eq!(store.all().len(), 1);
        store.remove(&file.id).unwrap();
        assert_eq!(store.all().len(), 0);

        let _ = fs::remove_file(path);
    }
}
