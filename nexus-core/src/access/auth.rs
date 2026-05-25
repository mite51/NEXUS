//! Push authorization — verifies a sender has permission to push to a folder
//!
//! This is the core auth check called when receiving a PushRequest.

use thiserror::Error;

use super::contact::ContactStore;
use super::folder::FolderStore;
use super::group::GroupStore;
use super::permission::Permission;

/// Errors from push authorization checks
#[derive(Debug, Error, PartialEq, Eq)]
pub enum AuthError {
    #[error("unknown contact: {did}")]
    UnknownContact { did: String },

    #[error("insufficient permission: {did} has '{actual}' but needs '{required}' on {folder}")]
    InsufficientPermission {
        did: String,
        folder: String,
        actual: String,
        required: String,
    },

    #[error("folder not found: {path}")]
    FolderNotFound { path: String },
}

/// Authorize a push request from a sender to a target folder.
///
/// Checks:
/// 1. Sender must be a known contact
/// 2. Target folder must exist
/// 3. Effective permission must include WRITE (for new assets) or MODIFY (for overwrites)
pub fn authorize_push(
    sender_did: &str,
    target_folder: &str,
    asset_id: Option<&str>,
    contacts: &ContactStore,
    folders: &FolderStore,
    groups: &GroupStore,
) -> Result<(), AuthError> {
    // 1. Must be a known contact
    if !contacts.is_known(sender_did) {
        return Err(AuthError::UnknownContact {
            did: sender_did.to_string(),
        });
    }

    // 2. Target folder must exist
    if folders.get(target_folder).is_none() {
        return Err(AuthError::FolderNotFound {
            path: target_folder.to_string(),
        });
    }

    // 3. Check effective permission
    let effective = folders.effective_permission(
        sender_did,
        target_folder,
        asset_id,
        contacts,
        groups,
    );

    // Push requires at least WRITE
    let required = if asset_id.is_some() {
        // Overwriting existing asset requires MODIFY
        Permission::WRITE | Permission::MODIFY
    } else {
        // New asset just needs WRITE
        Permission::WRITE
    };

    if !effective.satisfies(Permission::WRITE) {
        return Err(AuthError::InsufficientPermission {
            did: sender_did.to_string(),
            folder: target_folder.to_string(),
            actual: effective.to_string(),
            required: required.to_string(),
        });
    }

    Ok(())
}

