//! Vault folders — logical organization with hierarchical access control
//!
//! Folders provide bulk access control. Permission resolution:
//! asset_grant > folder_grant > contact.access (default)
//!
//! Asset/folder-level permissions take PRECEDENCE over contact-level defaults.
//! A contact with global `read` CAN be elevated to `read+write` on a specific folder.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

use super::contact::ContactStore;
use super::group::GroupStore;
use super::permission::Permission;

/// What an access grant applies to
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum GrantTarget {
    /// Grant applies to a specific asset
    Asset(String),
    /// Grant applies to a folder (and contents unless overridden)
    Folder(String),
}

/// Who receives the grant
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum Grantee {
    /// A specific contact by DID
    Contact(String),
    /// A group by ID (expands to all members)
    Group(String),
}

/// A scoped permission grant
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccessGrant {
    /// What this grant applies to
    pub target: GrantTarget,
    /// Who receives the grant
    pub grantee: Grantee,
    /// Permission level for this scope
    pub level: Permission,
    /// Optional expiry (unix timestamp seconds). None = permanent.
    pub expires: Option<u64>,
}

impl AccessGrant {
    /// Check if this grant has expired
    pub fn is_expired(&self) -> bool {
        if let Some(exp) = self.expires {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            now > exp
        } else {
            false
        }
    }

    /// Check if this grant applies to a specific DID (directly or via group)
    pub fn applies_to(&self, did: &str, groups: &GroupStore) -> bool {
        if self.is_expired() {
            return false;
        }
        match &self.grantee {
            Grantee::Contact(d) => d == did,
            Grantee::Group(gid) => groups.is_member(gid, did),
        }
    }
}

/// A vault folder with access control
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaultFolder {
    /// Folder path (e.g., "/projects/alpha")
    pub path: String,
    /// Display name
    pub label: Option<String>,
    /// Default access level for new assets in this folder
    pub default_access: Permission,
    /// Access grants scoped to this folder
    pub grants: Vec<AccessGrant>,
    /// Whether child folders inherit these grants
    pub inherit: bool,
}

/// Manages the folder store (folders.json)
#[derive(Debug, Clone)]
pub struct FolderStore {
    file_path: PathBuf,
    folders: Vec<VaultFolder>,
}

impl FolderStore {
    /// Open a folder store, loading from disk if file exists
    pub fn open(dir: impl AsRef<Path>) -> Result<Self, String> {
        let file_path = dir.as_ref().join("folders.json");
        let folders = if file_path.exists() {
            let data = fs::read_to_string(&file_path)
                .map_err(|e| format!("Failed to read folders.json: {}", e))?;
            serde_json::from_str(&data)
                .map_err(|e| format!("Failed to parse folders.json: {}", e))?
        } else {
            Vec::new()
        };
        Ok(Self { file_path, folders })
    }

