//! Access control module — permissions, contacts, groups, folders, authorization
//!
//! Implements the access control layer from the NEXUS architecture:
//! - Bitmask permissions (READ, WRITE, MODIFY)
//! - Contact management with per-contact access levels
//! - Groups (logical buckets of DIDs for bulk grants)
//! - Vault folders with hierarchical permission resolution
//! - Push authorization checks

pub mod permission;
pub mod contact;
pub mod group;
pub mod folder;
pub mod auth;

pub use permission::Permission;
pub use contact::{Contact, ContactStore};
pub use group::{Group, GroupStore};
pub use folder::{VaultFolder, AccessGrant, GrantTarget, Grantee, FolderStore};
pub use auth::{authorize_push, AuthError};
