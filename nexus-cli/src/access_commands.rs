//! CLI commands for access control management (Phase 3)
//!
//! Provides `nexus contact *` and `nexus folder *` subcommands that
//! operate on the nexus-core access control stores.

use nexus_core::access::{
    contact::{Contact, ContactStore},
    folder::{AccessGrant, FolderStore, GrantTarget, Grantee, VaultFolder},
    group::{Group, GroupStore},
    permission::Permission,
};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

/// Default store directory
#[allow(dead_code)]
const DEFAULT_STORE_DIR: &str = ".nexus-store";

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
        other => {
            // Try parsing as u8
            other.parse::<u8>()
                .map(|bits| Permission::from_bits_truncate(bits))
                .map_err(|_| format!("Invalid permission: '{}'. Use: none, read, write/rw, full/rwm, or a number 0-7", other))
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// CONTACT commands
// ═══════════════════════════════════════════════════════════════════════════════

/// Add a contact
pub fn contact_add(
    store_dir: &str,
    did: &str,
    label: &str,
    access: &str,
    peer_id: Option<&str>,
) -> Result<(), String> {
    let dir = Path::new(store_dir);
    let mut store = ContactStore::open(dir)
        .map_err(|e| format!("Failed to open contact store: {}", e))?;

    let perm = parse_permission(access)?;
    let now = now_epoch();

    let contact = Contact {
        did: did.to_string(),
        label: label.to_string(),
        peer_id: peer_id.map(|s| s.to_string()),
        pre_pk: vec![], // Filled in via join handshake or import
        access: perm,
        groups: vec![],
        created_at: now,
        updated_at: now,
    };

    store.add(contact)
        .map_err(|e| format!("Failed to add contact: {}", e))?;

    println!("✓ Added contact '{}' ({})", label, did);
    println!("  Access: {}", perm);
    Ok(())
}

/// Remove a contact
pub fn contact_remove(store_dir: &str, did: &str) -> Result<(), String> {
    let dir = Path::new(store_dir);
    let mut store = ContactStore::open(dir)
        .map_err(|e| format!("Failed to open contact store: {}", e))?;

    let label = store.get(did).map(|c| c.label.clone());

    store.remove(did)
        .map_err(|e| format!("Failed to remove contact: {}", e))?;

    if let Some(label) = label {
        println!("✓ Removed contact '{}' ({})", label, did);
    } else {
        println!("✓ Removed contact {}", did);
    }
    Ok(())
}

/// List all contacts
pub fn contact_list(store_dir: &str) -> Result<(), String> {
    let dir = Path::new(store_dir);
    let store = ContactStore::open(dir)
        .map_err(|e| format!("Failed to open contact store: {}", e))?;

    let contacts = store.list();
    if contacts.is_empty() {
        println!("No contacts.");
        return Ok(());
    }

    println!("{:<20} {:<40} {:<10} {}", "LABEL", "DID", "ACCESS", "GROUPS");
    println!("{}", "-".repeat(80));
    for c in contacts {
        let groups = if c.groups.is_empty() {
            "—".to_string()
        } else {
            c.groups.join(", ")
        };
        println!("{:<20} {:<40} {:<10} {}", c.label, c.did, c.access, groups);
    }
    Ok(())
}

/// Update a contact's access level
pub fn contact_set_access(store_dir: &str, did: &str, access: &str) -> Result<(), String> {
    let dir = Path::new(store_dir);
    let mut store = ContactStore::open(dir)
        .map_err(|e| format!("Failed to open contact store: {}", e))?;

    let perm = parse_permission(access)?;

    store.update_access(did, perm)
        .map_err(|e| format!("Failed to update access: {}", e))?;

    let label = store.get(did).map(|c| c.label.as_str()).unwrap_or(did);
    println!("✓ Updated access for '{}': {}", label, perm);
    Ok(())
}

/// Show a single contact's details
pub fn contact_show(store_dir: &str, did: &str) -> Result<(), String> {
    let dir = Path::new(store_dir);
    let store = ContactStore::open(dir)
        .map_err(|e| format!("Failed to open contact store: {}", e))?;

    let contact = store.get(did)
        .ok_or_else(|| format!("Contact not found: {}", did))?;

    println!("Label:      {}", contact.label);
    println!("DID:        {}", contact.did);
    println!("Access:     {}", contact.access);
    println!("Peer ID:    {}", contact.peer_id.as_deref().unwrap_or("—"));
    println!("PRE PK:     {} bytes", contact.pre_pk.len());
    println!("Groups:     {}", if contact.groups.is_empty() { "—".to_string() } else { contact.groups.join(", ") });
    println!("Created:    {}", contact.created_at);
    println!("Updated:    {}", contact.updated_at);
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// GROUP commands
// ═══════════════════════════════════════════════════════════════════════════════

/// Create a group
pub fn group_create(store_dir: &str, name: &str, _description: Option<&str>) -> Result<(), String> {
    let dir = Path::new(store_dir);
    let mut store = GroupStore::open(dir)
        .map_err(|e| format!("Failed to open group store: {}", e))?;

    store.create(Group {
        id: name.to_lowercase().replace(' ', "-"),
        name: name.to_string(),
        members: vec![],
        default_access: Permission::NONE,
    }).map_err(|e| format!("Failed to create group: {}", e))?;

    println!("✓ Created group '{}'", name);
    Ok(())
}

/// Delete a group
pub fn group_delete(store_dir: &str, name: &str) -> Result<(), String> {
    let dir = Path::new(store_dir);
    let mut store = GroupStore::open(dir)
        .map_err(|e| format!("Failed to open group store: {}", e))?;

    store.delete(name)
        .map_err(|e| format!("Failed to delete group: {}", e))?;

    println!("✓ Deleted group '{}'", name);
    Ok(())
}

/// Add a member to a group
pub fn group_add_member(store_dir: &str, group_name: &str, did: &str) -> Result<(), String> {
    let dir = Path::new(store_dir);
    let mut store = GroupStore::open(dir)
        .map_err(|e| format!("Failed to open group store: {}", e))?;

    store.add_member(group_name, did)
        .map_err(|e| format!("Failed to add member: {}", e))?;

    println!("✓ Added {} to group '{}'", did, group_name);
    Ok(())
}

/// Remove a member from a group
pub fn group_remove_member(store_dir: &str, group_name: &str, did: &str) -> Result<(), String> {
    let dir = Path::new(store_dir);
    let mut store = GroupStore::open(dir)
        .map_err(|e| format!("Failed to open group store: {}", e))?;

    store.remove_member(group_name, did)
        .map_err(|e| format!("Failed to remove member: {}", e))?;

    println!("✓ Removed {} from group '{}'", did, group_name);
    Ok(())
}

/// List all groups
pub fn group_list(store_dir: &str) -> Result<(), String> {
    let dir = Path::new(store_dir);
    let store = GroupStore::open(dir)
        .map_err(|e| format!("Failed to open group store: {}", e))?;

    let groups = store.list();
    if groups.is_empty() {
        println!("No groups.");
        return Ok(());
    }

    println!("{:<20} {:<10} {}", "NAME", "MEMBERS", "DEFAULT ACCESS");
    println!("{}", "-".repeat(60));
    for g in groups {
        println!("{:<20} {:<10} {}", g.name, g.members.len(), g.default_access);
    }
    Ok(())
}

/// Show a group's members
pub fn group_show(store_dir: &str, name: &str) -> Result<(), String> {
    let dir = Path::new(store_dir);
    let store = GroupStore::open(dir)
        .map_err(|e| format!("Failed to open group store: {}", e))?;

    let group = store.get(name)
        .ok_or_else(|| format!("Group not found: {}", name))?;

    println!("Group: {}", group.name);
    println!("ID: {}", group.id);
    println!("Default access: {}", group.default_access);
    println!("Members ({}):", group.members.len());
    for did in &group.members {
        println!("  • {}", did);
    }
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// FOLDER commands
// ═══════════════════════════════════════════════════════════════════════════════

/// Create a vault folder
pub fn folder_create(
    store_dir: &str,
    path: &str,
    label: Option<&str>,
    default_access: &str,
    inherit: bool,
) -> Result<(), String> {
    let dir = Path::new(store_dir);
    let mut store = FolderStore::open(dir)
        .map_err(|e| format!("Failed to open folder store: {}", e))?;

    let perm = parse_permission(default_access)?;

    store.create_folder(VaultFolder {
        path: path.to_string(),
        label: label.map(|s| s.to_string()),
        default_access: perm,
        grants: vec![],
        inherit,
    }).map_err(|e| format!("Failed to create folder: {}", e))?;

    println!("✓ Created folder '{}'", path);
    println!("  Default access: {}", perm);
    println!("  Inherits from parent: {}", inherit);
    Ok(())
}

/// Remove a vault folder
pub fn folder_remove(store_dir: &str, path: &str) -> Result<(), String> {
    let dir = Path::new(store_dir);
    let mut store = FolderStore::open(dir)
        .map_err(|e| format!("Failed to open folder store: {}", e))?;

    let removed = store.delete_folder(path)
        .map_err(|e| format!("Failed to remove folder: {}", e))?;

    if removed {
        println!("✓ Removed folder '{}'", path);
    } else {
        println!("Folder '{}' not found", path);
    }
    Ok(())
}

/// List all folders
pub fn folder_list(store_dir: &str) -> Result<(), String> {
    let dir = Path::new(store_dir);
    let store = FolderStore::open(dir)
        .map_err(|e| format!("Failed to open folder store: {}", e))?;

    let folders = store.list();
    if folders.is_empty() {
        println!("No folders.");
        return Ok(());
    }

    println!("{:<30} {:<15} {:<10} {}", "PATH", "LABEL", "DEFAULT", "GRANTS");
    println!("{}", "-".repeat(70));
    for f in folders {
        let label = f.label.as_deref().unwrap_or("—");
        println!("{:<30} {:<15} {:<10} {}", f.path, label, f.default_access, f.grants.len());
    }
    Ok(())
}

/// Grant access to a folder (or asset within a folder)
pub fn folder_grant(
    store_dir: &str,
    folder_path: &str,
    grantee_did: &str,
    access: &str,
    asset_id: Option<&str>,
    is_group: bool,
) -> Result<(), String> {
    let dir = Path::new(store_dir);
    let mut store = FolderStore::open(dir)
        .map_err(|e| format!("Failed to open folder store: {}", e))?;

    let perm = parse_permission(access)?;

    let target = match asset_id {
        Some(id) => GrantTarget::Asset(id.to_string()),
        None => GrantTarget::Folder(folder_path.to_string()),
    };

    let grantee = if is_group {
        Grantee::Group(grantee_did.to_string())
    } else {
        Grantee::Contact(grantee_did.to_string())
    };

    let grant = AccessGrant {
        target,
        grantee: grantee.clone(),
        level: perm,
        expires: None,
    };

    store.add_grant(folder_path, grant)
        .map_err(|e| format!("Failed to add grant: {}", e))?;

    let target_desc = match asset_id {
        Some(id) => format!("asset {} in {}", id, folder_path),
        None => folder_path.to_string(),
    };

    let grantee_desc = if is_group {
        format!("group:{}", grantee_did)
    } else {
        grantee_did.to_string()
    };

    println!("✓ Granted {} to {} on {}", perm, grantee_desc, target_desc);
    Ok(())
}

/// Revoke a grant from a folder
pub fn folder_revoke(
    store_dir: &str,
    folder_path: &str,
    grantee_did: &str,
    is_group: bool,
) -> Result<(), String> {
    let dir = Path::new(store_dir);
    let mut store = FolderStore::open(dir)
        .map_err(|e| format!("Failed to open folder store: {}", e))?;

    let grantee = if is_group {
        Grantee::Group(grantee_did.to_string())
    } else {
        Grantee::Contact(grantee_did.to_string())
    };

    store.remove_grants(folder_path, &grantee)
        .map_err(|e| format!("Failed to revoke grant: {}", e))?;

    let grantee_desc = if is_group {
        format!("group:{}", grantee_did)
    } else {
        grantee_did.to_string()
    };

    println!("✓ Revoked grant for {} on {}", grantee_desc, folder_path);
    Ok(())
}

/// Show a folder's details and grants
pub fn folder_show(store_dir: &str, path: &str) -> Result<(), String> {
    let dir = Path::new(store_dir);
    let store = FolderStore::open(dir)
        .map_err(|e| format!("Failed to open folder store: {}", e))?;

    let folder = store.get(path)
        .ok_or_else(|| format!("Folder not found: {}", path))?;

    println!("Path:           {}", folder.path);
    println!("Label:          {}", folder.label.as_deref().unwrap_or("—"));
    println!("Default access: {}", folder.default_access);
    println!("Inherit parent: {}", folder.inherit);
    println!();

    if folder.grants.is_empty() {
        println!("No grants.");
    } else {
        println!("Grants:");
        for g in &folder.grants {
            let target = match &g.target {
                GrantTarget::Folder(p) => format!("folder:{}", p),
                GrantTarget::Asset(id) => format!("asset:{}", id),
            };
            let grantee = match &g.grantee {
                Grantee::Contact(d) => d.clone(),
                Grantee::Group(n) => format!("group:{}", n),
            };
            let expiry = g.expires
                .map(|e| format!(" (expires: {})", e))
                .unwrap_or_default();
            println!("  {} → {} [{}]{}", grantee, target, g.level, expiry);
        }
    }
    Ok(())
}

/// Check effective permission for a DID on a folder/asset
pub fn folder_check(
    store_dir: &str,
    did: &str,
    folder_path: &str,
    asset_id: Option<&str>,
) -> Result<(), String> {
    let dir = Path::new(store_dir);
    let contacts = ContactStore::open(dir)
        .map_err(|e| format!("Failed to open contact store: {}", e))?;
    let folders = FolderStore::open(dir)
        .map_err(|e| format!("Failed to open folder store: {}", e))?;
    let groups = GroupStore::open(dir)
        .map_err(|e| format!("Failed to open group store: {}", e))?;

    let perm = folders.effective_permission(did, folder_path, asset_id, &contacts, &groups);

    println!("DID:         {}", did);
    println!("Folder:      {}", folder_path);
    if let Some(id) = asset_id {
        println!("Asset:       {}", id);
    }
    println!("Effective:   {}", perm);

    // Also check if push/pull would succeed
    let push_ok = perm.satisfies(Permission::WRITE);
    let pull_ok = perm.satisfies(Permission::READ);
    println!();
    println!("Can push: {}", if push_ok { "✓" } else { "✗" });
    println!("Can pull: {}", if pull_ok { "✓" } else { "✗" });

    Ok(())
}
