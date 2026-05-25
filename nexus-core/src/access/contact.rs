//! Contact management — DID-based identity records with access levels
//!
//! Contacts are persisted as `contacts.json` in the vault directory.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

use super::permission::Permission;

/// A known peer/identity with assigned access level
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Contact {
    /// Decentralized identifier (e.g., "did:nexus:<pubkey>")
    pub did: String,
    /// Human-friendly display name
    pub label: String,
    /// Last-known PeerId for network routing (not used for auth)
    pub peer_id: Option<String>,
    /// PRE public key bytes (for re-encryption grants)
    pub pre_pk: Vec<u8>,
    /// Global access level (default permission for this contact)
    pub access: Permission,
    /// Group membership (group IDs)
    pub groups: Vec<String>,
    /// Unix timestamp (seconds) when contact was created
    pub created_at: u64,
    /// Unix timestamp (seconds) when contact was last modified
    pub updated_at: u64,
}

/// Manages the contacts store (contacts.json)
#[derive(Debug, Clone)]
pub struct ContactStore {
    path: PathBuf,
    contacts: Vec<Contact>,
}

impl ContactStore {
    /// Open a contact store, loading from disk if file exists
    pub fn open(dir: impl AsRef<Path>) -> Result<Self, String> {
        let path = dir.as_ref().join("contacts.json");
        let contacts = if path.exists() {
            let data = fs::read_to_string(&path)
                .map_err(|e| format!("Failed to read contacts.json: {}", e))?;
            serde_json::from_str(&data)
                .map_err(|e| format!("Failed to parse contacts.json: {}", e))?
        } else {
            Vec::new()
        };
        Ok(Self { path, contacts })
    }

    /// Save current state to disk
    pub fn save(&self) -> Result<(), String> {
        let data = serde_json::to_string_pretty(&self.contacts)
            .map_err(|e| format!("Failed to serialize contacts: {}", e))?;
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create dir: {}", e))?;
        }
        fs::write(&self.path, data)
            .map_err(|e| format!("Failed to write contacts.json: {}", e))
    }

    /// Add a new contact (saves automatically)
    pub fn add(&mut self, contact: Contact) -> Result<(), String> {
        if self.contacts.iter().any(|c| c.did == contact.did) {
            return Err(format!("Contact already exists: {}", contact.did));
        }
        self.contacts.push(contact);
        self.save()
    }

    /// Remove a contact by DID (saves automatically)
    pub fn remove(&mut self, did: &str) -> Result<bool, String> {
        let before = self.contacts.len();
        self.contacts.retain(|c| c.did != did);
        if self.contacts.len() < before {
            self.save()?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Get a contact by DID
    pub fn get(&self, did: &str) -> Option<&Contact> {
        self.contacts.iter().find(|c| c.did == did)
    }

    /// Get a mutable contact by DID
    pub fn get_mut(&mut self, did: &str) -> Option<&mut Contact> {
        self.contacts.iter_mut().find(|c| c.did == did)
    }

    /// List all contacts
    pub fn list(&self) -> &[Contact] {
        &self.contacts
    }

    /// Update access level for a contact (saves automatically)
    pub fn update_access(&mut self, did: &str, access: Permission) -> Result<bool, String> {
        if let Some(contact) = self.contacts.iter_mut().find(|c| c.did == did) {
            contact.access = access;
            contact.updated_at = now_secs();
            self.save()?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Check if a DID is a known contact
    pub fn is_known(&self, did: &str) -> bool {
        self.contacts.iter().any(|c| c.did == did)
    }

    /// Get all DIDs in a specific group
    pub fn members_of_group(&self, group_id: &str) -> Vec<&Contact> {
        self.contacts
            .iter()
            .filter(|c| c.groups.contains(&group_id.to_string()))
            .collect()
    }
}

/// Current time as unix seconds
fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn make_contact(did: &str, access: Permission) -> Contact {
        Contact {
            did: did.to_string(),
            label: format!("Test {}", &did[10..14]),
            peer_id: None,
            pre_pk: vec![1, 2, 3],
            access,
            groups: vec![],
            created_at: now_secs(),
            updated_at: now_secs(),
        }
    }

    #[test]
    fn test_add_and_get() {
        let tmp = TempDir::new().unwrap();
        let mut store = ContactStore::open(tmp.path()).unwrap();

        let contact = make_contact("did:nexus:alice123456", Permission::READ);
        store.add(contact.clone()).unwrap();

        let retrieved = store.get("did:nexus:alice123456").unwrap();
        assert_eq!(retrieved.did, "did:nexus:alice123456");
        assert_eq!(retrieved.access, Permission::READ);
    }

    #[test]
    fn test_duplicate_rejected() {
        let tmp = TempDir::new().unwrap();
        let mut store = ContactStore::open(tmp.path()).unwrap();

        let contact = make_contact("did:nexus:alice123456", Permission::READ);
        store.add(contact.clone()).unwrap();
        assert!(store.add(contact).is_err());
    }

    #[test]
    fn test_remove() {
        let tmp = TempDir::new().unwrap();
        let mut store = ContactStore::open(tmp.path()).unwrap();

        store.add(make_contact("did:nexus:alice123456", Permission::READ)).unwrap();
        assert!(store.remove("did:nexus:alice123456").unwrap());
        assert!(!store.remove("did:nexus:alice123456").unwrap());
        assert!(store.get("did:nexus:alice123456").is_none());
    }

    #[test]
    fn test_update_access() {
        let tmp = TempDir::new().unwrap();
        let mut store = ContactStore::open(tmp.path()).unwrap();

        store.add(make_contact("did:nexus:alice123456", Permission::READ)).unwrap();
        assert!(store.update_access("did:nexus:alice123456", Permission::READ_WRITE).unwrap());

        let contact = store.get("did:nexus:alice123456").unwrap();
        assert_eq!(contact.access, Permission::READ_WRITE);
    }

    #[test]
    fn test_persistence() {
        let tmp = TempDir::new().unwrap();

        {
            let mut store = ContactStore::open(tmp.path()).unwrap();
            store.add(make_contact("did:nexus:alice123456", Permission::FULL)).unwrap();
        }

        // Reload from disk
        let store = ContactStore::open(tmp.path()).unwrap();
        assert_eq!(store.list().len(), 1);
        assert_eq!(store.get("did:nexus:alice123456").unwrap().access, Permission::FULL);
    }
}
