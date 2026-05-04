use nexus_core::crypto::{decrypt_data, encrypt_data, generate_dek};
use nexus_core::identity::{Did, IdentityKeypair, IdentityVault};
use nexus_core::storage::shard::{self, ShardManifest, DEFAULT_SHARD_SIZE};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::{self, Write};
use std::path::Path;

/// Manifest stored on disk (includes capsule/encrypted DEK for PRE — placeholder for now)
#[derive(Serialize, Deserialize)]
struct NexusManifest {
    /// Owner's DID
    owner: String,
    /// Shard manifest (CIDs, sizes, etc.)
    shards: ShardManifest,
    /// Encrypted DEK (hex) — for now, directly encrypted with owner's key via AES
    /// Will be replaced with umbral Capsule + encrypted_dek once PRE is integrated
    encrypted_dek: String,
    /// Nonce for DEK encryption (hex)
    dek_nonce: String,
}

pub fn init(vault_path: &str) -> Result<(), String> {
    if Path::new(vault_path).exists() {
        return Err(format!("Vault already exists at: {}", vault_path));
    }

    // Prompt for passphrase
    let passphrase = prompt_passphrase("Set vault passphrase: ")?;
    let confirm = prompt_passphrase("Confirm passphrase: ")?;

    if passphrase != confirm {
        return Err("Passphrases don't match".into());
    }

    let keypair = IdentityKeypair::generate();
    let did = Did::from_public_identity(&keypair.public_identity());

    let vault = IdentityVault::seal(&keypair, &passphrase)
        .map_err(|e| format!("Failed to create vault: {}", e))?;

    let json = serde_json::to_string_pretty(&vault)
        .map_err(|e| format!("Serialization error: {}", e))?;

    fs::write(vault_path, json)
        .map_err(|e| format!("Failed to write vault: {}", e))?;

    println!("✓ Identity created");
    println!("  DID: {}", did);
    println!("  Vault: {}", vault_path);
    println!();
    println!("  Keep your passphrase safe — it's the only way to unlock your identity.");

    Ok(())
}

pub fn identity(vault_path: &str) -> Result<(), String> {
    let passphrase = prompt_passphrase("Vault passphrase: ")?;
    let keypair = load_keypair(vault_path, &passphrase)?;
    let did = Did::from_public_identity(&keypair.public_identity());

    println!("DID: {}", did);
    println!("Public key: {}", hex_encode(&keypair.public_identity().public_key));

    Ok(())
}

pub fn encrypt(file_path: &str, output_dir: &str, vault_path: &str) -> Result<(), String> {
    let passphrase = prompt_passphrase("Vault passphrase: ")?;
    let keypair = load_keypair(vault_path, &passphrase)?;
    let did = Did::from_public_identity(&keypair.public_identity());

    // Read input file
    let plaintext = fs::read(file_path)
        .map_err(|e| format!("Failed to read file: {}", e))?;

    let filename = Path::new(file_path)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "unnamed".into());

    // Generate DEK and encrypt file body
    let dek = generate_dek();
    let encrypted_body = encrypt_data(&plaintext, &dek)
        .map_err(|e| format!("Encryption failed: {}", e))?;

    // Shard the encrypted data
    let (mut shard_manifest, shards) = shard::shard_data(&encrypted_body, DEFAULT_SHARD_SIZE);
    shard_manifest.filename = Some(filename.clone());

    // Write shards to output directory
    let shards_dir = Path::new(output_dir).join("shards");
    fs::create_dir_all(&shards_dir)
        .map_err(|e| format!("Failed to create shards dir: {}", e))?;

    for s in &shards {
        let shard_path = shards_dir.join(hex_encode(&s.cid));
        fs::write(&shard_path, &s.data)
            .map_err(|e| format!("Failed to write shard: {}", e))?;
    }

    // Encrypt the DEK for ourselves (simple AES wrap for now)
    // TODO: Replace with umbral PRE capsule once integrated
    let dek_key = derive_dek_wrapping_key(&keypair);
    let encrypted_dek_blob = encrypt_data(&dek, &dek_key)
        .map_err(|e| format!("DEK wrapping failed: {}", e))?;

    // Split nonce from ciphertext for storage
    let dek_nonce = hex_encode(&encrypted_dek_blob[..12]);
    let dek_ct = hex_encode(&encrypted_dek_blob[12..]);

    // Write manifest
    let manifest = NexusManifest {
        owner: did.0.clone(),
        shards: shard_manifest,
        encrypted_dek: dek_ct,
        dek_nonce,
    };

    let manifest_path = Path::new(output_dir).join(format!("{}.nexus", filename));
    let manifest_json = serde_json::to_string_pretty(&manifest)
        .map_err(|e| format!("Manifest serialization failed: {}", e))?;
    fs::write(&manifest_path, manifest_json)
        .map_err(|e| format!("Failed to write manifest: {}", e))?;

    println!("✓ Encrypted: {}", filename);
    println!("  Shards: {} ({} bytes each)", shards.len(), DEFAULT_SHARD_SIZE);
    println!("  Manifest: {}", manifest_path.display());

    Ok(())
}

