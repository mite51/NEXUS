use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use rand::Rng;
use tauri::State;

use nexus_core::identity::{IdentityKeypair, IdentityVault, Did};
use nexus_core::crypto::pre::{PreKeypair, PreSigner, PrePublicKey, reencrypt};
use nexus_core::crypto::{encrypt_data, decrypt_data, generate_dek};
use nexus_core::manifest::{NexusManifest, ShareGrant};
use nexus_core::storage::{shard_data, reassemble, ShardStore, DEFAULT_SHARD_SIZE};
use nexus_core::storage::shard::Shard;
use nexus_core::storage::{ReceivedFiles, ReceivedFile};
use nexus_core::network::{SendQueue, QueuedSend, SendStatus, NodeConfig};

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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
}

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
pub fn add_contact(name: &str, did: &str, pre_public_key_hex: Option<&str>, notes: Option<&str>) -> Result<Contact, String> {
    let mut file = load_contacts();

    // Check for duplicate DID
    if file.contacts.iter().any(|c| c.did == did) {
        return Err("Contact with this DID already exists".into());
    }

    let contact = Contact {
        name: name.to_string(),
        did: did.to_string(),
        pre_public_key_hex: pre_public_key_hex.map(|s| s.to_string()),
        notes: notes.map(|s| s.to_string()),
    };

    file.contacts.push(contact.clone());
    save_contacts(&file)?;
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
pub fn update_contact(did: &str, name: Option<&str>, pre_public_key_hex: Option<&str>, notes: Option<&str>) -> Result<Contact, String> {
    let mut file = load_contacts();
    let contact = file.contacts.iter_mut()
        .find(|c| c.did == did)
        .ok_or("Contact not found")?;

    if let Some(n) = name { contact.name = n.to_string(); }
    if let Some(pk) = pre_public_key_hex { contact.pre_public_key_hex = Some(pk.to_string()); }
    if let Some(n) = notes { contact.notes = Some(n.to_string()); }

    let updated = contact.clone();
    save_contacts(&file)?;
    Ok(updated)
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
    let manifest_json = fs::read_to_string(manifest_path)
        .map_err(|e| format!("Failed to read manifest: {}", e))?;
    let manifest: NexusManifest = serde_json::from_str(&manifest_json)
        .map_err(|e| format!("Invalid manifest: {}", e))?;

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

    // Build share grant
    let grant = ShareGrant {
        recipient: recipient_did.to_string(),
        recipient_pre_pk: recipient_pk,
        cfrags,
        verifying_key: vk,
        manifest_ref: manifest_path.to_string(),
    };

    // Write grant file
    let filename = Path::new(manifest_path)
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "file".into());
    let short_did = &recipient_did[recipient_did.len().saturating_sub(8)..];
    let grant_path = format!("{}.share-{}.json", filename, short_did);

    let grant_json = serde_json::to_string_pretty(&grant)
        .map_err(|e| format!("Serialization failed: {}", e))?;
    fs::write(&grant_path, &grant_json)
        .map_err(|e| format!("Failed to write grant: {}", e))?;

    Ok(ShareResult {
        grant_path,
        recipient: recipient_did.to_string(),
        cfrags_count: 1,
    })
}

const SEND_QUEUE_PATH: &str = ".nexus-send-queue.json";

#[derive(Serialize)]
pub struct QueuedSendInfo {
    pub id: String,
    pub recipient_did: String,
    pub recipient_peer_id: String,
    pub filename: String,
    pub status: String,
    pub queued_at: u64,
    pub attempts: u32,
}

impl From<QueuedSend> for QueuedSendInfo {
    fn from(s: QueuedSend) -> Self {
        let status = match &s.status {
            SendStatus::Pending => "pending".to_string(),
            SendStatus::InProgress => "in_progress".to_string(),
            SendStatus::Delivered => "delivered".to_string(),
            SendStatus::Failed { reason } => format!("failed: {}", reason),
        };
        Self {
            id: s.id,
            recipient_did: s.recipient_did,
            recipient_peer_id: s.recipient_peer_id,
            filename: s.filename,
            status,
            queued_at: s.queued_at,
            attempts: s.attempts,
        }
    }
}

