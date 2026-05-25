use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use rand::Rng;
use tauri::State;

use nexus_core::identity::{IdentityKeypair, IdentityVault, Did};
use nexus_core::crypto::pre::{PreKeypair, PreSigner, PrePublicKey, reencrypt};
use nexus_core::crypto::{encrypt_data, decrypt_data, generate_dek};
use nexus_core::manifest::{NexusManifest, ShareGrant};
use nexus_core::storage::{shard_data, reassemble, ShardStore, AssetStore, DEFAULT_SHARD_SIZE};
use nexus_core::storage::shard::Shard;
use nexus_core::storage::{ReceivedFiles, ReceivedFile};
use nexus_core::network::NodeConfig;

use crate::node_state::{NodeState, NodeInfo};
use crate::relay_state::{RelayState, RelayInfo};

// --- Response types ---

#[derive(Serialize)]
pub struct IdentityInfo {
    pub did: String,
    pub pre_public_key_hex: String,
    pub peer_id: String,
}

#[derive(Serialize)]
pub struct EncryptResult {
    pub filename: String,
    pub shard_count: usize,
    pub manifest_path: String,
}

#[derive(Serialize)]
pub struct StoreStatsResult {
    pub shard_count: usize,
    pub total_bytes: u64,
}

#[derive(Serialize)]
pub struct ShareResult {
    pub grant_path: String,
    pub recipient: String,
    pub cfrags_count: usize,
}

#[derive(Serialize)]
pub struct FileEntry {
    pub filename: String,
    pub manifest_path: String,
    pub owner: String,
    pub shard_count: usize,
    pub total_size: u64,
}

// --- Internal vault handling ---

#[derive(Serialize, Deserialize)]
struct VaultFile {
    identity_vault: IdentityVault,
    pre_seed: String,
    pre_public_key: nexus_core::crypto::pre::PrePublicKey,
}

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

fn hex_decode(s: &str) -> Result<Vec<u8>, String> {
    if s.len() % 2 != 0 { return Err("Invalid hex".into()); }
    (0..s.len()).step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i+2], 16).map_err(|_| "Invalid hex".into()))
        .collect()
}

fn load_keys(vault_path: &str, passphrase: &str) -> Result<(IdentityKeypair, PreKeypair), String> {
    let json = fs::read_to_string(vault_path)
        .map_err(|e| format!("Failed to read vault: {}", e))?;
    let vault_file: VaultFile = serde_json::from_str(&json)
        .map_err(|e| format!("Invalid vault: {}", e))?;
    let keypair = vault_file.identity_vault.unseal(passphrase)
        .map_err(|e| format!("Wrong passphrase: {}", e))?;
    let pre_seed = hex_decode(&vault_file.pre_seed)?;
    let pre_kp = PreKeypair::from_secret_bytes(&pre_seed)
        .map_err(|e| format!("PRE key error: {}", e))?;
    Ok((keypair, pre_kp))
}

// --- Contacts ---

#[derive(Serialize, Deserialize, Clone)]
pub struct Contact {
    pub name: String,
    pub did: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pre_public_key_hex: Option<String>,
    /// Encrypted PRE seed (hex) — generated on invite, claimable by recipient
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pre_seed_encrypted: Option<String>,
    /// Whether this contact was created via invite (keypair generated for them)
    #[serde(default, skip_serializing_if = "is_false")]
    pub invite_pending: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub peer_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub relay_addrs: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
}

fn is_false(b: &bool) -> bool { !b }

#[derive(Serialize, Deserialize, Default)]
struct ContactsFile {
    contacts: Vec<Contact>,
}

const CONTACTS_PATH: &str = ".nexus-contacts.json";

fn load_contacts() -> ContactsFile {
    fs::read_to_string(CONTACTS_PATH)
        .ok()
        .and_then(|json| serde_json::from_str(&json).ok())
        .unwrap_or_default()
}

fn save_contacts(file: &ContactsFile) -> Result<(), String> {
    let json = serde_json::to_string_pretty(file)
        .map_err(|e| format!("Serialization failed: {}", e))?;
    fs::write(CONTACTS_PATH, json)
        .map_err(|e| format!("Failed to write contacts: {}", e))?;
    Ok(())
}

/// Sync a UI contact to the ACL contact store (for push authorization).
/// Creates or updates the entry. Best-effort — won't fail if ACL store has issues.
fn sync_contact_to_acl(did: &str, label: &str, peer_id: Option<&str>, pre_pk_hex: Option<&str>) {
    use nexus_core::access::contact::ContactStore;
    use nexus_core::access::permission::Permission;

    // Ensure root folder exists for push auth
    ensure_root_folder();

    let Ok(mut store) = ContactStore::open(STORE_DIR) else { return };

    let pre_pk = pre_pk_hex
        .and_then(|h| hex_decode(h).ok())
        .unwrap_or_default();

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    // Check if contact already exists
    if let Some(existing) = store.get_mut(did) {
        // Update peer_id and pre_pk if needed
        if let Some(pid) = peer_id {
            existing.peer_id = Some(pid.to_string());
        }
        if !pre_pk.is_empty() {
            existing.pre_pk = pre_pk.clone();
        }
        existing.updated_at = now;
        let _ = store.save();
    } else {
        let contact = nexus_core::access::contact::Contact {
            did: did.to_string(),
            label: label.to_string(),
            peer_id: peer_id.map(|s| s.to_string()),
            pre_pk,
            access: Permission::WRITE,  // Default: allow push
            groups: vec![],
            created_at: now,
            updated_at: now,
        };
        let _ = store.add(contact);
    }
}

/// Ensure a default root folder exists in the ACL folder store (for push auth).
fn ensure_root_folder() {
    use nexus_core::access::folder::{FolderStore, VaultFolder};
    use nexus_core::access::permission::Permission;

    let Ok(mut store) = FolderStore::open(STORE_DIR) else { return };
    if store.get("/").is_none() {
        let folder = VaultFolder {
            path: "/".to_string(),
            label: Some("Root".to_string()),
            default_access: Permission::WRITE,
            grants: vec![],
            inherit: true,
        };
        let _ = store.create_folder(folder);
    }
}

/// Sync ALL legacy contacts to the ACL store (called on node start).
fn sync_all_contacts_to_acl() {
    let file = load_contacts();
    for contact in &file.contacts {
        sync_contact_to_acl(
            &contact.did,
            &contact.name,
            contact.peer_id.as_deref(),
            contact.pre_public_key_hex.as_deref(),
        );
    }
}

// --- Tauri Commands ---

#[tauri::command]
pub fn create_identity(vault_path: &str, passphrase: &str) -> Result<IdentityInfo, String> {
    if Path::new(vault_path).exists() {
        return Err("Vault already exists".into());
    }

    let keypair = IdentityKeypair::generate();
    let did = Did::from_public_identity(&keypair.public_identity());
    let vault = IdentityVault::seal(&keypair, passphrase)
        .map_err(|e| format!("Vault creation failed: {}", e))?;

    let pre_seed: [u8; 32] = rand::thread_rng().r#gen();
    let pre_kp = PreKeypair::from_secret_bytes(&pre_seed)
        .map_err(|e| format!("PRE key generation failed: {}", e))?;

    let vault_file = VaultFile {
        identity_vault: vault,
        pre_seed: hex_encode(&pre_seed),
        pre_public_key: pre_kp.public_key(),
    };

    let json = serde_json::to_string_pretty(&vault_file)
        .map_err(|e| format!("Serialization failed: {}", e))?;
    fs::write(vault_path, json)
        .map_err(|e| format!("Failed to write vault: {}", e))?;

    Ok(IdentityInfo {
        did: did.0,
        pre_public_key_hex: hex_encode(&pre_kp.public_key().bytes),
        peer_id: keypair.peer_id().to_string(),
    })
}

#[tauri::command]
pub fn get_identity(vault_path: &str, passphrase: &str) -> Result<IdentityInfo, String> {
    let (keypair, pre_kp) = load_keys(vault_path, passphrase)?;
    let did = Did::from_public_identity(&keypair.public_identity());
    Ok(IdentityInfo {
        did: did.0,
        pre_public_key_hex: hex_encode(&pre_kp.public_key().bytes),
        peer_id: keypair.peer_id().to_string(),
    })
}