/// Authorize a pull request (requires READ permission)
pub fn authorize_pull(
    requester_did: &str,
    folder_path: &str,
    asset_id: Option<&str>,
    contacts: &ContactStore,
    folders: &FolderStore,
    groups: &GroupStore,
) -> Result<(), AuthError> {
    // Must be a known contact
    if !contacts.is_known(requester_did) {
        return Err(AuthError::UnknownContact {
            did: requester_did.to_string(),
        });
    }

    // Check effective permission
    let effective = folders.effective_permission(
        requester_did,
        folder_path,
        asset_id,
        contacts,
        groups,
    );

    if !effective.satisfies(Permission::READ) {
        return Err(AuthError::InsufficientPermission {
            did: requester_did.to_string(),
            folder: folder_path.to_string(),
            actual: effective.to_string(),
            required: Permission::READ.to_string(),
        });
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::access::contact::Contact;
    use crate::access::folder::{AccessGrant, GrantTarget, Grantee, VaultFolder};
    use crate::access::group::Group;
    use tempfile::TempDir;

    fn setup_stores(tmp: &std::path::Path) -> (ContactStore, FolderStore, GroupStore) {
        let mut contacts = ContactStore::open(tmp).unwrap();
        let mut folders = FolderStore::open(tmp).unwrap();
        let mut groups = GroupStore::open(tmp).unwrap();

        // Add contacts
        contacts.add(Contact {
            did: "did:nexus:alice".to_string(),
            label: "Alice".to_string(),
            peer_id: None,
            pre_pk: vec![],
            access: Permission::READ_WRITE,
            groups: vec!["team".to_string()],
            created_at: 0,
            updated_at: 0,
        }).unwrap();

        contacts.add(Contact {
            did: "did:nexus:bob".to_string(),
            label: "Bob".to_string(),
            peer_id: None,
            pre_pk: vec![],
            access: Permission::READ, // read-only default
            groups: vec![],
            created_at: 0,
            updated_at: 0,
        }).unwrap();

        // Add group
        groups.create(Group {
            id: "team".to_string(),
            name: "Team".to_string(),
            members: vec!["did:nexus:alice".to_string()],
            default_access: Permission::READ_WRITE,
        }).unwrap();

        // Add folders
        folders.create_folder(VaultFolder {
            path: "/projects".to_string(),
            label: Some("Projects".to_string()),
            default_access: Permission::NONE,
            grants: vec![
                AccessGrant {
                    target: GrantTarget::Folder("/projects".to_string()),
                    grantee: Grantee::Group("team".to_string()),
                    level: Permission::READ_WRITE,
                    expires: None,
                },
            ],
            inherit: true,
        }).unwrap();

        folders.create_folder(VaultFolder {
            path: "/readonly".to_string(),
            label: None,
            default_access: Permission::READ,
            grants: vec![
                AccessGrant {
                    target: GrantTarget::Folder("/readonly".to_string()),
                    grantee: Grantee::Contact("did:nexus:bob".to_string()),
                    level: Permission::READ,
                    expires: None,
                },
            ],
            inherit: false,
        }).unwrap();

        (contacts, folders, groups)
    }

    #[test]
    fn test_authorize_push_success() {
        let tmp = TempDir::new().unwrap();
        let (contacts, folders, groups) = setup_stores(tmp.path());

        // Alice has READ_WRITE globally and via group grant on /projects
        let result = authorize_push(
            "did:nexus:alice", "/projects", None, &contacts, &folders, &groups);
        assert!(result.is_ok());
    }

    #[test]
    fn test_authorize_push_unknown_contact() {
        let tmp = TempDir::new().unwrap();
        let (contacts, folders, groups) = setup_stores(tmp.path());

        let result = authorize_push(
            "did:nexus:unknown", "/projects", None, &contacts, &folders, &groups);
        assert!(matches!(result, Err(AuthError::UnknownContact { .. })));
    }

    #[test]
    fn test_authorize_push_insufficient_permission() {
        let tmp = TempDir::new().unwrap();
        let (contacts, folders, groups) = setup_stores(tmp.path());

        // Bob only has READ on /readonly
        let result = authorize_push(
            "did:nexus:bob", "/readonly", None, &contacts, &folders, &groups);
        assert!(matches!(result, Err(AuthError::InsufficientPermission { .. })));
    }

    #[test]
    fn test_authorize_push_folder_not_found() {
        let tmp = TempDir::new().unwrap();
        let (contacts, folders, groups) = setup_stores(tmp.path());

        let result = authorize_push(
            "did:nexus:alice", "/nonexistent", None, &contacts, &folders, &groups);
        assert!(matches!(result, Err(AuthError::FolderNotFound { .. })));
    }

    #[test]
    fn test_authorize_pull_success() {
        let tmp = TempDir::new().unwrap();
        let (contacts, folders, groups) = setup_stores(tmp.path());

        // Bob has READ on /readonly
        let result = authorize_pull(
            "did:nexus:bob", "/readonly", None, &contacts, &folders, &groups);
        assert!(result.is_ok());
    }

    #[test]
    fn test_revocation() {
        let tmp = TempDir::new().unwrap();
        let (mut contacts, folders, groups) = setup_stores(tmp.path());

        // Alice can push
        assert!(authorize_push(
            "did:nexus:alice", "/projects", None, &contacts, &folders, &groups).is_ok());

        // Remove Alice
        contacts.remove("did:nexus:alice").unwrap();

        // Now she can't
        assert!(matches!(
            authorize_push("did:nexus:alice", "/projects", None, &contacts, &folders, &groups),
            Err(AuthError::UnknownContact { .. })
        ));
    }

    #[test]
    fn test_group_based_access() {
        let tmp = TempDir::new().unwrap();
        let (contacts, folders, groups) = setup_stores(tmp.path());

        // Alice is in "team" group which has WRITE on /projects
        let result = authorize_push(
            "did:nexus:alice", "/projects", None, &contacts, &folders, &groups);
        assert!(result.is_ok());
    }
}
