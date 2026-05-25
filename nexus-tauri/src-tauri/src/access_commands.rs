//! Tauri IPC commands for access control (contacts, groups, folders, grants).
//!
//! These wrap the `nexus_core::access` module for use from the frontend.

use nexus_core::access::{
    contact::{Contact as CoreContact, ContactStore},
    folder::{AccessGrant, FolderStore, GrantTarget, Grantee, VaultFolder},
    group::{Group as CoreGroup, GroupStore},
    permission::Permission,
};
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

const STORE_DIR: &str = ".nexus-store";

fn now_epoch() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn parse_permission(s: &str) -> Result<Permission, String> {
    match s.to_lowercase().as_str() {
        "none" | "0" => Ok(Permission::NONE),
        "read" | "r" | "1" => Ok(Permission::READ),
        "write" | "rw" | "read-write" | "readwrite" | "3" => Ok(Permission::READ_WRITE),
        "full" | "rwm" | "read-write-modify" | "7" => Ok(Permission::FULL),
        "modify" | "m" | "4" => Ok(Permission::MODIFY),
        other => other
            .parse::<u8>()
            .map(|bits| Permission::from_bits_truncate(bits))
            .map_err(|_| format!("Invalid permission: '{}'", other)),
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// DTOs for the frontend
// ═══════════════════════════════════════════════════════════════════════════════

#[derive(Serialize, Deserialize, Clone)]
pub struct AccessContact {
    pub did: String,
    pub label: String,
    pub peer_id: Option<String>,
    pub access: u8,
    pub access_label: String,
    pub groups: Vec<String>,
    pub created_at: u64,
    pub updated_at: u64,
}

impl From<&CoreContact> for AccessContact {
    fn from(c: &CoreContact) -> Self {
        AccessContact {
            did: c.did.clone(),
            label: c.label.clone(),
            peer_id: c.peer_id.clone(),
            access: c.access.bits(),
            access_label: c.access.to_string(),
            groups: c.groups.clone(),
            created_at: c.created_at,
            updated_at: c.updated_at,
        }
    }
}

#[derive(Serialize, Deserialize, Clone)]
pub struct AccessGroup {
    pub id: String,
    pub name: String,
    pub members: Vec<String>,
    pub default_access: u8,
    pub default_access_label: String,
}

impl From<&CoreGroup> for AccessGroup {
    fn from(g: &CoreGroup) -> Self {
        AccessGroup {
            id: g.id.clone(),
            name: g.name.clone(),
            members: g.members.clone(),
            default_access: g.default_access.bits(),
            default_access_label: g.default_access.to_string(),
        }
    }
}

#[derive(Serialize, Deserialize, Clone)]
pub struct AccessFolder {
    pub path: String,
    pub label: Option<String>,
    pub default_access: u8,
    pub default_access_label: String,
    pub inherit: bool,
    pub grant_count: usize,
}

impl From<&VaultFolder> for AccessFolder {
    fn from(f: &VaultFolder) -> Self {
        AccessFolder {
            path: f.path.clone(),
            label: f.label.clone(),
            default_access: f.default_access.bits(),
            default_access_label: f.default_access.to_string(),
            inherit: f.inherit,
            grant_count: f.grants.len(),
        }
    }
}

#[derive(Serialize, Deserialize, Clone)]
pub struct AccessGrantInfo {
    pub target_type: String, // "folder" or "asset"
    pub target_id: String,
    pub grantee_type: String, // "contact" or "group"
    pub grantee_id: String,
    pub level: u8,
    pub level_label: String,
    pub expires: Option<u64>,
}

impl From<&AccessGrant> for AccessGrantInfo {
    fn from(g: &AccessGrant) -> Self {
        let (target_type, target_id) = match &g.target {
            GrantTarget::Folder(p) => ("folder".to_string(), p.clone()),
            GrantTarget::Asset(id) => ("asset".to_string(), id.clone()),
        };
        let (grantee_type, grantee_id) = match &g.grantee {
            Grantee::Contact(d) => ("contact".to_string(), d.clone()),
            Grantee::Group(n) => ("group".to_string(), n.clone()),
        };
        AccessGrantInfo {
            target_type,
            target_id,
            grantee_type,
            grantee_id,
            level: g.level.bits(),
            level_label: g.level.to_string(),
            expires: g.expires,
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct PermissionCheck {
    pub did: String,
    pub folder: String,
    pub asset: Option<String>,
    pub effective: u8,
    pub effective_label: String,
    pub can_push: bool,
    pub can_pull: bool,
}

#[derive(Serialize, Deserialize)]
pub struct FolderDetail {
    pub folder: AccessFolder,
    pub grants: Vec<AccessGrantInfo>,
}

// ═══════════════════════════════════════════════════════════════════════════════
// CONTACT IPC commands
// ═══════════════════════════════════════════════════════════════════════════════

#[tauri::command]
pub fn acl_contact_add(
    did: &str,
    label: &str,
    access: &str,
    peer_id: Option<&str>,
) -> Result<AccessContact, String> {
    let mut store = ContactStore::open(STORE_DIR)
        .map_err(|e| format!("Failed to open contact store: {}", e))?;

    let perm = parse_permission(access)?;
    let now = now_epoch();

    let contact = CoreContact {
        did: did.to_string(),
        label: label.to_string(),
        peer_id: peer_id.map(|s| s.to_string()),
        pre_pk: vec![],
        access: perm,
        groups: vec![],
        created_at: now,
        updated_at: now,
    };

    store.add(contact.clone())
        .map_err(|e| format!("Failed to add contact: {}", e))?;

    Ok(AccessContact::from(&contact))
}

#[tauri::command]
pub fn acl_contact_remove(did: &str) -> Result<(), String> {
    let mut store = ContactStore::open(STORE_DIR)
        .map_err(|e| format!("Failed to open contact store: {}", e))?;
    store.remove(did)
        .map_err(|e| format!("Failed to remove contact: {}", e))?;
    Ok(())
}

#[tauri::command]
pub fn acl_contact_list() -> Result<Vec<AccessContact>, String> {
    let store = ContactStore::open(STORE_DIR)
        .map_err(|e| format!("Failed to open contact store: {}", e))?;
    Ok(store.list().iter().map(AccessContact::from).collect())
}

#[tauri::command]
pub fn acl_contact_get(did: &str) -> Result<AccessContact, String> {
    let store = ContactStore::open(STORE_DIR)
        .map_err(|e| format!("Failed to open contact store: {}", e))?;
    store.get(did)
        .map(AccessContact::from)
        .ok_or_else(|| format!("Contact not found: {}", did))
}

#[tauri::command]
pub fn acl_contact_set_access(did: &str, access: &str) -> Result<AccessContact, String> {
    let mut store = ContactStore::open(STORE_DIR)
        .map_err(|e| format!("Failed to open contact store: {}", e))?;

    let perm = parse_permission(access)?;
    store.update_access(did, perm)
        .map_err(|e| format!("Failed to update access: {}", e))?;

    store.get(did)
        .map(AccessContact::from)
        .ok_or_else(|| format!("Contact not found after update: {}", did))
}

// ═══════════════════════════════════════════════════════════════════════════════
// GROUP IPC commands
// ═══════════════════════════════════════════════════════════════════════════════

#[tauri::command]
pub fn acl_group_create(name: &str) -> Result<AccessGroup, String> {
    let mut store = GroupStore::open(STORE_DIR)
        .map_err(|e| format!("Failed to open group store: {}", e))?;

    let group = CoreGroup {
        id: name.to_lowercase().replace(' ', "-"),
        name: name.to_string(),
        members: vec![],
        default_access: Permission::NONE,
    };

    store.create(group.clone())
        .map_err(|e| format!("Failed to create group: {}", e))?;

    Ok(AccessGroup::from(&group))
}

#[tauri::command]
pub fn acl_group_delete(name: &str) -> Result<(), String> {
    let mut store = GroupStore::open(STORE_DIR)
        .map_err(|e| format!("Failed to open group store: {}", e))?;
    store.delete(name)
        .map_err(|e| format!("Failed to delete group: {}", e))?;
    Ok(())
}

#[tauri::command]
pub fn acl_group_list() -> Result<Vec<AccessGroup>, String> {
    let store = GroupStore::open(STORE_DIR)
        .map_err(|e| format!("Failed to open group store: {}", e))?;
    Ok(store.list().iter().map(AccessGroup::from).collect())
}

#[tauri::command]
pub fn acl_group_get(name: &str) -> Result<AccessGroup, String> {
    let store = GroupStore::open(STORE_DIR)
        .map_err(|e| format!("Failed to open group store: {}", e))?;
    store.get(name)
        .map(AccessGroup::from)
        .ok_or_else(|| format!("Group not found: {}", name))
}

#[tauri::command]
pub fn acl_group_add_member(group: &str, did: &str) -> Result<AccessGroup, String> {
    let mut store = GroupStore::open(STORE_DIR)
        .map_err(|e| format!("Failed to open group store: {}", e))?;
    store.add_member(group, did)
        .map_err(|e| format!("Failed to add member: {}", e))?;
    store.get(group)
        .map(AccessGroup::from)
        .ok_or_else(|| format!("Group not found: {}", group))
}

#[tauri::command]
pub fn acl_group_remove_member(group: &str, did: &str) -> Result<AccessGroup, String> {
    let mut store = GroupStore::open(STORE_DIR)
        .map_err(|e| format!("Failed to open group store: {}", e))?;
    store.remove_member(group, did)
        .map_err(|e| format!("Failed to remove member: {}", e))?;
    store.get(group)
        .map(AccessGroup::from)
        .ok_or_else(|| format!("Group not found: {}", group))
}

// ═══════════════════════════════════════════════════════════════════════════════
// FOLDER IPC commands
// ═══════════════════════════════════════════════════════════════════════════════

#[tauri::command]
pub fn acl_folder_create(
    path: &str,
    label: Option<&str>,
    default_access: &str,
    inherit: bool,
) -> Result<AccessFolder, String> {
    let mut store = FolderStore::open(STORE_DIR)
        .map_err(|e| format!("Failed to open folder store: {}", e))?;

    let perm = parse_permission(default_access)?;

    let folder = VaultFolder {
        path: path.to_string(),
        label: label.map(|s| s.to_string()),
        default_access: perm,
        grants: vec![],
        inherit,
    };

    store.create_folder(folder.clone())
        .map_err(|e| format!("Failed to create folder: {}", e))?;

    Ok(AccessFolder::from(&folder))
}

#[tauri::command]
pub fn acl_folder_remove(path: &str) -> Result<(), String> {
    let mut store = FolderStore::open(STORE_DIR)
        .map_err(|e| format!("Failed to open folder store: {}", e))?;
    store.delete_folder(path)
        .map_err(|e| format!("Failed to remove folder: {}", e))?;
    Ok(())
}

#[tauri::command]
pub fn acl_folder_list() -> Result<Vec<AccessFolder>, String> {
    let store = FolderStore::open(STORE_DIR)
        .map_err(|e| format!("Failed to open folder store: {}", e))?;
    Ok(store.list().iter().map(AccessFolder::from).collect())
}

#[tauri::command]
pub fn acl_folder_get(path: &str) -> Result<FolderDetail, String> {
    let store = FolderStore::open(STORE_DIR)
        .map_err(|e| format!("Failed to open folder store: {}", e))?;
    let folder = store.get(path)
        .ok_or_else(|| format!("Folder not found: {}", path))?;
    Ok(FolderDetail {
        folder: AccessFolder::from(folder),
        grants: folder.grants.iter().map(AccessGrantInfo::from).collect(),
    })
}

#[tauri::command]
pub fn acl_folder_grant(
    folder_path: &str,
    grantee: &str,
    access: &str,
    is_group: bool,
    asset_id: Option<&str>,
) -> Result<FolderDetail, String> {
    let mut store = FolderStore::open(STORE_DIR)
        .map_err(|e| format!("Failed to open folder store: {}", e))?;

    let perm = parse_permission(access)?;

    let target = match asset_id {
        Some(id) => GrantTarget::Asset(id.to_string()),
        None => GrantTarget::Folder(folder_path.to_string()),
    };

    let grantee_val = if is_group {
        Grantee::Group(grantee.to_string())
    } else {
        Grantee::Contact(grantee.to_string())
    };

    let grant = AccessGrant {
        target,
        grantee: grantee_val,
        level: perm,
        expires: None,
    };

    store.add_grant(folder_path, grant)
        .map_err(|e| format!("Failed to add grant: {}", e))?;

    let folder = store.get(folder_path)
        .ok_or_else(|| format!("Folder not found: {}", folder_path))?;
    Ok(FolderDetail {
        folder: AccessFolder::from(folder),
        grants: folder.grants.iter().map(AccessGrantInfo::from).collect(),
    })
}

#[tauri::command]
pub fn acl_folder_revoke(
    folder_path: &str,
    grantee: &str,
    is_group: bool,
) -> Result<FolderDetail, String> {
    let mut store = FolderStore::open(STORE_DIR)
        .map_err(|e| format!("Failed to open folder store: {}", e))?;

    let grantee_val = if is_group {
        Grantee::Group(grantee.to_string())
    } else {
        Grantee::Contact(grantee.to_string())
    };

    store.remove_grants(folder_path, &grantee_val)
        .map_err(|e| format!("Failed to revoke grant: {}", e))?;

    let folder = store.get(folder_path)
        .ok_or_else(|| format!("Folder not found: {}", folder_path))?;
    Ok(FolderDetail {
        folder: AccessFolder::from(folder),
        grants: folder.grants.iter().map(AccessGrantInfo::from).collect(),
    })
}

#[tauri::command]
pub fn acl_check_permission(
    did: &str,
    folder_path: &str,
    asset_id: Option<&str>,
) -> Result<PermissionCheck, String> {
    let contacts = ContactStore::open(STORE_DIR)
        .map_err(|e| format!("Failed to open contact store: {}", e))?;
    let folders = FolderStore::open(STORE_DIR)
        .map_err(|e| format!("Failed to open folder store: {}", e))?;
    let groups = GroupStore::open(STORE_DIR)
        .map_err(|e| format!("Failed to open group store: {}", e))?;

    let perm = folders.effective_permission(did, folder_path, asset_id, &contacts, &groups);

    Ok(PermissionCheck {
        did: did.to_string(),
        folder: folder_path.to_string(),
        asset: asset_id.map(|s| s.to_string()),
        effective: perm.bits(),
        effective_label: perm.to_string(),
        can_push: perm.satisfies(Permission::WRITE),
        can_pull: perm.satisfies(Permission::READ),
    })
}