#[tauri::command]
pub async fn encrypt_file(file_path: &str, vault_path: &str, passphrase: &str, app_handle: tauri::AppHandle) -> Result<EncryptResult, String> {
    use tauri::Emitter;

    let (_keypair, pre_kp) = load_keys(vault_path, passphrase)?;
    let (keypair, _) = load_keys(vault_path, passphrase)?;
    let did = Did::from_public_identity(&keypair.public_identity());

    let plaintext = fs::read(file_path)
        .map_err(|e| format!("Failed to read file: {}", e))?;

    let filename = Path::new(file_path)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "unnamed".into());

    let dek = generate_dek();
    let encrypted_body = encrypt_data(&plaintext, &dek)
        .map_err(|e| format!("Encryption failed: {}", e))?;

    let (mut manifest_data, shards) = shard_data(&encrypted_body, DEFAULT_SHARD_SIZE);
    manifest_data.filename = Some(filename.clone());

    // Write shards to output directory
    let shards_dir = Path::new(".").join("shards");
    fs::create_dir_all(&shards_dir).ok();
    for s in &shards {
        let shard_path = shards_dir.join(hex_encode(&s.cid));
        fs::write(&shard_path, &s.data).ok();
    }

    // Store in local shard store
    if let Ok(store) = ShardStore::open(".nexus-store") {
        let total = shards.len();
        for (i, s) in shards.iter().enumerate() {
            let _ = store.put(s);
            let _ = app_handle.emit("nexus://encrypt-progress", serde_json::json!({
                "current": i + 1,
                "total": total
            }));
        }
    }

    let encrypted_dek = pre_kp.encrypt_dek(&dek)
        .map_err(|e| format!("DEK encryption failed: {}", e))?;

    let manifest = NexusManifest {
        owner: did.0,
        owner_pre_pk: pre_kp.public_key(),
        shards: manifest_data,
        encrypted_dek,
    };

    let manifest_path = format!("{}.nexus", filename);
    let manifest_json = serde_json::to_string_pretty(&manifest)
        .map_err(|e| format!("Serialization failed: {}", e))?;
    fs::write(&manifest_path, manifest_json)
        .map_err(|e| format!("Failed to write manifest: {}", e))?;

    Ok(EncryptResult {
        filename,
        shard_count: shards.len(),
        manifest_path,
    })
}

#[tauri::command]
pub async fn decrypt_file(manifest_path: &str, output_path: Option<&str>, vault_path: &str, passphrase: &str, app_handle: tauri::AppHandle) -> Result<String, String> {
    use tauri::Emitter;

    let (_keypair, pre_kp) = load_keys(vault_path, passphrase)?;

    let manifest_json = fs::read_to_string(manifest_path)
        .map_err(|e| format!("Failed to read manifest: {}", e))?;
    let manifest: NexusManifest = serde_json::from_str(&manifest_json)
        .map_err(|e| format!("Invalid manifest: {}", e))?;

    let dek = pre_kp.decrypt_dek(&manifest.encrypted_dek)
        .map_err(|_| "Failed to decrypt — wrong key or not the owner".to_string())?;

    let manifest_dir = Path::new(manifest_path).parent().unwrap_or(Path::new("."));
    let shards_dir = manifest_dir.join("shards");

    let total_shards = manifest.shards.shards.len();
    let mut shards = Vec::new();
    for (i, cid_hex) in manifest.shards.shards.iter().enumerate() {
        // Try local store first, then shards directory
        let data = ShardStore::open(".nexus-store").ok()
            .and_then(|s| s.get(cid_hex).ok().flatten().map(|s| s.data))
            .or_else(|| fs::read(shards_dir.join(cid_hex)).ok())
            .ok_or_else(|| format!("Shard not found: {}", cid_hex))?;
        shards.push(Shard { cid: hex_decode(cid_hex)?, data });
        let _ = app_handle.emit("nexus://decrypt-progress", serde_json::json!({
            "current": i + 1,
            "total": total_shards
        }));
    }

    let encrypted_body = reassemble(&manifest.shards, &shards)
        .ok_or("Reassembly failed")?;

    let plaintext = decrypt_data(&encrypted_body, &dek)
        .map_err(|_| "Decryption failed".to_string())?;

    let out = output_path
        .map(|s| s.to_string())
        .or(manifest.shards.filename.clone())
        .unwrap_or_else(|| "decrypted_output".into());

    fs::write(&out, &plaintext)
        .map_err(|e| format!("Failed to write: {}", e))?;

    Ok(out)
}
#[tauri::command]
pub fn get_store_stats() -> Result<StoreStatsResult, String> {
    let store = ShardStore::open(".nexus-store")?;
    let stats = store.stats()?;
    Ok(StoreStatsResult {
        shard_count: stats.shard_count,
        total_bytes: stats.total_bytes,
    })
}

#[derive(Serialize)]
pub struct ShardInfo {
    pub cid: String,
    pub size: u64,
}

#[tauri::command]
pub fn list_shards() -> Result<Vec<ShardInfo>, String> {
    let store = ShardStore::open(".nexus-store")?;
    let cids = store.list()?;
    let mut shards = Vec::new();
    for cid in cids {
        let size = store.get(&cid)?
            .map(|s| s.data.len() as u64)
            .unwrap_or(0);
        shards.push(ShardInfo { cid, size });
    }
    Ok(shards)
}

#[derive(Serialize)]
pub struct VerifyResult {
    pub total: usize,
    pub valid: usize,
    pub corrupted: Vec<String>,
}

#[tauri::command]
pub fn verify_store() -> Result<VerifyResult, String> {
    use nexus_core::storage::compute_cid;

    let store = ShardStore::open(".nexus-store")?;
    let cids = store.list()?;
    let total = cids.len();
    let mut valid = 0;
    let mut corrupted = Vec::new();

    for cid_hex in &cids {
        match store.get(cid_hex)? {
            Some(shard) => {
                let computed_bytes = compute_cid(&shard.data);
                let computed: String = computed_bytes.iter().map(|b| format!("{:02x}", b)).collect();
                if computed == *cid_hex {
                    valid += 1;
                } else {
                    corrupted.push(cid_hex.clone());
                }
            }
            None => corrupted.push(cid_hex.clone()),
        }
    }

    Ok(VerifyResult { total, valid, corrupted })
}

#[tauri::command]
pub fn list_files() -> Result<Vec<FileEntry>, String> {
    let mut files = Vec::new();

    // Scan current directory for .nexus manifests
    let entries = fs::read_dir(".")
        .map_err(|e| format!("Failed to read directory: {}", e))?;

    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some("nexus") {
            if let Ok(json) = fs::read_to_string(&path) {
                if let Ok(manifest) = serde_json::from_str::<NexusManifest>(&json) {
                    files.push(FileEntry {
                        filename: manifest.shards.filename.unwrap_or_else(|| "unnamed".into()),
                        manifest_path: path.to_string_lossy().to_string(),
                        owner: manifest.owner,
                        shard_count: manifest.shards.shards.len(),
                        total_size: manifest.shards.total_size,
                    });
                }
            }
        }
    }

    Ok(files)
}

#[tauri::command]
pub fn add_contact(name: &str, did: &str, pre_public_key_hex: Option<&str>, peer_id: Option<&str>, relay_addrs: Option<Vec<String>>, notes: Option<&str>, vault_path: Option<&str>, passphrase: Option<&str>) -> Result<Contact, String> {
    let mut file = load_contacts();

    // Check for duplicate DID
    if file.contacts.iter().any(|c| c.did == did) {
        return Err("Contact with this DID already exists".into());
    }

    // If no PRE public key provided, generate an invite keypair
    let (pre_pk_hex, pre_seed_enc, invite) = if let Some(pk) = pre_public_key_hex {
        (Some(pk.to_string()), None, false)
    } else if let (Some(vp), Some(pass), Some(pid)) = (vault_path, passphrase, peer_id) {
        // Deterministic derivation: derive PRE key from vault seed + peer_id
        let (_id_kp, pre_kp) = load_keys(vp, pass)?;
        let vault_seed = pre_kp.to_secret_bytes();
        let derived_kp = PreKeypair::derive_for_peer(&vault_seed, pid);
        let pk_hex = hex_encode(&derived_kp.public_key().bytes);
        let seed_hex = hex_encode(&derived_kp.to_secret_bytes());
        (Some(pk_hex), Some(seed_hex), true)
    } else {
        // Random generation fallback (no vault context)
        let invite_kp = PreKeypair::generate();
        let pk_hex = hex_encode(&invite_kp.public_key().bytes);
        let seed_hex = hex_encode(&invite_kp.to_secret_bytes());
        (Some(pk_hex), Some(seed_hex), true)
    };

    let contact = Contact {
        name: name.to_string(),
        did: did.to_string(),
        pre_public_key_hex: pre_pk_hex,
        pre_seed_encrypted: pre_seed_enc,
        invite_pending: invite,
        peer_id: peer_id.map(|s| s.to_string()),
        relay_addrs,
        notes: notes.map(|s| s.to_string()),
    };

    file.contacts.push(contact.clone());
    save_contacts(&file)?;

    // Sync to ACL contact store so push auth recognizes this contact
    sync_contact_to_acl(did, name, peer_id, pre_public_key_hex);

    Ok(contact)
}

