//! Manifest types — on-disk formats for encrypted files and share grants
//!
//! These are shared between CLI, Tauri GUI, and any other frontends.

use serde::{Deserialize, Serialize};
use crate::crypto::pre::{PrePublicKey, EncryptedDek, SerializedCfrag, VerifyingKey};
use crate::storage::shard::ShardManifest;

/// On-disk manifest for an encrypted file
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NexusManifest {
    /// Owner's DID
    pub owner: String,
    /// Owner's PRE public key (for re-encryption)
    pub owner_pre_pk: PrePublicKey,
    /// Shard manifest (CIDs, sizes, etc.)
    pub shards: ShardManifest,
    /// Umbral-encrypted DEK (capsule + ciphertext)
    pub encrypted_dek: EncryptedDek,
}

/// On-disk share grant — gives a recipient access via PRE
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShareGrant {
    /// Recipient's DID
    pub recipient: String,
    /// Recipient's PRE public key
    pub recipient_pre_pk: PrePublicKey,
    /// Re-encrypted capsule fragments
    pub cfrags: Vec<SerializedCfrag>,
    /// Verifying key for cfrag verification
    pub verifying_key: VerifyingKey,
    /// Reference to the original manifest
    pub manifest_ref: String,
}
