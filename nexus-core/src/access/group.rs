//! Group management — logical buckets of DIDs for bulk access grants
//!
//! Groups exist purely for convenience. PRE grants are still per-contact.
//! A group is just a policy shorthand — "grant read to group X" expands
//! to individual grants at write time.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

use super::permission::Permission;

/// A logical group of contacts
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Group {
    /// Unique identifier (uuid or short slug)
    pub id: String,
    /// Human-friendly name (e.g., "Team Alpha", "Family")
    pub name: String,
    /// List of member DIDs
    pub members: Vec<String>,
    /// Default access level applied when group is granted folder access
    pub default_access: Permission,
}

/// Manages the groups store (groups.json)
#[derive(Debug, Clone)]
pub struct GroupStore {
    path: PathBuf,
    groups: Vec<Group>,
}

impl GroupStore {
    /// Open a group store, loading from disk if file exists
    pub fn open(dir: impl AsRef<Path>) -> Result<Self, String> {
        let path = dir.as_ref().join("groups.json");
        let groups = if path.exists() {
            let data = fs::read_to_string(&path)
                .map_err(|e| format!("Failed to read groups.json: {}", e))?;
            serde_json::from_str(&data)
                .map_err(|e| format!("Failed to parse groups.json: {}", e))?
        } else {
            Vec::new()
        };
        Ok(Self { path, groups })
    }

    /// Save current state to disk
    pub fn save(&self) -> Result<(), String> {
        let data = serde_json::to_string_pretty(&self.groups)
            .map_err(|e| format!("Failed to serialize groups: {}", e))?;
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create dir: {}", e))?;
        }
        fs::write(&self.path, data)
            .map_err(|e| format!("Failed to write groups.json: {}", e))
    }

    /// Create a new group (saves automatically)
    pub fn create(&mut self, group: Group) -> Result<(), String> {
        if self.groups.iter().any(|g| g.id == group.id) {
            return Err(format!("Group already exists: {}", group.id));
        }
        self.groups.push(group);
        self.save()
    }

    /// Delete a group by ID (saves automatically)
    pub fn delete(&mut self, id: &str) -> Result<bool, String> {
        let before = self.groups.len();
        self.groups.retain(|g| g.id != id);
        if self.groups.len() < before {
            self.save()?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Add a member DID to a group (saves automatically)
    pub fn add_member(&mut self, group_id: &str, did: &str) -> Result<bool, String> {
        if let Some(group) = self.groups.iter_mut().find(|g| g.id == group_id) {
            if group.members.contains(&did.to_string()) {
                return Ok(false); // already a member
            }
            group.members.push(did.to_string());
            self.save()?;
            Ok(true)
        } else {
            Err(format!("Group not found: {}", group_id))
        }
    }

    /// Remove a member DID from a group (saves automatically)
    pub fn remove_member(&mut self, group_id: &str, did: &str) -> Result<bool, String> {
        if let Some(group) = self.groups.iter_mut().find(|g| g.id == group_id) {
            let before = group.members.len();
            group.members.retain(|m| m != did);
            if group.members.len() < before {
                self.save()?;
                Ok(true)
            } else {
                Ok(false)
            }
        } else {
            Err(format!("Group not found: {}", group_id))
        }
    }

    /// Get a group by ID
    pub fn get(&self, id: &str) -> Option<&Group> {
        self.groups.iter().find(|g| g.id == id)
    }

    /// List all groups
    pub fn list(&self) -> &[Group] {
        &self.groups
    }

    /// Find all groups a DID belongs to
    pub fn groups_for_did(&self, did: &str) -> Vec<&Group> {
        self.groups
            .iter()
            .filter(|g| g.members.contains(&did.to_string()))
            .collect()
    }

    /// Check if a DID is in a specific group
    pub fn is_member(&self, group_id: &str, did: &str) -> bool {
        self.groups
            .iter()
            .find(|g| g.id == group_id)
            .map(|g| g.members.contains(&did.to_string()))
            .unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_create_group() {
        let tmp = TempDir::new().unwrap();
        let mut store = GroupStore::open(tmp.path()).unwrap();

        let group = Group {
            id: "team-alpha".to_string(),
            name: "Team Alpha".to_string(),
            members: vec![],
            default_access: Permission::READ,
        };
        store.create(group).unwrap();

        let retrieved = store.get("team-alpha").unwrap();
        assert_eq!(retrieved.name, "Team Alpha");
    }

    #[test]
    fn test_add_remove_member() {
        let tmp = TempDir::new().unwrap();
        let mut store = GroupStore::open(tmp.path()).unwrap();

        store.create(Group {
            id: "g1".to_string(),
            name: "Group One".to_string(),
            members: vec![],
            default_access: Permission::READ,
        }).unwrap();

        assert!(store.add_member("g1", "did:nexus:alice").unwrap());
        assert!(!store.add_member("g1", "did:nexus:alice").unwrap()); // dup
        assert!(store.is_member("g1", "did:nexus:alice"));

        assert!(store.remove_member("g1", "did:nexus:alice").unwrap());
        assert!(!store.is_member("g1", "did:nexus:alice"));
    }

    #[test]
    fn test_groups_for_did() {
        let tmp = TempDir::new().unwrap();
        let mut store = GroupStore::open(tmp.path()).unwrap();

        store.create(Group {
            id: "g1".to_string(),
            name: "One".to_string(),
            members: vec!["did:nexus:alice".to_string()],
            default_access: Permission::READ,
        }).unwrap();

        store.create(Group {
            id: "g2".to_string(),
            name: "Two".to_string(),
            members: vec!["did:nexus:alice".to_string(), "did:nexus:bob".to_string()],
            default_access: Permission::READ_WRITE,
        }).unwrap();

        let alice_groups = store.groups_for_did("did:nexus:alice");
        assert_eq!(alice_groups.len(), 2);

        let bob_groups = store.groups_for_did("did:nexus:bob");
        assert_eq!(bob_groups.len(), 1);
        assert_eq!(bob_groups[0].id, "g2");
    }

    #[test]
    fn test_persistence() {
        let tmp = TempDir::new().unwrap();

        {
            let mut store = GroupStore::open(tmp.path()).unwrap();
            store.create(Group {
                id: "persist-test".to_string(),
                name: "Persist".to_string(),
                members: vec!["did:nexus:bob".to_string()],
                default_access: Permission::FULL,
            }).unwrap();
        }

        let store = GroupStore::open(tmp.path()).unwrap();
        assert_eq!(store.list().len(), 1);
        assert!(store.is_member("persist-test", "did:nexus:bob"));
    }
}