#[tauri::command]
pub fn list_contacts() -> Result<Vec<Contact>, String> {
    Ok(load_contacts().contacts)
}

#[tauri::command]
pub fn remove_contact(did: &str) -> Result<(), String> {
    let mut file = load_contacts();
    let before = file.contacts.len();
    file.contacts.retain(|c| c.did != did);
    if file.contacts.len() == before {
        return Err("Contact not found".into());
    }
    save_contacts(&file)?;
    Ok(())
}

#[tauri::command]
pub fn update_contact(did: &str, name: Option<&str>, pre_public_key_hex: Option<&str>, peer_id: Option<&str>, relay_addrs: Option<Vec<String>>, notes: Option<&str>) -> Result<Contact, String> {
    let mut file = load_contacts();
    let contact = file.contacts.iter_mut()
        .find(|c| c.did == did)
        .ok_or("Contact not found")?;

    if let Some(n) = name { contact.name = n.to_string(); }
    if let Some(pk) = pre_public_key_hex { contact.pre_public_key_hex = Some(pk.to_string()); }
    if let Some(p) = peer_id { contact.peer_id = Some(p.to_string()); }
    if let Some(addrs) = relay_addrs { contact.relay_addrs = Some(addrs); }
    if let Some(n) = notes { contact.notes = Some(n.to_string()); }

    let updated = contact.clone();
    save_contacts(&file)?;

    // Sync updated info to ACL store
    sync_contact_to_acl(did, updated.name.as_str(), updated.peer_id.as_deref(), updated.pre_public_key_hex.as_deref());

    Ok(updated)
}

/// Export the invite PRE seed for a contact (so recipient can claim their key)
#[tauri::command]
pub fn get_invite_key(did: &str) -> Result<String, String> {
    let file = load_contacts();
    let contact = file.contacts.iter()
        .find(|c| c.did == did)
        .ok_or("Contact not found")?;

    if !contact.invite_pending {
        return Err("Contact is not an invite (they provided their own key)".into());
    }

    contact.pre_seed_encrypted.clone()
        .ok_or("No invite seed stored for this contact".into())
}

// --- Join Request / Response ---

/// A join request: what you send to someone to ask them to add you
#[derive(Serialize, Deserialize, Clone)]
pub struct JoinRequest {
    pub name: String,
    pub peer_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pre_public_key_hex: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub relay_addrs: Option<Vec<String>>,
}

/// A join response: what you send back after accepting a join request
#[derive(Serialize, Deserialize, Clone)]
pub struct JoinResponse {
    pub name: String,
    pub peer_id: String,
    pub pre_public_key_hex: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub relay_addrs: Option<Vec<String>>,
}

/// Generate my join request (to send to someone else)
#[tauri::command]
pub fn create_join_request(vault_path: &str, passphrase: &str, name: &str, include_pre: bool) -> Result<String, String> {
    let (keypair, pre_kp) = load_keys(vault_path, passphrase)?;

    let request = JoinRequest {
        name: name.to_string(),
        peer_id: keypair.peer_id().to_string(),
        pre_public_key_hex: if include_pre {
            Some(hex_encode(&pre_kp.public_key().bytes))
        } else {
            None
        },
        relay_addrs: None, // TODO: pull from config if available
    };

    serde_json::to_string(&request)
        .map_err(|e| format!("Serialization failed: {}", e))
}

/// Accept an incoming join request — adds them as contact, returns a join response
#[tauri::command]
pub fn accept_join_request(vault_path: &str, passphrase: &str, my_name: &str, request_json: &str) -> Result<String, String> {
    let request: JoinRequest = serde_json::from_str(request_json)
        .map_err(|e| format!("Invalid join request: {}", e))?;

    let (keypair, pre_kp) = load_keys(vault_path, passphrase)?;
    let _my_did = Did::from_public_identity(&keypair.public_identity());

    // Deterministically derive a PRE keypair for the requester from our vault seed + their peer_id
    let vault_seed = pre_kp.to_secret_bytes();
    let their_kp = PreKeypair::derive_for_peer(&vault_seed, &request.peer_id);
    let their_pre_pk_hex = hex_encode(&their_kp.public_key().bytes);
    let their_seed_hex = hex_encode(&their_kp.to_secret_bytes());

    // Determine their DID placeholder
    let their_did = format!("did:nexus:peer-{}", &request.peer_id[..16.min(request.peer_id.len())]);

    // Add them as a contact
    let mut file = load_contacts();
    if !file.contacts.iter().any(|c| c.peer_id.as_deref() == Some(&request.peer_id)) {
        let contact = Contact {
            name: request.name.clone(),
            did: their_did.clone(),
            pre_public_key_hex: request.pre_public_key_hex.clone(),
            pre_seed_encrypted: if request.pre_public_key_hex.is_none() {
                None // They didn't provide a key, we can't share with them yet
            } else {
                None
            },
            invite_pending: request.pre_public_key_hex.is_none(),
            peer_id: Some(request.peer_id.clone()),
            relay_addrs: request.relay_addrs.clone(),
            notes: Some("Added via join request".to_string()),
        };
        file.contacts.push(contact);
        save_contacts(&file)?;

        // Sync to ACL store
        let acl_pre = request.pre_public_key_hex.as_deref();
        sync_contact_to_acl(&their_did, &request.name, Some(&request.peer_id), acl_pre);
    }

    // Build join response with the PRE we generated for them
    let response = JoinResponse {
        name: my_name.to_string(),
        peer_id: keypair.peer_id().to_string(),
        pre_public_key_hex: their_pre_pk_hex,
        relay_addrs: None, // TODO: pull from config
    };

    // Also return the seed so UI can show/store it
    // We embed it in a wrapper for the frontend
    let result = serde_json::json!({
        "response": response,
        "invite_seed_hex": their_seed_hex,
    });

    serde_json::to_string(&result)
        .map_err(|e| format!("Serialization failed: {}", e))
}

/// Apply a join response — updates the contact that sent us a response
#[tauri::command]
pub fn apply_join_response(response_json: &str) -> Result<String, String> {
    let response: JoinResponse = serde_json::from_str(response_json)
        .map_err(|e| format!("Invalid join response: {}", e))?;

    let mut file = load_contacts();

    // Find contact by peer_id or add new
    let contact = file.contacts.iter_mut()
        .find(|c| c.peer_id.as_deref() == Some(&response.peer_id));

    if let Some(contact) = contact {
        // Update existing contact with their PRE key
        contact.pre_public_key_hex = Some(response.pre_public_key_hex.clone());
        contact.name = response.name.clone();
        if let Some(addrs) = &response.relay_addrs {
            contact.relay_addrs = Some(addrs.clone());
        }
    } else {
        // New contact from response
        let new_contact = Contact {
            name: response.name.clone(),
            did: format!("did:nexus:peer-{}", &response.peer_id[..16.min(response.peer_id.len())]),
            pre_public_key_hex: Some(response.pre_public_key_hex.clone()),
            pre_seed_encrypted: None,
            invite_pending: false,
            peer_id: Some(response.peer_id.clone()),
            relay_addrs: response.relay_addrs.clone(),
            notes: Some("Added via join response".to_string()),
        };
        file.contacts.push(new_contact);
    }

    save_contacts(&file)?;

    // Sync to ACL store
    sync_contact_to_acl(
        &format!("did:nexus:peer-{}", &response.peer_id[..16.min(response.peer_id.len())]),
        &response.name,
        Some(&response.peer_id),
        Some(&response.pre_public_key_hex),
    );

    Ok(format!("Contact '{}' updated with PRE key", response.name))
}