    /// Save current state to disk
    pub fn save(&self) -> Result<(), String> {
        let data = serde_json::to_string_pretty(&self.folders)
            .map_err(|e| format!("Failed to serialize folders: {}", e))?;
        if let Some(parent) = self.file_path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create dir: {}", e))?;
        }
        fs::write(&self.file_path, data)
            .map_err(|e| format!("Failed to write folders.json: {}", e))
    }

    /// Create a new folder (saves automatically)
    pub fn create_folder(&mut self, folder: VaultFolder) -> Result<(), String> {
        // Validate path
        validate_folder_path(&folder.path)?;

        if self.folders.iter().any(|f| f.path == folder.path) {
            return Err(format!("Folder already exists: {}", folder.path));
        }
        self.folders.push(folder);
        self.save()
    }

    /// Delete a folder by path (saves automatically)
    pub fn delete_folder(&mut self, path: &str) -> Result<bool, String> {
        let before = self.folders.len();
        self.folders.retain(|f| f.path != path);
        if self.folders.len() < before {
            self.save()?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Add a grant to a folder (saves automatically)
    pub fn add_grant(&mut self, folder_path: &str, grant: AccessGrant) -> Result<(), String> {
        let folder = self.folders.iter_mut().find(|f| f.path == folder_path)
            .ok_or_else(|| format!("Folder not found: {}", folder_path))?;
        folder.grants.push(grant);
        self.save()
    }

    /// Remove grants for a specific grantee from a folder (saves automatically)
    pub fn remove_grants(&mut self, folder_path: &str, grantee: &Grantee) -> Result<bool, String> {
        let folder = self.folders.iter_mut().find(|f| f.path == folder_path)
            .ok_or_else(|| format!("Folder not found: {}", folder_path))?;
        let before = folder.grants.len();
        folder.grants.retain(|g| &g.grantee != grantee);
        if folder.grants.len() < before {
            self.save()?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Get a folder by path
    pub fn get(&self, path: &str) -> Option<&VaultFolder> {
        self.folders.iter().find(|f| f.path == path)
    }

    /// List all folders
    pub fn list(&self) -> &[VaultFolder] {
        &self.folders
    }

    /// Compute effective permission for a DID on a folder (optionally asset-scoped)
    ///
    /// Resolution order:
    /// 1. Asset-level grant (if asset_id provided)
    /// 2. Folder-level grant (on the target folder)
    /// 3. Parent folder grants (if inherit is enabled, walking up)
    /// 4. Contact's global access level (default)
    pub fn effective_permission(
        &self,
        did: &str,
        folder_path: &str,
        asset_id: Option<&str>,
        contacts: &ContactStore,
        groups: &GroupStore,
    ) -> Permission {
        // 1. Check asset-level grant
        if let Some(aid) = asset_id {
            if let Some(perm) = self.find_grant_for(did, &GrantTarget::Asset(aid.to_string()), groups) {
                return perm;
            }
        }

        // 2. Check folder-level grant (exact match)
        if let Some(perm) = self.find_grant_for(did, &GrantTarget::Folder(folder_path.to_string()), groups) {
            return perm;
        }

        // 3. Walk up parent folders (inheritance)
        let mut current = folder_path.to_string();
        while let Some(parent) = parent_path(&current) {
            if let Some(folder) = self.get(&parent) {
                if folder.inherit {
                    if let Some(perm) = self.find_folder_grant(did, folder, groups) {
                        return perm;
                    }
                }
            }
            current = parent;
        }

        // 4. Fall back to contact's global access
        contacts
            .get(did)
            .map(|c| c.access)
            .unwrap_or(Permission::NONE)
    }

    /// Find a matching grant for a DID on a specific target across all folders
    fn find_grant_for(&self, did: &str, target: &GrantTarget, groups: &GroupStore) -> Option<Permission> {
        for folder in &self.folders {
            for grant in &folder.grants {
                if &grant.target == target && grant.applies_to(did, groups) {
                    return Some(grant.level);
                }
            }
        }
        None
    }

    /// Find a folder-level grant for a DID on a specific folder
    fn find_folder_grant(&self, did: &str, folder: &VaultFolder, groups: &GroupStore) -> Option<Permission> {
        let target = GrantTarget::Folder(folder.path.clone());
        for grant in &folder.grants {
            if grant.target == target && grant.applies_to(did, groups) {
                return Some(grant.level);
            }
        }
        None
    }
}

/// Validate folder path (security: no traversal, no absolute paths escaping, reasonable length)
fn validate_folder_path(path: &str) -> Result<(), String> {
    if path.is_empty() {
        return Err("Folder path cannot be empty".to_string());
    }
    if !path.starts_with('/') {
        return Err("Folder path must start with /".to_string());
    }
    if path.contains("..") {
        return Err("Folder path cannot contain '..'".to_string());
    }
    if path.len() > 260 {
        return Err("Folder path exceeds MAX_PATH limit (260 chars)".to_string());
    }
    Ok(())
}

/// Get parent path (e.g., "/a/b/c" -> "/a/b", "/a" -> "/")
fn parent_path(path: &str) -> Option<String> {
    if path == "/" {
        return None;
    }
    let trimmed = path.trim_end_matches('/');
    match trimmed.rfind('/') {
        Some(0) => Some("/".to_string()),
        Some(pos) => Some(trimmed[..pos].to_string()),
        None => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_create_folder() {
        let tmp = TempDir::new().unwrap();
        let mut store = FolderStore::open(tmp.path()).unwrap();

        store.create_folder(VaultFolder {
            path: "/projects".to_string(),
            label: Some("Projects".to_string()),
            default_access: Permission::READ,
            grants: vec![],
            inherit: true,
        }).unwrap();

        let folder = store.get("/projects").unwrap();
        assert_eq!(folder.label.as_deref(), Some("Projects"));
    }

    #[test]
    fn test_path_validation() {
        assert!(validate_folder_path("/valid/path").is_ok());
        assert!(validate_folder_path("/").is_ok());
        assert!(validate_folder_path("no-slash").is_err());
        assert!(validate_folder_path("/bad/../escape").is_err());
        assert!(validate_folder_path("").is_err());
        assert!(validate_folder_path(&"/".repeat(261)).is_err());
    }

    #[test]
    fn test_parent_path() {
        assert_eq!(parent_path("/a/b/c"), Some("/a/b".to_string()));
        assert_eq!(parent_path("/a/b"), Some("/a".to_string()));
        assert_eq!(parent_path("/a"), Some("/".to_string()));
        assert_eq!(parent_path("/"), None);
    }

    #[test]
    fn test_grant_lifecycle() {
        let tmp = TempDir::new().unwrap();
        let mut store = FolderStore::open(tmp.path()).unwrap();

        store.create_folder(VaultFolder {
            path: "/shared".to_string(),
            label: None,
            default_access: Permission::NONE,
            grants: vec![],
            inherit: false,
        }).unwrap();

        let grant = AccessGrant {
            target: GrantTarget::Folder("/shared".to_string()),
            grantee: Grantee::Contact("did:nexus:alice".to_string()),
            level: Permission::READ_WRITE,
            expires: None,
        };
        store.add_grant("/shared", grant).unwrap();

        let folder = store.get("/shared").unwrap();
        assert_eq!(folder.grants.len(), 1);

        store.remove_grants("/shared", &Grantee::Contact("did:nexus:alice".to_string())).unwrap();
        let folder = store.get("/shared").unwrap();
        assert_eq!(folder.grants.len(), 0);
    }

    #[test]
    fn test_effective_permission_resolution() {
        let tmp = TempDir::new().unwrap();

        // Set up contacts
        let mut contacts = ContactStore::open(tmp.path()).unwrap();
        contacts.add(super::super::contact::Contact {
            did: "did:nexus:alice".to_string(),
            label: "Alice".to_string(),
            peer_id: None,
            pre_pk: vec![],
            access: Permission::READ, // global default
            groups: vec![],
            created_at: 0,
            updated_at: 0,
        }).unwrap();

        let groups = GroupStore::open(tmp.path()).unwrap();

        // Set up folders with grants
        let mut folders = FolderStore::open(tmp.path()).unwrap();
        folders.create_folder(VaultFolder {
            path: "/projects".to_string(),
            label: None,
            default_access: Permission::NONE,
            grants: vec![
                AccessGrant {
                    target: GrantTarget::Folder("/projects".to_string()),
                    grantee: Grantee::Contact("did:nexus:alice".to_string()),
                    level: Permission::READ_WRITE,  // elevated from contact default
                    expires: None,
                },
            ],
            inherit: true,
        }).unwrap();

        // Folder grant takes precedence over contact default
        let perm = folders.effective_permission(
            "did:nexus:alice", "/projects", None, &contacts, &groups);
        assert_eq!(perm, Permission::READ_WRITE);

        // Unknown folder falls back to contact default
        let perm = folders.effective_permission(
            "did:nexus:alice", "/other", None, &contacts, &groups);
        assert_eq!(perm, Permission::READ);

        // Unknown DID gets NONE
        let perm = folders.effective_permission(
            "did:nexus:unknown", "/projects", None, &contacts, &groups);
        assert_eq!(perm, Permission::NONE);
    }

    #[test]
    fn test_asset_level_override() {
        let tmp = TempDir::new().unwrap();

        let mut contacts = ContactStore::open(tmp.path()).unwrap();
        contacts.add(super::super::contact::Contact {
            did: "did:nexus:bob".to_string(),
            label: "Bob".to_string(),
            peer_id: None,
            pre_pk: vec![],
            access: Permission::READ,
            groups: vec![],
            created_at: 0,
            updated_at: 0,
        }).unwrap();

        let groups = GroupStore::open(tmp.path()).unwrap();

        let mut folders = FolderStore::open(tmp.path()).unwrap();
        folders.create_folder(VaultFolder {
            path: "/docs".to_string(),
            label: None,
            default_access: Permission::NONE,
            grants: vec![
                // Folder-level: read only
                AccessGrant {
                    target: GrantTarget::Folder("/docs".to_string()),
                    grantee: Grantee::Contact("did:nexus:bob".to_string()),
                    level: Permission::READ,
                    expires: None,
                },
                // Asset-level: full access to specific asset
                AccessGrant {
                    target: GrantTarget::Asset("asset-123".to_string()),
                    grantee: Grantee::Contact("did:nexus:bob".to_string()),
                    level: Permission::FULL,
                    expires: None,
                },
            ],
            inherit: false,
        }).unwrap();

        // Without asset_id: folder grant (READ)
        let perm = folders.effective_permission(
            "did:nexus:bob", "/docs", None, &contacts, &groups);
        assert_eq!(perm, Permission::READ);

        // With asset_id: asset grant (FULL) takes precedence
        let perm = folders.effective_permission(
            "did:nexus:bob", "/docs", Some("asset-123"), &contacts, &groups);
        assert_eq!(perm, Permission::FULL);
    }
}