pub fn decrypt(manifest_path: &str, output_path: Option<&str>, vault_path: &str) -> Result<(), String> {
    let passphrase = prompt_passphrase("Vault passphrase: ")?;
    let keypair = load_keypair(vault_path, &passphrase)?;

    // Load manifest
    let manifest_json = fs::read_to_string(manifest_path)
        .map_err(|e| format!("Failed to read manifest: {}", e))?;
    let manifest: NexusManifest = serde_json::from_str(&manifest_json)
        .map_err(|e| format!("Invalid manifest: {}", e))?;

    // Decrypt the DEK
    let dek_key = derive_dek_wrapping_key(&keypair);
    let mut encrypted_dek_blob = hex_decode(&manifest.dek_nonce)?;
    encrypted_dek_blob.extend(hex_decode(&manifest.encrypted_dek)?);

    let dek_bytes = decrypt_data(&encrypted_dek_blob, &dek_key)
        .map_err(|_| "Failed to decrypt DEK — wrong identity or corrupted manifest".to_string())?;

    if dek_bytes.len() != 32 {
        return Err("Invalid DEK size".into());
    }
    let mut dek = [0u8; 32];
    dek.copy_from_slice(&dek_bytes);

    // Load shards
    let manifest_dir = Path::new(manifest_path).parent().unwrap_or(Path::new("."));
    let shards_dir = manifest_dir.join("shards");

    let mut shards = Vec::new();
    for cid_hex in &manifest.shards.shards {
        let shard_path = shards_dir.join(cid_hex);
        let data = fs::read(&shard_path)
            .map_err(|e| format!("Failed to read shard {}: {}", cid_hex, e))?;
        shards.push(nexus_core::storage::shard::Shard {
            cid: hex_decode(cid_hex)?,
            data,
        });
    }

    // Reassemble encrypted body
    let encrypted_body = shard::reassemble(&manifest.shards, &shards)
        .ok_or("Failed to reassemble shards — missing or corrupted")?;

    // Decrypt file body
    let plaintext = decrypt_data(&encrypted_body, &dek)
        .map_err(|_| "Decryption failed — corrupted data".to_string())?;

    // Write output
    let out = output_path
        .map(|s| s.to_string())
        .or(manifest.shards.filename.clone())
        .unwrap_or_else(|| "decrypted_output".into());

    fs::write(&out, &plaintext)
        .map_err(|e| format!("Failed to write output: {}", e))?;

    println!("✓ Decrypted: {} ({} bytes)", out, plaintext.len());

    Ok(())
}

pub fn share(_manifest_path: &str, _to_did: &str, _vault_path: &str) -> Result<(), String> {
    // TODO: Implement once umbral-pre is integrated
    // Flow:
    // 1. Load manifest + decrypt DEK with owner's key
    // 2. Re-encrypt DEK for recipient's DID using umbral PRE
    // 3. Output a .nexus-share file containing: capsule, cfrag, recipient DID
    println!("⚠ Share command requires PRE integration (coming in next iteration)");
    println!("  This will generate a re-encryption key for the recipient's DID.");
    Ok(())
}

// --- Helpers ---

fn load_keypair(vault_path: &str, passphrase: &str) -> Result<IdentityKeypair, String> {
    let json = fs::read_to_string(vault_path)
        .map_err(|e| format!("Failed to read vault: {}", e))?;
    let vault: IdentityVault = serde_json::from_str(&json)
        .map_err(|e| format!("Invalid vault file: {}", e))?;
    vault.unseal(passphrase)
        .map_err(|e| format!("Failed to unlock vault: {}", e))
}

/// Derive a DEK-wrapping key from the identity keypair
/// This is a temporary approach — will be replaced by umbral PRE
fn derive_dek_wrapping_key(keypair: &IdentityKeypair) -> [u8; 32] {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    // Simple KDF: SHA-256 of secret key bytes (placeholder)
    // TODO: Replace with proper HKDF once umbral handles key wrapping
    let secret = keypair.to_secret_bytes();
    let mut key = [0u8; 32];
    // Use a basic hash — this is temporary until PRE replaces it
    for (i, chunk) in secret.chunks(4).enumerate() {
        let mut hasher = DefaultHasher::new();
        chunk.hash(&mut hasher);
        i.hash(&mut hasher);
        let h = hasher.finish().to_le_bytes();
        let start = (i * 4) % 32;
        let end = (start + 8).min(32);
        key[start..end].copy_from_slice(&h[..end - start]);
    }
    key
}

fn prompt_passphrase(prompt: &str) -> Result<String, String> {
    eprint!("{}", prompt);
    io::stderr().flush().ok();
    let mut input = String::new();
    io::stdin().read_line(&mut input)
        .map_err(|e| format!("Failed to read input: {}", e))?;
    Ok(input.trim().to_string())
}

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

fn hex_decode(s: &str) -> Result<Vec<u8>, String> {
    if s.len() % 2 != 0 {
        return Err("Invalid hex string".into());
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).map_err(|_| "Invalid hex".into()))
        .collect()
}