#[tauri::command]
pub fn share_file(
    manifest_path: &str,
    recipient_did: &str,
    recipient_pre_pk_hex: &str,
    vault_path: &str,
    passphrase: &str,
) -> Result<ShareResult, String> {
    let (_keypair, pre_kp) = load_keys(vault_path, passphrase)?;

    // Load manifest
    let manifest_bytes = fs::read(manifest_path)
        .map_err(|e| format!("Failed to read manifest: {}", e))?;
    let manifest: NexusManifest = serde_json::from_slice(&manifest_bytes)
        .map_err(|e| format!("Invalid manifest: {}", e))?;

    // Compute asset ID from manifest content
    let asset_id = AssetStore::compute_asset_id(&manifest_bytes);

    // Parse recipient's PRE public key
    let recipient_pk_bytes = hex_decode(recipient_pre_pk_hex)?;
    let recipient_pk = PrePublicKey { bytes: recipient_pk_bytes };

    // Generate PRE signer + kfrags (threshold=1, shares=1 for direct share)
    let signer = PreSigner::new();
    let vk = signer.verifying_key();

    let kfrags = signer
        .generate_kfrags(&pre_kp, &recipient_pk, 1, 1)
        .map_err(|e| format!("kfrag generation failed: {}", e))?;

    // Re-encrypt (act as own proxy)
    let cfrags: Result<Vec<_>, _> = kfrags
        .iter()
        .map(|kf| reencrypt(&manifest.encrypted_dek, kf, &pre_kp.public_key(), &recipient_pk, &vk))
        .collect();
    let cfrags = cfrags.map_err(|e| format!("Re-encryption failed: {}", e))?;

    // Build share grant (rfrag)
    let grant = ShareGrant {
        recipient: recipient_did.to_string(),
        recipient_pre_pk: recipient_pk,
        cfrags,
        verifying_key: vk,
        manifest_ref: manifest_path.to_string(),
    };

    // Serialize the grant as the rfrag
    let rfrag_bytes = serde_json::to_vec(&grant)
        .map_err(|e| format!("Serialization failed: {}", e))?;

    // Store rfrag in the asset store
    let store = AssetStore::open(".nexus-store")
        .map_err(|e| format!("Failed to open asset store: {}", e))?;

    // Also store the manifest in the asset store (idempotent)
    store.put_manifest(&manifest_bytes)?;

    // Write rfrag
    store.put_rfrag(&asset_id, recipient_did, &rfrag_bytes)?;

    let grant_path = format!(".nexus-store/rfrags/{}/{}", asset_id, recipient_did.replace(':', "_"));

    Ok(ShareResult {
        grant_path,
        recipient: recipient_did.to_string(),
        cfrags_count: 1,
    })
}

// --- Share management (pull-only model) ---

#[derive(Serialize)]
pub struct ShareInfo {
    pub asset_id: String,
    pub share_link: String,
    pub shared_with: Vec<SharedUserInfo>,
    pub public: bool,
}

#[derive(Serialize)]
pub struct SharedUserInfo {
    pub did: String,
    pub name: Option<String>,
}

#[tauri::command]
pub fn get_share_info(manifest_path: &str, peer_id: &str) -> Result<ShareInfo, String> {
    let manifest_bytes = fs::read(manifest_path)
        .map_err(|e| format!("Failed to read manifest: {}", e))?;
    let asset_id = AssetStore::compute_asset_id(&manifest_bytes);
    let store = AssetStore::open(".nexus-store")
        .map_err(|e| format!("Failed to open store: {}", e))?;

    let shared_dids = store.list_shared_users(&asset_id).unwrap_or_default();

    // Resolve names from contacts
    let contacts_file = load_contacts();
    let shared_with: Vec<SharedUserInfo> = shared_dids.iter().map(|did| {
        let name = contacts_file.contacts.iter()
            .find(|c| c.did == *did)
            .map(|c| c.name.clone());
        SharedUserInfo { did: did.clone(), name }
    }).collect();

    let share_link = AssetStore::share_link(peer_id, &asset_id);

    let public = store.is_public(&asset_id).unwrap_or(false);

    Ok(ShareInfo {
        asset_id,
        share_link,
        shared_with,
        public,
    })
}

#[tauri::command]
pub fn revoke_share(manifest_path: &str, recipient_did: &str) -> Result<bool, String> {
    let manifest_bytes = fs::read(manifest_path)
        .map_err(|e| format!("Failed to read manifest: {}", e))?;
    let asset_id = AssetStore::compute_asset_id(&manifest_bytes);
    let store = AssetStore::open(".nexus-store")
        .map_err(|e| format!("Failed to open store: {}", e))?;
    store.remove_rfrag(&asset_id, recipient_did)
}

#[tauri::command]
pub fn set_share_public(manifest_path: &str, public: bool, vault_path: &str, passphrase: &str) -> Result<bool, String> {
    let manifest_bytes = fs::read(manifest_path)
        .map_err(|e| format!("Failed to read manifest: {}", e))?;
    let asset_id = AssetStore::compute_asset_id(&manifest_bytes);
    let store = AssetStore::open(".nexus-store")
        .map_err(|e| format!("Failed to open store: {}", e))?;

    use nexus_core::crypto::pre::{public_pre_keypair, PUBLIC_DID};

    if public {
        // Generate PRE rfrag for the well-known public identity
        let (_identity_kp, pre_kp) = load_keys(vault_path, passphrase)?;
        let manifest: NexusManifest = serde_json::from_slice(&manifest_bytes)
            .map_err(|e| format!("Invalid manifest: {}", e))?;

        let public_kp = public_pre_keypair();
        let public_pk = public_kp.public_key();

        // Generate kfrags: owner → public keypair
        let signer = PreSigner::new();
        let vk = signer.verifying_key();
        let kfrags = signer
            .generate_kfrags(&pre_kp, &public_pk, 1, 1)
            .map_err(|e| format!("kfrag generation failed: {}", e))?;

        // Re-encrypt to produce cfrags
        let cfrags: Result<Vec<_>, _> = kfrags
            .iter()
            .map(|kf| reencrypt(&manifest.encrypted_dek, kf, &pre_kp.public_key(), &public_pk, &vk))
            .collect();
        let cfrags = cfrags.map_err(|e| format!("Re-encryption failed: {}", e))?;

        // Build share grant (same format as private shares)
        let grant = ShareGrant {
            recipient: PUBLIC_DID.to_string(),
            recipient_pre_pk: public_pk,
            cfrags,
            verifying_key: vk,
            manifest_ref: manifest_path.to_string(),
        };

        let rfrag_bytes = serde_json::to_vec(&grant)
            .map_err(|e| format!("Serialization failed: {}", e))?;

        // Store as rfrag for did:nexus:public
        store.put_rfrag(&asset_id, PUBLIC_DID, &rfrag_bytes)?;
    } else {
        // Revoke public: remove the public rfrag
        use nexus_core::crypto::pre::PUBLIC_DID;
        let _ = store.remove_rfrag(&asset_id, PUBLIC_DID);
    }

    store.set_public(&asset_id, public)
}

#[derive(Serialize)]
pub struct PullResult {
    pub filename: String,
    pub size: usize,
    pub path: String,
}