#[tauri::command]
pub fn queue_send(
    manifest_path: &str,
    recipient_did: &str,
    recipient_peer_id: &str,
    recipient_addr: Option<&str>,
    share_grant_json: Option<&str>,
) -> Result<QueuedSendInfo, String> {
    // Load manifest to get filename + shard CIDs
    let manifest_json = fs::read_to_string(manifest_path)
        .map_err(|e| format!("Failed to read manifest: {}", e))?;
    let manifest: NexusManifest = serde_json::from_str(&manifest_json)
        .map_err(|e| format!("Invalid manifest: {}", e))?;

    let filename = manifest.shards.filename.unwrap_or_else(|| "unnamed".into());
    let shard_cids = manifest.shards.shards.clone();

    let queue = SendQueue::open(SEND_QUEUE_PATH);
    let send = queue.enqueue(
        recipient_did.to_string(),
        recipient_peer_id.to_string(),
        recipient_addr.map(|s| s.to_string()),
        manifest_path.to_string(),
        filename,
        share_grant_json.map(|s| s.to_string()),
        shard_cids,
    )?;

    Ok(send.into())
}

#[tauri::command]
pub fn list_send_queue() -> Result<Vec<QueuedSendInfo>, String> {
    let queue = SendQueue::open(SEND_QUEUE_PATH);
    Ok(queue.all().into_iter().map(|s| s.into()).collect())
}

#[tauri::command]
pub fn cancel_send(id: &str) -> Result<(), String> {
    let queue = SendQueue::open(SEND_QUEUE_PATH);
    queue.remove(id)
}

#[tauri::command]
pub fn retry_send(id: &str) -> Result<(), String> {
    let queue = SendQueue::open(SEND_QUEUE_PATH);
    queue.retry(id)
}

const RECEIVED_FILES_PATH: &str = ".nexus-received.json";

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

        pre_kp.decrypt_dek_reencrypted(&manifest.encrypted_dek, &grant.cfrags, &manifest.owner_pre_pk, &grant.verifying_key)
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

    let out = output_path
        .map(|s| s.to_string())
        .or(manifest.shards.filename.clone())
        .unwrap_or_else(|| "decrypted_output".into());

    fs::write(&out, &plaintext)
        .map_err(|e| format!("Failed to write: {}", e))?;

    // Mark as decrypted
    let _ = received_store.mark_decrypted(received_id);

    Ok(out)
}

#[tauri::command]
pub fn remove_received(id: &str) -> Result<(), String> {
    let store = ReceivedFiles::open(RECEIVED_FILES_PATH);
    store.remove(id)
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

    let config = NodeConfig {
        listen_addrs: vec![
            format!("/ip4/0.0.0.0/tcp/{}", port),
            format!("/ip4/0.0.0.0/udp/{}/quic-v1", port),
        ],
        bootstrap_peers,
        mdns_enabled: true,
        relay_servers: vec![],
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
pub struct AppConfig {
    pub listen_port: Option<u16>,
    pub bootstrap_peers: Vec<String>,
    #[serde(default)]
    pub relay_servers: Vec<String>,
    #[serde(default = "default_telemetry_enabled")]
    pub telemetry_enabled: bool,
}

fn default_telemetry_enabled() -> bool { true }

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            listen_port: None,
            bootstrap_peers: vec![],
            relay_servers: vec![],
            telemetry_enabled: true,
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
    vault_path: &str,
    passphrase: &str,
    port: Option<u16>,
    max_circuits: Option<u32>,
    max_reservations_per_peer: Option<u32>,
    state: State<'_, RelayState>,
) -> Result<String, String> {
    let (identity, _pre_kp) = load_keys(vault_path, passphrase)?;
    state.start(
        identity,
        port.unwrap_or(4002),
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
