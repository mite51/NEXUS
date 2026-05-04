use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use rand::Rng;

use nexus_core::identity::{IdentityKeypair, IdentityVault, Did};
use nexus_core::crypto::pre::{PreKeypair, PreSigner, PrePublicKey, reencrypt};
use nexus_core::crypto::{encrypt_data, decrypt_data, generate_dek};
use nexus_core::manifest::{NexusManifest, ShareGrant};
use nexus_core::storage::{shard_data, reassemble, ShardStore, DEFAULT_SHARD_SIZE};
use nexus_core::storage::shard::Shard;

// --- Response types ---

#[derive(Serialize)]
pub struct IdentityInfo {
    pub did: String,
    pub pre_public_key_hex: String,
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
    })
}

#[tauri::command]
pub fn get_identity(vault_path: &str, passphrase: &str) -> Result<IdentityInfo, String> {
    let (keypair, pre_kp) = load_keys(vault_path, passphrase)?;
    let did = Did::from_public_identity(&keypair.public_identity());
    Ok(IdentityInfo {
        did: did.0,
        pre_public_key_hex: hex_encode(&pre_kp.public_key().bytes),
    })
}

#[tauri::command]
pub fn encrypt_file(file_path: &str, vault_path: &str, passphrase: &str) -> Result<EncryptResult, String> {
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
        for s in &shards {
            let _ = store.put(s);
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
pub fn decrypt_file(manifest_path: &str, output_path: Option<&str>, vault_path: &str, passphrase: &str) -> Result<String, String> {
    let (_keypair, pre_kp) = load_keys(vault_path, passphrase)?;

    let manifest_json = fs::read_to_string(manifest_path)
        .map_err(|e| format!("Failed to read manifest: {}", e))?;
    let manifest: NexusManifest = serde_json::from_str(&manifest_json)
        .map_err(|e| format!("Invalid manifest: {}", e))?;

    let dek = pre_kp.decrypt_dek(&manifest.encrypted_dek)
        .map_err(|_| "Failed to decrypt — wrong key or not the owner".to_string())?;

    let manifest_dir = Path::new(manifest_path).parent().unwrap_or(Path::new("."));
    let shards_dir = manifest_dir.join("shards");

    let mut shards = Vec::new();
    for cid_hex in &manifest.shards.shards {
        // Try local store first, then shards directory
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