#[tauri::command]
pub async fn pull_shared_file(
    link: &str,
    vault_path: &str,
    passphrase: &str,
    output_dir: Option<&str>,
    add_to_my_files: bool,
    state: State<'_, crate::node_state::NodeState>,
    _app_handle: tauri::AppHandle,
) -> Result<PullResult, String> {
    use nexus_core::storage::AssetStore;
    use nexus_core::manifest::NexusManifest;
    use nexus_core::crypto::{decrypt_data, encrypt_data, generate_dek};
    use nexus_core::storage::shard;
    use nexus_core::network::NodeCommand;
    use nexus_core::network::protocol::NexusResponse;

    // Parse link
    let (target_peer_str, asset_id) = AssetStore::parse_share_link(link)
        .ok_or_else(|| format!("Invalid share link: {}", link))?;

    let (keypair, pre_kp) = load_keys(vault_path, passphrase)?;
    let my_did = keypair.did();
    let signature = keypair.sign(asset_id.as_bytes());

    let target_peer: nexus_core::network::PeerId = target_peer_str.parse()
        .map_err(|e| format!("Invalid peer ID: {}", e))?;

    // Subscribe to pull responses before sending request
    let mut pull_rx = state.subscribe_pull_responses();

    // Get node command channel
    let cmd_tx = state.command_tx().await
        .ok_or("Node not running")?;

    // Try to dial target peer through configured relay circuits
    // (needed when there's no direct connection to the peer)
    let saved_config = get_config();
    let relay_addrs: Vec<String> = saved_config.relay_servers.iter().map(|e| e.addr.clone()).collect();
    eprintln!("[pull] Relay servers from config: {:?}", relay_addrs);
    eprintln!("[pull] Target peer: {}", target_peer);
    for relay_addr_str in &relay_addrs {
        // Build circuit address: <relay-addr>/p2p-circuit/p2p/<target-peer>
        let circuit_addr_str = format!("{}/p2p-circuit/p2p/{}", relay_addr_str, target_peer);
        eprintln!("[pull] Dialing circuit: {}", circuit_addr_str);
        if let Ok(circuit_addr) = circuit_addr_str.parse::<nexus_core::network::Multiaddr>() {
            let _ = cmd_tx.send(NodeCommand::Dial(circuit_addr)).await;
        } else {
            eprintln!("[pull] Failed to parse circuit addr: {}", circuit_addr_str);
        }
    }
    if relay_addrs.is_empty() {
        eprintln!("[pull] WARNING: No relay servers configured! Cannot dial through circuit.");
    }
    // Give the relay circuit time to establish
    tokio::time::sleep(std::time::Duration::from_secs(3)).await;

    // Send pull request
    cmd_tx.send(NodeCommand::PullAsset {
        peer: target_peer,
        asset_id: asset_id.clone(),
        requester_did: my_did,
        signature,
    }).await.map_err(|e| format!("Failed to send pull: {}", e))?;

    // Wait for response (timeout 60s)
    let response = tokio::time::timeout(std::time::Duration::from_secs(60), async {
        loop {
            match pull_rx.recv().await {
                Ok(pr) => return Ok(pr.response),
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(e) => return Err(format!("Channel error: {}", e)),
            }
        }
    }).await
    .map_err(|_| "Pull timed out (60s)".to_string())?
    .map_err(|e| e)?;

    match response {
        NexusResponse::Asset { asset_id: _, rfrag, manifest, shards } => {
            // Parse rfrag and manifest
            let grant: nexus_core::manifest::ShareGrant = serde_json::from_slice(&rfrag)
                .map_err(|e| format!("Invalid rfrag: {}", e))?;
            let nexus_manifest: NexusManifest = serde_json::from_slice(&manifest)
                .map_err(|e| format!("Invalid manifest: {}", e))?;

            let filename = nexus_manifest.shards.filename.clone()
                .unwrap_or_else(|| "unnamed".into());

            // Verify shard CIDs
            if shards.len() != nexus_manifest.shards.shards.len() {
                return Err(format!("Shard count mismatch: {} vs {}",
                    shards.len(), nexus_manifest.shards.shards.len()));
            }
            for (expected_cid, shard_data) in nexus_manifest.shards.shards.iter().zip(&shards) {
                let computed = nexus_core::storage::shard::compute_cid(shard_data);
                let computed_hex: String = computed.iter().map(|b| format!("{:02x}", b)).collect();
                if computed_hex != *expected_cid {
                    return Err(format!("CID mismatch: expected {}, got {}", expected_cid, computed_hex));
                }
            }

            // Decrypt DEK via PRE
            let decrypt_kp = if grant.recipient == nexus_core::crypto::pre::PUBLIC_DID {
                nexus_core::crypto::pre::public_pre_keypair()
            } else {
                pre_kp.clone()
            };
            let dek = decrypt_kp.decrypt_dek_reencrypted(
                &nexus_manifest.encrypted_dek,
                &grant.cfrags,
                &nexus_manifest.owner_pre_pk,
                &grant.verifying_key,
            ).map_err(|e| format!("PRE decryption failed: {}", e))?;

            // Reassemble shards
            let shard_objs: Vec<nexus_core::storage::shard::Shard> = nexus_manifest.shards.shards.iter()
                .zip(shards)
                .map(|(cid_hex, data)| {
                    let cid_bytes: Vec<u8> = (0..cid_hex.len())
                        .step_by(2)
                        .map(|i| u8::from_str_radix(&cid_hex[i..i+2], 16).unwrap())
                        .collect();
                    nexus_core::storage::shard::Shard { cid: cid_bytes, data }
                })
                .collect();

            let encrypted_body = shard::reassemble(&nexus_manifest.shards, &shard_objs)
                .ok_or("Failed to reassemble shards")?;

            let plaintext = decrypt_data(&encrypted_body, &dek)
                .map_err(|_| "Decryption failed".to_string())?;

            let size = plaintext.len();

            if add_to_my_files {
                // Re-encrypt with our own key and store as a local asset
                let new_dek = generate_dek();
                let new_encrypted = encrypt_data(&plaintext, &new_dek)
                    .map_err(|e| format!("Re-encryption failed: {}", e))?;
                let new_encrypted_dek = pre_kp.encrypt_dek(&new_dek)
                    .map_err(|e| format!("DEK encryption failed: {}", e))?;

                let (new_shard_manifest, new_shard_objs) = nexus_core::storage::shard_data(&new_encrypted, nexus_core::storage::DEFAULT_SHARD_SIZE);
                let shard_store = nexus_core::storage::ShardStore::open(".nexus-store")
                    .map_err(|e| format!("Failed to open shard store: {}", e))?;
                for s in &new_shard_objs {
                    shard_store.put(s).ok();
                }

                let new_manifest = NexusManifest {
                    owner: keypair.did(),
                    encrypted_dek: new_encrypted_dek,
                    owner_pre_pk: pre_kp.public_key(),
                    shards: new_shard_manifest,
                };

                let manifest_bytes = serde_json::to_vec_pretty(&new_manifest)
                    .map_err(|e| format!("Manifest serialization failed: {}", e))?;

                let asset_store = AssetStore::open(".nexus-store")
                    .map_err(|e| format!("Failed to open asset store: {}", e))?;
                asset_store.put_manifest(&manifest_bytes)?;

                // Also save manifest to manifests/ for the file list
                let manifest_dir = Path::new(".nexus-store").join("manifests");
                fs::create_dir_all(&manifest_dir).ok();
                let manifest_path = manifest_dir.join(format!("{}.json",
                    AssetStore::compute_asset_id(&manifest_bytes)));
                fs::write(&manifest_path, &manifest_bytes).ok();

                Ok(PullResult {
                    filename,
                    size,
                    path: manifest_path.to_string_lossy().into(),
                })
            } else {
                // Save to download folder
                let dir = output_dir
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| {
                        dirs::download_dir()
                            .unwrap_or_else(|| std::env::current_dir().unwrap_or_default())
                            .to_string_lossy().into()
                    });
                let out_path = Path::new(&dir).join(&filename);
                fs::write(&out_path, &plaintext)
                    .map_err(|e| format!("Failed to write file: {}", e))?;

                Ok(PullResult {
                    filename,
                    size,
                    path: out_path.to_string_lossy().into(),
                })
            }
        }
        NexusResponse::AssetDenied { asset_id, reason } => {
            Err(format!("Access denied for {}: {}", &asset_id[..16.min(asset_id.len())], reason))
        }
        other => {
            Err(format!("Unexpected response: {:?}", other))
        }
    }
}

const RECEIVED_FILES_PATH: &str = ".nexus-received.json";
const STORE_DIR: &str = ".nexus-store";

#[derive(Serialize)]
pub struct ReceivedFileInfo {
    pub id: String,
    pub sender_did: String,
    pub filename: String,
    pub has_share_grant: bool,
    pub received_at: u64,
    pub decrypted: bool,
    pub total_size: u64,
    pub shard_count: usize,
}

impl From<ReceivedFile> for ReceivedFileInfo {
    fn from(f: ReceivedFile) -> Self {
        // Try to read manifest for size info
        let (total_size, shard_count) = fs::read_to_string(&f.manifest_path)
            .ok()
            .and_then(|json| serde_json::from_str::<NexusManifest>(&json).ok())
            .map(|m| (m.shards.total_size, m.shards.shards.len()))
            .unwrap_or((0, 0));

        Self {
            id: f.id,
            sender_did: f.sender_did,
            filename: f.filename,
            has_share_grant: f.share_grant_json.is_some(),
            received_at: f.received_at,
            decrypted: f.decrypted,
            total_size,
            shard_count,
        }
    }
}

#[tauri::command]
pub fn list_received_files() -> Result<Vec<ReceivedFileInfo>, String> {
    let store = ReceivedFiles::open(RECEIVED_FILES_PATH);
    Ok(store.all().into_iter().map(|f| f.into()).collect())
}

#[tauri::command]
pub fn decrypt_received(
    received_id: &str,
    vault_path: &str,
    passphrase: &str,
    output_path: Option<&str>,
) -> Result<String, String> {
    let received_store = ReceivedFiles::open(RECEIVED_FILES_PATH);
    let all = received_store.all();
    let entry = all.iter().find(|f| f.id == received_id)
        .ok_or("Received file not found")?;

    let (_keypair, pre_kp) = load_keys(vault_path, passphrase)?;

    // Load manifest
    let manifest_json = fs::read_to_string(&entry.manifest_path)
        .map_err(|e| format!("Failed to read manifest: {}", e))?;
    let manifest: NexusManifest = serde_json::from_str(&manifest_json)
        .map_err(|e| format!("Invalid manifest: {}", e))?;

    // Determine if we need PRE decryption or direct
    let dek = if let Some(ref grant_json) = entry.share_grant_json {
        // PRE shared decryption
        let grant: ShareGrant = serde_json::from_str(grant_json)
            .map_err(|e| format!("Invalid share grant: {}", e))?;

        let decrypt_kp = if grant.recipient == nexus_core::crypto::pre::PUBLIC_DID {
            nexus_core::crypto::pre::public_pre_keypair()
        } else {
            pre_kp.clone()
        };
        decrypt_kp.decrypt_dek_reencrypted(&manifest.encrypted_dek, &grant.cfrags, &manifest.owner_pre_pk, &grant.verifying_key)
            .map_err(|e| format!("PRE decryption failed: {:?}", e))?
    } else {
        // Direct decryption (we're the owner)
        pre_kp.decrypt_dek(&manifest.encrypted_dek)
            .map_err(|_| "Failed to decrypt — wrong key or not the owner".to_string())?
    };

    // Load shards and reassemble
    let manifest_dir = Path::new(&entry.manifest_path).parent().unwrap_or(Path::new("."));
    let shards_dir = manifest_dir.join("shards");

    let mut shards = Vec::new();
    for cid_hex in &manifest.shards.shards {
        let data = ShardStore::open(".nexus-store").ok()
            .and_then(|s| s.get(cid_hex).ok().flatten().map(|s| s.data))
            .or_else(|| fs::read(shards_dir.join(cid_hex)).ok())
            .ok_or_else(|| format!("Shard not found: {}", cid_hex))?;
        shards.push(Shard { cid: hex_decode(cid_hex)?, data });
    }

    let encrypted_body = reassemble(&manifest.shards, &shards)
        .ok_or("Reassembly failed")?;

    let plaintext = decrypt_data(&encrypted_body, &dek)
        .map_err(|_| "Decryption failed".to_string())?;

    let filename = entry.filename.clone();
    let out = match output_path {
        Some(dir) => {
            let p = Path::new(dir).join(&filename);
            p.to_string_lossy().to_string()
        }
        None => filename,
    };

    fs::write(&out, &plaintext)
        .map_err(|e| format!("Failed to write to {}: {}", out, e))?;

    // Mark as decrypted
    let _ = received_store.mark_decrypted(received_id);

    Ok(out)
}

#[tauri::command]
pub fn remove_received(id: &str) -> Result<(), String> {
    let store = ReceivedFiles::open(RECEIVED_FILES_PATH);
    store.remove(id)
}

// --- Push send command ---

#[derive(Serialize, Clone)]
pub struct PushSendProgress {
    pub status: String,
    pub filename: String,
    pub shards_sent: usize,
    pub shards_total: usize,
    pub asset_id: Option<String>,
    pub error: Option<String>,
}

#[tauri::command]
pub async fn push_to_peer(
    file_path: &str,
    target_peer_id: &str,
    target_folder: &str,
    vault_path: &str,
    passphrase: &str,
    state: State<'_, crate::node_state::NodeState>,
    app_handle: tauri::AppHandle,
) -> Result<String, String> {
    use tauri::Emitter;
    use nexus_core::network::NodeCommand;
    use nexus_core::network::protocol::NexusResponse;
    use nexus_core::storage::compute_cid;

    eprintln!("[push] push_to_peer called: file={}, peer={}, folder={}", file_path, target_peer_id, target_folder);

    // Validate file
    if !Path::new(file_path).exists() {
        return Err(format!("File not found: {}", file_path));
    }

    let filename = Path::new(file_path)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "unnamed".into());

    let file_data = fs::read(file_path)
        .map_err(|e| format!("Failed to read file: {}", e))?;
    let total_size = file_data.len() as u64;

    // Load keys
    let (keypair, pre_kp) = load_keys(vault_path, passphrase)?;
    let my_did = keypair.did();

    // Encrypt and shard
    let dek = generate_dek();
    let encrypted_data = encrypt_data(&file_data, &dek)
        .map_err(|e| format!("Encryption failed: {:?}", e))?;
    let (mut shard_manifest, shards) = shard_data(&encrypted_data, DEFAULT_SHARD_SIZE);
    shard_manifest.filename = Some(filename.clone());
    let shard_count = shards.len();

    // Look up receiver's PRE public key from UI contacts (.nexus-contacts.json)
    let contacts_file = load_contacts();
    let receiver_contact = contacts_file.contacts.iter()
        .find(|c| c.peer_id.as_deref() == Some(target_peer_id))
        .ok_or_else(|| format!("No contact with peer_id: {}", target_peer_id))?;

    eprintln!("[push] Found contact: {} (relay_addrs: {:?})", receiver_contact.name, receiver_contact.relay_addrs);

    let pre_pk_hex = receiver_contact.pre_public_key_hex.as_deref()
        .ok_or_else(|| format!("Contact '{}' has no PRE public key", receiver_contact.name))?;
    let pre_pk_bytes = hex_decode(pre_pk_hex)?;
    let receiver_pre_pk = PrePublicKey { bytes: pre_pk_bytes };

    // Encrypt DEK for receiver
    let encrypted_dek = PreKeypair::encrypt_dek_for(&receiver_pre_pk, &dek)
        .map_err(|e| format!("DEK encryption failed: {:?}", e))?;

    // Build manifest
    let manifest = NexusManifest {
        owner: my_did.clone(),
        owner_pre_pk: pre_kp.public_key(),
        shards: shard_manifest,
        encrypted_dek,
    };
    let manifest_bytes = serde_json::to_vec(&manifest)
        .map_err(|e| format!("Manifest serialization failed: {}", e))?;
    let manifest_hash = hex_encode(&compute_cid(&manifest_bytes));

    // Build auth signature
    let nonce: Vec<u8> = (0..16).map(|_| rand::random::<u8>()).collect();
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    let mut sign_payload = Vec::new();
    sign_payload.extend_from_slice(my_did.as_bytes());
    sign_payload.extend_from_slice(target_folder.as_bytes());
    sign_payload.extend_from_slice(manifest_hash.as_bytes());
    sign_payload.extend_from_slice(&nonce);
    sign_payload.extend_from_slice(&timestamp.to_le_bytes());
    let signature = keypair.sign(&sign_payload);

    // Parse target peer
    let target_peer: nexus_core::network::PeerId = target_peer_id.parse()
        .map_err(|e| format!("Invalid peer ID: {}", e))?;

    // Get node command channel
    let cmd_tx = state.command_tx().await
        .ok_or("Node not running")?;

    // Subscribe to push responses
    let mut push_rx = state.subscribe_pull_responses();

    // Dial target through relay circuit
    // Prefer contact's stored relay_addrs, fall back to global config
    let contact_relays = receiver_contact.relay_addrs.clone().unwrap_or_default();
    let global_relays: Vec<String> = get_config().relay_servers.iter().map(|e| e.addr.clone()).collect();
    let relay_addrs: Vec<String> = if !contact_relays.is_empty() {
        contact_relays
    } else {
        global_relays
    };

    for relay_addr_str in &relay_addrs {
        eprintln!("[push] Trying relay: {}", relay_addr_str);
        if let Ok(relay_ma) = relay_addr_str.parse::<nexus_core::network::Multiaddr>() {
            eprintln!("[push] Hole-punching via relay: {} -> {}", relay_addr_str, target_peer);
            let _ = cmd_tx.send(NodeCommand::HolePunch {
                peer: target_peer,
                relay_addr: relay_ma,
            }).await;
        }
    }

    // Wait for circuit connection to establish
    tokio::time::sleep(std::time::Duration::from_secs(3)).await;

    // Emit initial progress
    let _ = app_handle.emit("nexus://push-send-progress", PushSendProgress {
        status: "requesting".into(),
        filename: filename.clone(),
        shards_sent: 0,
        shards_total: shard_count,
        asset_id: None,
        error: None,
    });

    // Send PushRequest
    cmd_tx.send(NodeCommand::PushRequest {
        peer: target_peer,
        sender_did: my_did.clone(),
        target_folder: target_folder.to_string(),
        filename: filename.clone(),
        total_size,
        shard_count,
        manifest_hash,
        signature,
        nonce,
        timestamp,
    }).await.map_err(|e| format!("Failed to send push request: {}", e))?;

    // Wait for PushAccepted or PushDenied
    let session_id = tokio::time::timeout(std::time::Duration::from_secs(10), async {
        loop {
            match push_rx.recv().await {
                Ok(resp) if resp.peer == target_peer => {
                    match resp.response {
                        NexusResponse::PushAccepted { session_id } => return Ok(session_id),
                        NexusResponse::PushDenied { reason } => return Err(format!("Push denied: {}", reason)),
                        _ => {}
                    }
                }
                Err(e) => return Err(format!("Channel error: {}", e)),
                _ => {}
            }
        }
    }).await
        .map_err(|_| "Timeout waiting for push accept/deny".to_string())??;

    let _ = app_handle.emit("nexus://push-send-progress", PushSendProgress {
        status: "streaming".into(),
        filename: filename.clone(),
        shards_sent: 0,
        shards_total: shard_count,
        asset_id: None,
        error: None,
    });

    // Stream shards
    for (i, shard) in shards.iter().enumerate() {
        let cid_hex = hex_encode(&shard.cid);
        cmd_tx.send(NodeCommand::PushData {
            peer: target_peer,
            session_id: session_id.clone(),
            shard_index: i,
            cid: cid_hex,
            data: shard.data.clone(),
        }).await.map_err(|e| format!("Failed to send shard {}: {}", i, e))?;

        // Wait for ack
        let _ack = tokio::time::timeout(std::time::Duration::from_secs(30), async {
            loop {
                match push_rx.recv().await {
                    Ok(resp) if resp.peer == target_peer => {
                        match resp.response {
                            NexusResponse::PushShardAck { shard_index, .. } if shard_index == i => return Ok(()),
                            NexusResponse::PushFailed { reason, .. } => return Err(format!("Push failed at shard {}: {}", i, reason)),
                            _ => {}
                        }
                    }
                    Err(e) => return Err(format!("Channel error: {}", e)),
                    _ => {}
                }
            }
        }).await
            .map_err(|_| format!("Timeout waiting for shard {} ack", i))??;

        let _ = app_handle.emit("nexus://push-send-progress", PushSendProgress {
            status: "streaming".into(),
            filename: filename.clone(),
            shards_sent: i + 1,
            shards_total: shard_count,
            asset_id: None,
            error: None,
        });
    }

    // Send PushComplete
    cmd_tx.send(NodeCommand::PushComplete {
        peer: target_peer,
        session_id: session_id.clone(),
        manifest: manifest_bytes,
    }).await.map_err(|e| format!("Failed to send push complete: {}", e))?;

    // Wait for PushStored
    let asset_id = tokio::time::timeout(std::time::Duration::from_secs(30), async {
        loop {
            match push_rx.recv().await {
                Ok(resp) if resp.peer == target_peer => {
                    match resp.response {
                        NexusResponse::PushStored { asset_id, .. } => return Ok(asset_id),
                        NexusResponse::PushFailed { reason, .. } => return Err(format!("Push failed: {}", reason)),
                        _ => {}
                    }
                }
                Err(e) => return Err(format!("Channel error: {}", e)),
                _ => {}
            }
        }
    }).await
        .map_err(|_| "Timeout waiting for storage confirmation".to_string())??;

    let _ = app_handle.emit("nexus://push-send-progress", PushSendProgress {
        status: "complete".into(),
        filename: filename.clone(),
        shards_sent: shard_count,
        shards_total: shard_count,
        asset_id: Some(asset_id.clone()),
        error: None,
    });

    Ok(asset_id)
}

// --- Node lifecycle commands ---

#[tauri::command]
pub async fn start_node(
    vault_path: &str,
    passphrase: &str,
    listen_port: Option<u16>,
    state: State<'_, NodeState>,
    app_handle: tauri::AppHandle,
) -> Result<String, String> {
    let (keypair, _pre_kp) = load_keys(vault_path, passphrase)?;

    // Load saved config, prefer explicit listen_port param if provided
    let saved = get_config();
    let port = listen_port.or(saved.listen_port).unwrap_or(0);
    let bootstrap_peers: Vec<(nexus_core::network::PeerId, nexus_core::network::Multiaddr)> = saved.bootstrap_peers.iter()
        .filter_map(|addr_str| {
            let ma: nexus_core::network::Multiaddr = addr_str.parse().ok()?;
            // Extract peer id from the /p2p/<id> component
            let addr_string = addr_str.to_string();
            let p2p_idx = addr_string.rfind("/p2p/")?;
            let peer_id_str = &addr_string[p2p_idx + 5..];
            let peer_id: nexus_core::network::PeerId = peer_id_str.parse().ok()?;
            Some((peer_id, ma))
        })
        .collect();

    let relay_addrs: Vec<String> = saved.relay_servers.iter().map(|e| e.addr.clone()).collect();
    eprintln!("[start_node] Config loaded - relay_servers: {:?}, port: {}", relay_addrs, port);

    // Sync all legacy contacts to ACL store on startup
    sync_all_contacts_to_acl();

    let config = NodeConfig {
        listen_addrs: vec![
            format!("/ip4/0.0.0.0/tcp/{}", port),
            format!("/ip4/0.0.0.0/udp/{}/quic-v1", port),
        ],
        bootstrap_peers,
        mdns_enabled: true,
        relay_servers: relay_addrs,
        telemetry_enabled: true,
        telemetry_dir: None,
    };

    state.start(keypair, config, app_handle).await
}

#[tauri::command]
pub async fn stop_node(state: State<'_, NodeState>) -> Result<(), String> {
    state.stop().await
}

#[tauri::command]
pub async fn get_node_info(state: State<'_, NodeState>) -> Result<NodeInfo, String> {
    Ok(state.info().await)
}

// --- Config persistence ---

const CONFIG_PATH: &str = ".nexus-config.json";

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct RelayServerEntry {
    pub name: String,
    pub addr: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AppConfig {
    pub listen_port: Option<u16>,
    pub bootstrap_peers: Vec<String>,
    #[serde(default, deserialize_with = "deserialize_relay_servers")]
    pub relay_servers: Vec<RelayServerEntry>,
    #[serde(default = "default_telemetry_enabled")]
    pub telemetry_enabled: bool,
    #[serde(default)]
    pub auto_start_node: bool,
    #[serde(default)]
    pub auto_start_relay: bool,
    #[serde(default = "default_relay_port")]
    pub relay_port: u16,
    #[serde(default = "default_relay_max_circuits")]
    pub relay_max_circuits: u32,
    #[serde(default)]
    pub use_local_relay: bool,
}

fn default_relay_port() -> u16 { 4002 }
fn default_relay_max_circuits() -> u32 { 128 }

fn default_telemetry_enabled() -> bool { true }

/// Backwards-compatible deserializer: accepts either Vec<String> (old format)
/// or Vec<RelayServerEntry> (new format).
fn deserialize_relay_servers<'de, D>(deserializer: D) -> Result<Vec<RelayServerEntry>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum RelayItem {
        Entry(RelayServerEntry),
        Plain(String),
    }

    let items: Vec<RelayItem> = Vec::deserialize(deserializer)?;
    Ok(items
        .into_iter()
        .map(|item| match item {
            RelayItem::Entry(e) => e,
            RelayItem::Plain(addr) => RelayServerEntry {
                name: String::new(),
                addr,
            },
        })
        .collect())
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            listen_port: None,
            bootstrap_peers: vec![],
            relay_servers: vec![],
            telemetry_enabled: true,
            auto_start_node: false,
            auto_start_relay: false,
            relay_port: default_relay_port(),
            relay_max_circuits: default_relay_max_circuits(),
            use_local_relay: false,
        }
    }
}

#[tauri::command]
pub fn get_config() -> AppConfig {
    fs::read_to_string(CONFIG_PATH)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

#[tauri::command]
pub fn save_config(config: AppConfig) -> Result<(), String> {
    let json = serde_json::to_string_pretty(&config)
        .map_err(|e| format!("Serialize error: {}", e))?;
    fs::write(CONFIG_PATH, json)
        .map_err(|e| format!("Write error: {}", e))
}

#[tauri::command]
pub fn delete_file(manifest_path: &str) -> Result<(), String> {
    // Read manifest to find shard CIDs
    let manifest_json = fs::read_to_string(manifest_path)
        .map_err(|e| format!("Failed to read manifest: {}", e))?;
    let manifest: NexusManifest = serde_json::from_str(&manifest_json)
        .map_err(|e| format!("Invalid manifest: {}", e))?;

    // Remove shards from local store
    if let Ok(store) = ShardStore::open(".nexus-store") {
        for cid_hex in &manifest.shards.shards {
            let _ = store.remove(cid_hex);
        }
    }

    // Remove manifest file
    fs::remove_file(manifest_path)
        .map_err(|e| format!("Failed to delete manifest: {}", e))?;

    Ok(())
}

#[tauri::command]
pub fn rename_file(manifest_path: &str, new_name: &str) -> Result<(), String> {
    let manifest_json = fs::read_to_string(manifest_path)
        .map_err(|e| format!("Failed to read manifest: {}", e))?;
    let mut manifest: NexusManifest = serde_json::from_str(&manifest_json)
        .map_err(|e| format!("Invalid manifest: {}", e))?;

    manifest.shards.filename = Some(new_name.to_string());

    let json = serde_json::to_string_pretty(&manifest)
        .map_err(|e| format!("Serialize error: {}", e))?;
    fs::write(manifest_path, json)
        .map_err(|e| format!("Failed to write manifest: {}", e))?;

    Ok(())
}

#[tauri::command]
pub fn get_connectivity_stats() -> Result<serde_json::Value, String> {
    use nexus_core::network::TelemetryCollector;

    let collector = TelemetryCollector::new(
        ".nexus-telemetry",
        "unknown".to_string(),
        true,
    );
    let stats = collector.stats();
    serde_json::to_value(&stats).map_err(|e| e.to_string())
}

/// Export an encrypted file as a portable .nexus bundle (tar of manifest + shards)
#[tauri::command]
pub fn export_file_bundle(manifest_path: &str, output_path: &str) -> Result<String, String> {
    use std::io::Write;
    use nexus_core::storage::ShardStore;
    use nexus_core::manifest::NexusManifest;

    let manifest_json = fs::read_to_string(manifest_path)
        .map_err(|e| format!("Failed to read manifest: {}", e))?;
    let manifest: NexusManifest = serde_json::from_str(&manifest_json)
        .map_err(|e| format!("Invalid manifest: {}", e))?;

    let store = ShardStore::open(".nexus-store")
        .map_err(|e| format!("Failed to open store: {}", e))?;

    // Create a simple tar-like bundle: JSON header + shard data
    let mut bundle = Vec::new();

    // Write manifest as first entry
    let manifest_bytes = manifest_json.as_bytes();
    let header = serde_json::json!({
        "version": 1,
        "manifest_size": manifest_bytes.len(),
        "shard_count": manifest.shards.shards.len(),
        "original_name": manifest.shards.filename.as_deref().unwrap_or("unnamed"),
    });
    let header_bytes = serde_json::to_vec(&header)
        .map_err(|e| format!("Header serialize error: {}", e))?;

    // Format: [4-byte header len][header json][manifest json][shard data...]
    bundle.write_all(&(header_bytes.len() as u32).to_le_bytes())
        .map_err(|e| e.to_string())?;
    bundle.write_all(&header_bytes)
        .map_err(|e| e.to_string())?;
    bundle.write_all(manifest_bytes)
        .map_err(|e| e.to_string())?;

    // Write each shard: [4-byte len][shard data]
    for cid_hex in &manifest.shards.shards {
        let shard = store.get(cid_hex)
            .map_err(|e| format!("Failed to read shard {}: {}", cid_hex, e))?
            .ok_or_else(|| format!("Missing shard: {}", cid_hex))?;
        bundle.write_all(&(shard.data.len() as u32).to_le_bytes())
            .map_err(|e| e.to_string())?;
        bundle.write_all(&shard.data)
            .map_err(|e| e.to_string())?;
    }

    fs::write(output_path, &bundle)
        .map_err(|e| format!("Failed to write bundle: {}", e))?;

    Ok(format!("{} bytes written", bundle.len()))
}

/// Import a .nexus bundle: extract manifest + shards into local store
#[tauri::command]
pub fn import_file_bundle(bundle_path: &str) -> Result<String, String> {
    use std::io::Read;
    use nexus_core::storage::ShardStore;
    use nexus_core::manifest::NexusManifest;

    let data = fs::read(bundle_path)
        .map_err(|e| format!("Failed to read bundle: {}", e))?;

    let mut cursor = std::io::Cursor::new(&data);
    let mut buf4 = [0u8; 4];

    // Read header
    std::io::Read::read_exact(&mut cursor, &mut buf4)
        .map_err(|e| format!("Invalid bundle (header len): {}", e))?;
    let header_len = u32::from_le_bytes(buf4) as usize;

    let mut header_bytes = vec![0u8; header_len];
    cursor.read_exact(&mut header_bytes)
        .map_err(|e| format!("Invalid bundle (header): {}", e))?;

    let header: serde_json::Value = serde_json::from_slice(&header_bytes)
        .map_err(|e| format!("Invalid bundle header JSON: {}", e))?;

    let manifest_size = header["manifest_size"].as_u64()
        .ok_or("Missing manifest_size in header")? as usize;
    let shard_count = header["shard_count"].as_u64()
        .ok_or("Missing shard_count in header")? as usize;

    // Read manifest
    let mut manifest_bytes = vec![0u8; manifest_size];
    cursor.read_exact(&mut manifest_bytes)
        .map_err(|e| format!("Invalid bundle (manifest): {}", e))?;

    let manifest_json = String::from_utf8(manifest_bytes.clone())
        .map_err(|e| format!("Manifest not UTF-8: {}", e))?;
    let _manifest: NexusManifest = serde_json::from_str(&manifest_json)
        .map_err(|e| format!("Invalid manifest: {}", e))?;

    // Store shards
    let store = ShardStore::open(".nexus-store")
        .map_err(|e| format!("Failed to open store: {}", e))?;

    for i in 0..shard_count {
        cursor.read_exact(&mut buf4)
            .map_err(|e| format!("Invalid bundle (shard {} len): {}", i, e))?;
        let shard_len = u32::from_le_bytes(buf4) as usize;

        let mut shard_data = vec![0u8; shard_len];
        cursor.read_exact(&mut shard_data)
            .map_err(|e| format!("Invalid bundle (shard {} data): {}", i, e))?;

        store.put_data(&shard_data)
            .map_err(|e| format!("Failed to store shard: {}", e))?;
    }

    // Save manifest to received files directory
    let _ = fs::create_dir_all("received");
    let manifest_filename = format!("received/{}.nexus",
        _manifest.shards.filename.as_deref().unwrap_or("imported"));
    fs::write(&manifest_filename, &manifest_json)
        .map_err(|e| format!("Failed to write manifest: {}", e))?;

    Ok(manifest_filename)
}

// --- Relay Server Commands ---

#[tauri::command]
pub async fn start_relay(
    port: Option<u16>,
    max_circuits: Option<u32>,
    max_reservations_per_peer: Option<u32>,
    state: State<'_, RelayState>,
) -> Result<String, String> {
    let actual_port = port.unwrap_or(4002);
    eprintln!("[start_relay] port={}, max_circuits={}, max_res_per_peer={}",
        actual_port, max_circuits.unwrap_or(128), max_reservations_per_peer.unwrap_or(4));
    state.start(
        actual_port,
        max_circuits.unwrap_or(128),
        max_reservations_per_peer.unwrap_or(4),
    ).await
}

#[tauri::command]
pub async fn stop_relay(state: State<'_, RelayState>) -> Result<(), String> {
    state.stop().await
}

#[tauri::command]
pub async fn get_relay_info(state: State<'_, RelayState>) -> Result<RelayInfo, String> {
    Ok(state.info().await)
}
