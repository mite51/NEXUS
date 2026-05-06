use nexus_core::crypto::{decrypt_data, encrypt_data, generate_dek};
use nexus_core::crypto::pre::{self, PreKeypair, PreSigner, PrePublicKey};
use nexus_core::identity::{Did, IdentityKeypair, IdentityVault};
use nexus_core::manifest::{NexusManifest, ShareGrant};
use nexus_core::network::{NexusNode, NodeConfig, NodeEvent};
use nexus_core::network::{RelayServer, RelayConfig, RelayServerEvent};
use nexus_core::storage::shard::{self, Shard, DEFAULT_SHARD_SIZE};
use nexus_core::storage::{ShardStore, compute_cid};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::{self, Write};
use std::path::Path;
use std::time::Duration;

pub fn init(vault_path: &str) -> Result<(), String> {
    if Path::new(vault_path).exists() {
        return Err(format!("Vault already exists at: {}", vault_path));
    }

    let passphrase = prompt_passphrase("Set vault passphrase: ")?;
    let confirm = prompt_passphrase("Confirm passphrase: ")?;

    if passphrase != confirm {
        return Err("Passphrases don't match".into());
    }

    // Generate Ed25519 identity keypair
    let keypair = IdentityKeypair::generate();
    let did = Did::from_public_identity(&keypair.public_identity());

    // Generate PRE keypair (secp256k1 for Umbral)
    let pre_keypair = PreKeypair::generate();
    let pre_pk = pre_keypair.public_key();

    // Store both in vault
    let vault = IdentityVault::seal(&keypair, &passphrase)
        .map_err(|e| format!("Failed to create vault: {}", e))?;

    // Save vault + PRE seed together
    let vault_data = VaultFile {
        identity_vault: vault,
        pre_seed: hex_encode(&pre_keypair.to_secret_bytes()),
        pre_public_key: pre_pk.clone(),
    };

    let json = serde_json::to_string_pretty(&vault_data)
        .map_err(|e| format!("Serialization error: {}", e))?;

    fs::write(vault_path, json)
        .map_err(|e| format!("Failed to write vault: {}", e))?;

    println!("✓ Identity created");
    println!("  DID: {}", did);
    println!("  PRE public key: {} bytes", pre_pk.bytes.len());
    println!("  Vault: {}", vault_path);
    println!();
    println!("  Keep your passphrase safe — it's the only way to unlock your identity.");

    Ok(())
}

pub fn identity(vault_path: &str) -> Result<(), String> {
    let passphrase = prompt_passphrase("Vault passphrase: ")?;
    let (keypair, pre_kp) = load_keys(vault_path, &passphrase)?;
    let did = Did::from_public_identity(&keypair.public_identity());

    println!("DID: {}", did);
    println!("Ed25519 public key: {}", hex_encode(&keypair.public_identity().public_key));
    println!("PRE public key (hex): {}", hex_encode(&pre_kp.public_key().bytes));

    Ok(())
}

pub fn encrypt(file_path: &str, output_dir: &str, vault_path: &str) -> Result<(), String> {
    let passphrase = prompt_passphrase("Vault passphrase: ")?;
    let (keypair, pre_kp) = load_keys(vault_path, &passphrase)?;
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

    // Also store shards in local store for P2P serving
    let store = ShardStore::open(".nexus-store").ok();
    if let Some(ref store) = store {
        for s in &shards {
            let _ = store.put(s);
        }
    }

    // Encrypt DEK using Umbral PRE (owner can always decrypt their own capsule)
    let encrypted_dek = pre_kp.encrypt_dek(&dek)
        .map_err(|e| format!("PRE encryption failed: {}", e))?;

    // Write manifest
    let manifest = NexusManifest {
        owner: did.0.clone(),
        owner_pre_pk: pre_kp.public_key(),
        shards: shard_manifest,
        encrypted_dek,
    };

    let manifest_path = Path::new(output_dir).join(format!("{}.nexus", filename));
    let manifest_json = serde_json::to_string_pretty(&manifest)
        .map_err(|e| format!("Manifest serialization failed: {}", e))?;
    fs::write(&manifest_path, manifest_json)
        .map_err(|e| format!("Failed to write manifest: {}", e))?;

    println!("✓ Encrypted: {}", filename);
    println!("  Shards: {} ({} bytes each)", shards.len(), DEFAULT_SHARD_SIZE);
    println!("  Manifest: {}", manifest_path.display());
    if store.is_some() {
        println!("  📦 Shards stored locally (available for P2P serving)");
    }
    println!("  DEK wrapped with Umbral PRE (owner-recoverable + shareable)");

    Ok(())
}

pub fn decrypt(manifest_path: &str, output_path: Option<&str>, vault_path: &str) -> Result<(), String> {
    let passphrase = prompt_passphrase("Vault passphrase: ")?;
    let (_keypair, pre_kp) = load_keys(vault_path, &passphrase)?;

    // Load manifest
    let manifest_json = fs::read_to_string(manifest_path)
        .map_err(|e| format!("Failed to read manifest: {}", e))?;
    let manifest: NexusManifest = serde_json::from_str(&manifest_json)
        .map_err(|e| format!("Invalid manifest: {}", e))?;

    // Try owner decryption first
    let dek = pre_kp.decrypt_dek(&manifest.encrypted_dek)
        .map_err(|_| {
            // Maybe we're a recipient with a share grant?
            "Failed to decrypt DEK — you may need a .nexus-share file if you're not the owner"
        })?;

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

pub fn decrypt_shared(
    manifest_path: &str,
    share_path: &str,
    output_path: Option<&str>,
    vault_path: &str,
) -> Result<(), String> {
    let passphrase = prompt_passphrase("Vault passphrase: ")?;
    let (_keypair, pre_kp) = load_keys(vault_path, &passphrase)?;

    // Load manifest
    let manifest_json = fs::read_to_string(manifest_path)
        .map_err(|e| format!("Failed to read manifest: {}", e))?;
    let manifest: NexusManifest = serde_json::from_str(&manifest_json)
        .map_err(|e| format!("Invalid manifest: {}", e))?;

    // Load share grant
    let share_json = fs::read_to_string(share_path)
        .map_err(|e| format!("Failed to read share grant: {}", e))?;
    let grant: ShareGrant = serde_json::from_str(&share_json)
        .map_err(|e| format!("Invalid share grant: {}", e))?;

    // Decrypt via re-encrypted cfrags
    let dek = pre_kp.decrypt_dek_reencrypted(
        &manifest.encrypted_dek,
        &grant.cfrags,
        &manifest.owner_pre_pk,
        &grant.verifying_key,
    ).map_err(|e| format!("Delegated decryption failed: {}", e))?;

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

    // Reassemble and decrypt
    let encrypted_body = shard::reassemble(&manifest.shards, &shards)
        .ok_or("Failed to reassemble shards — missing or corrupted")?;

    let plaintext = decrypt_data(&encrypted_body, &dek)
        .map_err(|_| "Decryption failed — corrupted data".to_string())?;

    let out = output_path
        .map(|s| s.to_string())
        .or(manifest.shards.filename.clone())
        .unwrap_or_else(|| "decrypted_output".into());

    fs::write(&out, &plaintext)
        .map_err(|e| format!("Failed to write output: {}", e))?;

    println!("✓ Decrypted (shared): {} ({} bytes)", out, plaintext.len());

    Ok(())
}

pub fn share(manifest_path: &str, recipient_pk_path: &str, vault_path: &str) -> Result<(), String> {
    let passphrase = prompt_passphrase("Vault passphrase: ")?;
    let (keypair, pre_kp) = load_keys(vault_path, &passphrase)?;
    let did = Did::from_public_identity(&keypair.public_identity());

    // Load manifest to verify ownership
    let manifest_json = fs::read_to_string(manifest_path)
        .map_err(|e| format!("Failed to read manifest: {}", e))?;
    let manifest: NexusManifest = serde_json::from_str(&manifest_json)
        .map_err(|e| format!("Invalid manifest: {}", e))?;

    if manifest.owner != did.0 {
        return Err("You are not the owner of this file".into());
    }

    // Load recipient's public key
    let recipient_json = fs::read_to_string(recipient_pk_path)
        .map_err(|e| format!("Failed to read recipient key: {}", e))?;
    let recipient_info: RecipientKey = serde_json::from_str(&recipient_json)
        .map_err(|e| format!("Invalid recipient key file: {}", e))?;

    // Generate signing key and kfrags (threshold=1, shares=1 for direct 1:1 sharing)
    let signer = PreSigner::new();
    let vk = signer.verifying_key();

    let kfrags = signer.generate_kfrags(&pre_kp, &recipient_info.pre_public_key, 1, 1)
        .map_err(|e| format!("kfrag generation failed: {}", e))?;

    // Re-encrypt the capsule (we act as our own proxy here)
    let cfrag = pre::reencrypt(
        &manifest.encrypted_dek,
        &kfrags[0],
        &pre_kp.public_key(),
        &recipient_info.pre_public_key,
        &vk,
    ).map_err(|e| format!("Re-encryption failed: {}", e))?;

    // Write share grant
    let grant = ShareGrant {
        recipient: recipient_info.did.clone(),
        recipient_pre_pk: recipient_info.pre_public_key.clone(),
        cfrags: vec![cfrag],
        verifying_key: vk,
        manifest_ref: manifest_path.to_string(),
    };

    let share_filename = format!(
        "{}.share-{}.json",
        Path::new(manifest_path).file_stem().unwrap_or_default().to_string_lossy(),
        &recipient_info.did[recipient_info.did.len().saturating_sub(8)..]
    );

    let share_json = serde_json::to_string_pretty(&grant)
        .map_err(|e| format!("Serialization failed: {}", e))?;

    fs::write(&share_filename, share_json)
        .map_err(|e| format!("Failed to write share grant: {}", e))?;

    println!("✓ Shared with: {}", recipient_info.did);
    println!("  Grant file: {}", share_filename);
    println!("  Send this file + the shards to the recipient.");
    println!("  They can decrypt with: nexus decrypt-shared <manifest> --share {}", share_filename);

    Ok(())
}

pub fn export_key(vault_path: &str) -> Result<(), String> {
    let passphrase = prompt_passphrase("Vault passphrase: ")?;
    let (keypair, pre_kp) = load_keys(vault_path, &passphrase)?;
    let did = Did::from_public_identity(&keypair.public_identity());

    let recipient_key = RecipientKey {
        did: did.0.clone(),
        pre_public_key: pre_kp.public_key(),
    };

    let filename = format!("{}.pubkey.json", &did.0[did.0.len().saturating_sub(12)..]);
    let json = serde_json::to_string_pretty(&recipient_key)
        .map_err(|e| format!("Serialization failed: {}", e))?;
    fs::write(&filename, &json)
        .map_err(|e| format!("Failed to write: {}", e))?;

    println!("✓ Exported public key: {}", filename);
    println!("  Share this file with anyone who wants to send you encrypted files.");

    Ok(())
}

// --- Data Structures ---

/// Combined vault file (identity + PRE key)
#[derive(Serialize, Deserialize)]
struct VaultFile {
    identity_vault: IdentityVault,
    pre_seed: String,
    pre_public_key: PrePublicKey,
}

/// Exported public key for receiving shared files
#[derive(Serialize, Deserialize)]
struct RecipientKey {
    did: String,
    pre_public_key: PrePublicKey,
}

// --- Helpers ---

fn load_keys(vault_path: &str, passphrase: &str) -> Result<(IdentityKeypair, PreKeypair), String> {
    let json = fs::read_to_string(vault_path)
        .map_err(|e| format!("Failed to read vault: {}", e))?;
    let vault_file: VaultFile = serde_json::from_str(&json)
        .map_err(|e| format!("Invalid vault file: {}", e))?;

    let keypair = vault_file.identity_vault.unseal(passphrase)
        .map_err(|e| format!("Failed to unlock vault: {}", e))?;

    let pre_seed = hex_decode(&vault_file.pre_seed)?;
    let pre_kp = PreKeypair::from_secret_bytes(&pre_seed)
        .map_err(|e| format!("Failed to restore PRE key: {}", e))?;

    Ok((keypair, pre_kp))
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

// --- Network Commands ---

pub async fn run_node(vault_path: &str, listen_addrs: &[String], bootstrap: &[String], relay_servers: &[String]) -> Result<(), String> {
    let passphrase = prompt_passphrase("Vault passphrase: ")?;
    let (keypair, _pre_kp) = load_keys(vault_path, &passphrase)?;
    let did = Did::from_public_identity(&keypair.public_identity());

    // Convert identity key to libp2p keypair
    let libp2p_keypair = keypair.to_libp2p_keypair();
    let peer_id = libp2p_keypair.public().to_peer_id();

    println!("⚡ NEXUS Node");
    println!("  DID:     {}", did);
    println!("  PeerId:  {}", peer_id);
    println!();

    // Open local shard store
    let store = ShardStore::open(".nexus-store")
        .map_err(|e| format!("Failed to open shard store: {}", e))?;
    let stats = store.stats().unwrap_or(nexus_core::storage::StoreStats { shard_count: 0, total_bytes: 0 });
    println!("  📦 Store: {} shards ({:.2} MB)", stats.shard_count, stats.total_bytes as f64 / 1_048_576.0);
    println!();

    // Parse bootstrap peers
    let mut bootstrap_peers = Vec::new();
    for addr_str in bootstrap {
        let addr: libp2p::Multiaddr = addr_str.parse()
            .map_err(|e| format!("Invalid bootstrap addr '{}': {}", addr_str, e))?;
        if let Some(libp2p::multiaddr::Protocol::P2p(pid)) = addr.iter().last() {
            bootstrap_peers.push((pid, addr.clone()));
        } else {
            return Err(format!("Bootstrap addr must end with /p2p/<peer_id>: {}", addr_str));
        }
    }

    let config = NodeConfig {
        listen_addrs: listen_addrs.to_vec(),
        bootstrap_peers,
        mdns_enabled: true,
        relay_servers: relay_servers.to_vec(),
        telemetry_enabled: true,
        telemetry_dir: None,
    };

    let mut node = NexusNode::start(libp2p_keypair, config).await
        .map_err(|e| format!("Failed to start node: {}", e))?;

    println!("  Waiting for connections...");
    println!();

    // Event loop — serve shards automatically
    loop {
        match node.event_rx.recv().await {
            Some(NodeEvent::Listening(addr)) => {
                println!("  📡 Listening: {}/p2p/{}", addr, peer_id);
            }
            Some(NodeEvent::PeerDiscovered(peer)) => {
                println!("  ✅ Peer discovered: {}", peer);
            }
            Some(NodeEvent::PeerDisconnected(peer)) => {
                println!("  ❌ Peer disconnected: {}", peer);
            }
            Some(NodeEvent::ShardRequested { peer, cid, channel }) => {
                // Auto-serve from local store
                let response = match store.get(&cid) {
                    Ok(Some(shard)) => {
                        println!("  📦 Serving shard to {}: {}...", peer, &cid[..16.min(cid.len())]);
                        nexus_core::network::protocol::NexusResponse::Shard {
                            cid: cid.clone(),
                            data: shard.data,
                        }
                    }
                    _ => {
                        println!("  ❓ Shard not found: {}...", &cid[..16.min(cid.len())]);
                        nexus_core::network::protocol::NexusResponse::ShardNotFound { cid }
                    }
                };
                let _ = node.command_tx.send(
                    nexus_core::network::NodeCommand::RespondShard { channel, response }
                ).await;
            }
            Some(NodeEvent::KfragsReceived { peer, manifest_id, .. }) => {
                println!("  🔑 Kfrags received from {} for manifest {}", peer, manifest_id);
            }
            Some(NodeEvent::GossipMessage { topic, source, .. }) => {
                println!("  📢 Gossip on '{}' from {:?}", topic, source);
            }
            Some(NodeEvent::ShardReceived { peer, .. }) => {
                println!("  📥 Shard response from {}", peer);
            }
            Some(NodeEvent::ShardPushed { peer, cid, data, channel }) => {
                // Auto-accept pushed shards and store them
                let cid_short = &cid[..16.min(cid.len())];
                match store.put(&nexus_core::storage::Shard {
                    cid: nexus_core::storage::compute_cid(&data).to_vec(),
                    data,
                }) {
                    Ok(_) => {
                        println!("  📥 Received shard from {}: {}...", peer, cid_short);
                        let _ = node.command_tx.send(
                            nexus_core::network::NodeCommand::RespondShard {
                                channel,
                                response: nexus_core::network::protocol::NexusResponse::ShardAccepted { cid },
                            }
                        ).await;
                    }
                    Err(e) => {
                        println!("  ❌ Failed to store pushed shard: {}", e);
                        let _ = node.command_tx.send(
                            nexus_core::network::NodeCommand::RespondShard {
                                channel,
                                response: nexus_core::network::protocol::NexusResponse::Error {
                                    message: format!("Store failed: {}", e),
                                },
                            }
                        ).await;
                    }
                }
            }
            Some(NodeEvent::ManifestPushed { peer, manifest_json, share_grant_json, channel }) => {
                // Save manifest and optional share grant to disk
                let incoming_dir = ".nexus-incoming";
                let _ = fs::create_dir_all(incoming_dir);

                // Write manifest
                let manifest_filename = format!("{}/manifest-{}.nexus", incoming_dir, &peer.to_string()[..8]);
                let _ = fs::write(&manifest_filename, &manifest_json);
                println!("  📄 Manifest received from {}: {}", peer, manifest_filename);

                // Write share grant if present
                if let Some(ref grant_json) = share_grant_json {
                    let grant_filename = format!("{}/share-{}.json", incoming_dir, &peer.to_string()[..8]);
                    let _ = fs::write(&grant_filename, grant_json);
                    println!("  🔑 Share grant saved: {}", grant_filename);
                }

                let _ = node.command_tx.send(
                    nexus_core::network::NodeCommand::RespondShard {
                        channel,
                        response: nexus_core::network::protocol::NexusResponse::ManifestAccepted,
                    }
                ).await;
            }
            Some(NodeEvent::NatStatusChanged { status }) => {
                println!("  NAT status: {:?}", status);
            }
            Some(NodeEvent::RelayReserved { relay_peer, relay_addr }) => {
                println!("  Relay reserved: {} at {}", relay_peer, relay_addr);
            }
            Some(NodeEvent::HolePunchResult { remote_peer, success }) => {
                if success {
                    println!("  ✓ Hole punch succeeded with {}", remote_peer);
                } else {
                    println!("  ✗ Hole punch failed with {}", remote_peer);
                }
            }
            None => {
                println!("  Node event channel closed — shutting down.");
                break;
            }
        }
    }

    Ok(())
}

pub async fn ping_peer(vault_path: &str, addr_str: &str) -> Result<(), String> {
    let passphrase = prompt_passphrase("Vault passphrase: ")?;
    let (keypair, _) = load_keys(vault_path, &passphrase)?;

    let libp2p_keypair = keypair.to_libp2p_keypair();
    let _our_peer_id = libp2p_keypair.public().to_peer_id();

    let addr: libp2p::Multiaddr = addr_str.parse()
        .map_err(|e| format!("Invalid multiaddr: {}", e))?;

    // Extract target peer ID
    let target_peer_id = addr.iter()
        .find_map(|p| if let libp2p::multiaddr::Protocol::P2p(pid) = p { Some(pid) } else { None })
        .ok_or("Multiaddr must contain /p2p/<peer_id>")?;

    println!("Pinging {} ...", target_peer_id);

    let config = NodeConfig {
        listen_addrs: vec!["/ip4/0.0.0.0/udp/0/quic-v1".to_string()],
        bootstrap_peers: vec![],
        mdns_enabled: false,
        relay_servers: vec![],
        telemetry_enabled: true,
        telemetry_dir: None,
    };

    let mut node = NexusNode::start(libp2p_keypair, config).await
        .map_err(|e| format!("Failed to start node: {}", e))?;

    // Wait for listening
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Dial the peer
    node.dial(addr).await.map_err(|e| format!("Dial failed: {}", e))?;

    // Send a ping request
    use nexus_core::network::NodeCommand;
    node.command_tx.send(NodeCommand::RequestShard {
        peer: target_peer_id,
        cid: "__ping__".to_string(),
    }).await.map_err(|e| format!("Send failed: {}", e))?;

    // Wait for response (timeout 5s)
    let start = std::time::Instant::now();
    loop {
        if start.elapsed() > Duration::from_secs(5) {
            println!("❌ Timeout — peer did not respond within 5s");
            break;
        }
        match tokio::time::timeout(Duration::from_secs(5), node.event_rx.recv()).await {
            Ok(Some(NodeEvent::PeerDiscovered(peer))) if peer == target_peer_id => {
                let elapsed = start.elapsed();
                println!("✅ Connected to {} in {:?}", peer, elapsed);
                break;
            }
            Ok(Some(_)) => continue,
            _ => {
                println!("❌ Could not reach peer");
                break;
            }
        }
    }

    node.shutdown().await.map_err(|e| format!("Shutdown: {}", e))?;
    Ok(())
}

pub async fn get_shard(vault_path: &str, cid: &str, addr_str: &str) -> Result<(), String> {
    let passphrase = prompt_passphrase("Vault passphrase: ")?;
    let (keypair, _) = load_keys(vault_path, &passphrase)?;

    let libp2p_keypair = keypair.to_libp2p_keypair();

    let addr: libp2p::Multiaddr = addr_str.parse()
        .map_err(|e| format!("Invalid multiaddr: {}", e))?;

    let target_peer_id = addr.iter()
        .find_map(|p| if let libp2p::multiaddr::Protocol::P2p(pid) = p { Some(pid) } else { None })
        .ok_or("Multiaddr must contain /p2p/<peer_id>")?;

    println!("📡 Requesting shard {} from {}...", &cid[..16], target_peer_id);

    let config = NodeConfig {
        listen_addrs: vec!["/ip4/0.0.0.0/udp/0/quic-v1".to_string()],
        bootstrap_peers: vec![],
        mdns_enabled: false,
        relay_servers: vec![],
        telemetry_enabled: false,
        telemetry_dir: None,
    };

    let mut node = NexusNode::start(libp2p_keypair, config).await
        .map_err(|e| format!("Failed to start node: {}", e))?;

    tokio::time::sleep(Duration::from_millis(200)).await;

    // Dial the peer
    node.dial(addr).await.map_err(|e| format!("Dial failed: {}", e))?;

    // Wait for connection
    let start = std::time::Instant::now();
    let connected = tokio::time::timeout(Duration::from_secs(10), async {
        while let Some(event) = node.event_rx.recv().await {
            if let NodeEvent::PeerDiscovered(p) = event {
                if p == target_peer_id { return true; }
            }
        }
        false
    }).await.unwrap_or(false);

    if !connected {
        node.shutdown().await.ok();
        return Err("Failed to connect within 10s".into());
    }
    let connect_time = start.elapsed();
    println!("  ✅ Connected in {:?}", connect_time);

    // Send GetShard request
    use nexus_core::network::NodeCommand;
    node.command_tx.send(NodeCommand::RequestShard {
        peer: target_peer_id,
        cid: cid.to_string(),
    }).await.map_err(|e| format!("Send failed: {}", e))?;

    // Wait for shard response
    let fetch_start = std::time::Instant::now();
    let result = tokio::time::timeout(Duration::from_secs(15), async {
        while let Some(event) = node.event_rx.recv().await {
            if let NodeEvent::ShardReceived { peer: _, response } = event {
                return Some(response);
            }
        }
        None
    }).await;

    match result {
        Ok(Some(response)) => {
            let elapsed = fetch_start.elapsed();
            println!("  ✅ Response received in {:?}: {:?}", elapsed, response);
        }
        _ => {
            println!("  ❌ No shard response within 15s");
        }
    }

    node.shutdown().await.ok();
    Ok(())
}

pub async fn fetch(
    manifest_path: &str,
    peer_addr: &str,
    share_path: Option<&str>,
    output_path: Option<&str>,
    vault_path: &str,
) -> Result<(), String> {
    let passphrase = prompt_passphrase("Vault passphrase: ")?;
    let (keypair, pre_kp) = load_keys(vault_path, &passphrase)?;

    // Load manifest
    let manifest_json = fs::read_to_string(manifest_path)
        .map_err(|e| format!("Failed to read manifest: {}", e))?;
    let manifest: NexusManifest = serde_json::from_str(&manifest_json)
        .map_err(|e| format!("Invalid manifest: {}", e))?;

    let filename = manifest.shards.filename.clone().unwrap_or_else(|| "unnamed".into());
    println!("📡 Fetching: {} ({} shards)", filename, manifest.shards.shards.len());

    // Decrypt DEK (owner or shared)
    let dek = if let Some(share_file) = share_path {
        let share_json = fs::read_to_string(share_file)
            .map_err(|e| format!("Failed to read share grant: {}", e))?;
        let grant: ShareGrant = serde_json::from_str(&share_json)
            .map_err(|e| format!("Invalid share grant: {}", e))?;
        pre_kp.decrypt_dek_reencrypted(
            &manifest.encrypted_dek,
            &grant.cfrags,
            &manifest.owner_pre_pk,
            &grant.verifying_key,
        ).map_err(|e| format!("Delegated decryption failed: {}", e))?
    } else {
        pre_kp.decrypt_dek(&manifest.encrypted_dek)
            .map_err(|_| "Failed to decrypt DEK — use --share if you're not the owner".to_string())?
    };

    // Parse peer address
    let addr: libp2p::Multiaddr = peer_addr.parse()
        .map_err(|e| format!("Invalid multiaddr: {}", e))?;
    let target_peer = addr.iter()
        .find_map(|p| if let libp2p::multiaddr::Protocol::P2p(pid) = p { Some(pid) } else { None })
        .ok_or("Peer address must end with /p2p/<peer_id>")?;

    // Start ephemeral node (random port, no mDNS needed)
    let libp2p_keypair = keypair.to_libp2p_keypair();
    let config = NodeConfig {
        listen_addrs: vec!["/ip4/0.0.0.0/udp/0/quic-v1".to_string()],
        bootstrap_peers: vec![],
        mdns_enabled: false,
        relay_servers: vec![],
        telemetry_enabled: true,
        telemetry_dir: None,
    };

    let mut node = NexusNode::start(libp2p_keypair, config).await
        .map_err(|e| format!("Failed to start node: {}", e))?;

    // Wait for our own listen addr
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Dial the peer
    println!("  Connecting to {}...", target_peer);
    node.dial(addr).await.map_err(|e| format!("Dial failed: {}", e))?;

    // Wait for connection
    let connected = tokio::time::timeout(Duration::from_secs(10), async {
        while let Some(event) = node.event_rx.recv().await {
            if let NodeEvent::PeerDiscovered(p) = event {
                if p == target_peer { return true; }
            }
        }
        false
    }).await.unwrap_or(false);

    if !connected {
        node.shutdown().await.ok();
        return Err("Failed to connect to peer within 10s".into());
    }
    println!("  ✅ Connected");

    // Small delay for connection stabilization
    tokio::time::sleep(Duration::from_millis(300)).await;

    // Fetch each shard
    let mut shards = Vec::new();
    let total = manifest.shards.shards.len();

    for (i, cid_hex) in manifest.shards.shards.iter().enumerate() {
        // Check local store first
        let store = ShardStore::open(".nexus-store").ok();
        if let Some(ref store) = store {
            if let Ok(Some(shard)) = store.get(cid_hex) {
                println!("  [{}/{}] 📦 Local: {}...", i + 1, total, &cid_hex[..16.min(cid_hex.len())]);
                shards.push(shard);
                continue;
            }
        }

        // Request from peer
        node.request_shard(target_peer, cid_hex.clone()).await
            .map_err(|e| format!("Request failed: {}", e))?;

        // Wait for response
        let response = tokio::time::timeout(Duration::from_secs(30), async {
            while let Some(event) = node.event_rx.recv().await {
                if let NodeEvent::ShardReceived { response, .. } = event {
                    return Some(response);
                }
            }
            None
        }).await
            .map_err(|_| format!("Timeout fetching shard {}", cid_hex))?
            .ok_or_else(|| format!("Channel closed while fetching shard {}", cid_hex))?;

        match response {
            nexus_core::network::protocol::NexusResponse::Shard { cid, data } => {
                // Verify CID
                let computed = compute_cid(&data);
                let computed_hex = hex_encode(&computed);
                if computed_hex != *cid_hex {
                    node.shutdown().await.ok();
                    return Err(format!(
                        "CID mismatch for shard {}: expected {}, got {}",
                        i, cid_hex, computed_hex
                    ));
                }
                println!("  [{}/{}] ⬇️  Fetched: {}...", i + 1, total, &cid[..16.min(cid.len())]);
                let shard = nexus_core::storage::shard::Shard { cid: computed, data };

                // Store locally for future use
                if let Some(ref store) = store {
                    let _ = store.put(&shard);
                }
                shards.push(shard);
            }
            nexus_core::network::protocol::NexusResponse::ShardNotFound { cid } => {
                node.shutdown().await.ok();
                return Err(format!("Peer does not have shard: {}", cid));
            }
            other => {
                node.shutdown().await.ok();
                return Err(format!("Unexpected response: {:?}", other));
            }
        }
    }

    node.shutdown().await.ok();

    // Reassemble and decrypt
    println!("  Reassembling...");
    let encrypted_body = shard::reassemble(&manifest.shards, &shards)
        .ok_or("Failed to reassemble shards")?;

    let plaintext = decrypt_data(&encrypted_body, &dek)
        .map_err(|_| "Decryption failed — data corrupted".to_string())?;

    let out = output_path
        .map(|s| s.to_string())
        .unwrap_or(filename);

    fs::write(&out, &plaintext)
        .map_err(|e| format!("Failed to write output: {}", e))?;

    println!("  ✓ Decrypted: {} ({} bytes)", out, plaintext.len());
    Ok(())
}

pub async fn send(
    manifest_path: &str,
    peer_addr: &str,
    share_path: Option<&str>,
    vault_path: &str,
) -> Result<(), String> {
    let passphrase = prompt_passphrase("Vault passphrase: ")?;
    let (keypair, _pre_kp) = load_keys(vault_path, &passphrase)?;

    // Load manifest
    let manifest_json = fs::read_to_string(manifest_path)
        .map_err(|e| format!("Failed to read manifest: {}", e))?;
    let manifest: NexusManifest = serde_json::from_str(&manifest_json)
        .map_err(|e| format!("Invalid manifest: {}", e))?;

    let filename = manifest.shards.filename.clone().unwrap_or_else(|| "unnamed".into());
    println!("📤 Sending: {} ({} shards)", filename, manifest.shards.shards.len());

    // Load optional share grant
    let share_json = if let Some(path) = share_path {
        Some(fs::read_to_string(path)
            .map_err(|e| format!("Failed to read share grant: {}", e))?)
    } else {
        None
    };

    // Parse peer address
    let addr: libp2p::Multiaddr = peer_addr.parse()
        .map_err(|e| format!("Invalid multiaddr: {}", e))?;
    let target_peer = addr.iter()
        .find_map(|p| if let libp2p::multiaddr::Protocol::P2p(pid) = p { Some(pid) } else { None })
        .ok_or("Peer address must end with /p2p/<peer_id>")?;

    // Start ephemeral node
    let libp2p_keypair = keypair.to_libp2p_keypair();
    let config = NodeConfig {
        listen_addrs: vec!["/ip4/0.0.0.0/udp/0/quic-v1".to_string()],
        bootstrap_peers: vec![],
        mdns_enabled: false,
        relay_servers: vec![],
        telemetry_enabled: true,
        telemetry_dir: None,
    };

    let mut node = NexusNode::start(libp2p_keypair, config).await
        .map_err(|e| format!("Failed to start node: {}", e))?;

    tokio::time::sleep(Duration::from_millis(200)).await;

    // Dial the peer
    println!("  Connecting to {}...", target_peer);
    node.dial(addr).await.map_err(|e| format!("Dial failed: {}", e))?;

    // Wait for connection
    let connected = tokio::time::timeout(Duration::from_secs(10), async {
        while let Some(event) = node.event_rx.recv().await {
            if let NodeEvent::PeerDiscovered(p) = event {
                if p == target_peer { return true; }
            }
        }
        false
    }).await.unwrap_or(false);

    if !connected {
        node.shutdown().await.ok();
        return Err("Failed to connect to peer within 10s".into());
    }
    println!("  ✅ Connected");
    tokio::time::sleep(Duration::from_millis(300)).await;

    // Load shards from local store or manifest directory
    let store = ShardStore::open(".nexus-store").ok();
    let manifest_dir = Path::new(manifest_path).parent().unwrap_or(Path::new("."));
    let shards_dir = manifest_dir.join("shards");
    let total = manifest.shards.shards.len();

    for (i, cid_hex) in manifest.shards.shards.iter().enumerate() {
        // Try local store first, then shards directory
        let shard_data = if let Some(ref store) = store {
            store.get(cid_hex).ok().flatten().map(|s| s.data)
        } else {
            None
        }.or_else(|| {
            let shard_path = shards_dir.join(cid_hex);
            fs::read(&shard_path).ok()
        });

        let data = shard_data.ok_or_else(|| format!("Shard not found: {}", cid_hex))?;

        // Push shard to peer
        node.push_shard(target_peer, cid_hex.clone(), data).await
            .map_err(|e| format!("Push failed: {}", e))?;

        // Wait for acknowledgment
        let ack = tokio::time::timeout(Duration::from_secs(30), async {
            while let Some(event) = node.event_rx.recv().await {
                if let NodeEvent::ShardReceived { response, .. } = event {
                    return Some(response);
                }
            }
            None
        }).await;

        match ack {
            Ok(Some(nexus_core::network::protocol::NexusResponse::ShardAccepted { .. })) => {
                println!("  [{}/{}] ⬆️  Pushed: {}...", i + 1, total, &cid_hex[..16.min(cid_hex.len())]);
            }
            Ok(Some(nexus_core::network::protocol::NexusResponse::Error { message })) => {
                node.shutdown().await.ok();
                return Err(format!("Peer rejected shard: {}", message));
            }
            _ => {
                println!("  [{}/{}] ⚠️  No ack (continuing): {}...", i + 1, total, &cid_hex[..16.min(cid_hex.len())]);
            }
        }
    }

    // Push manifest + share grant
    node.push_manifest(target_peer, manifest_json, share_json).await
        .map_err(|e| format!("Manifest push failed: {}", e))?;

    // Wait for manifest ack
    let manifest_ack = tokio::time::timeout(Duration::from_secs(10), async {
        while let Some(event) = node.event_rx.recv().await {
            if let NodeEvent::ShardReceived { response, .. } = event {
                return Some(response);
            }
        }
        None
    }).await;

    match manifest_ack {
        Ok(Some(nexus_core::network::protocol::NexusResponse::ManifestAccepted)) => {
            println!("  📄 Manifest delivered");
        }
        _ => {
            println!("  ⚠️  Manifest ack not received (peer may still have it)");
        }
    }

    node.shutdown().await.ok();
    println!("  ✓ Send complete: {} ({} shards transferred)", filename, total);
    Ok(())
}

// --- Store Commands ---

pub fn store_stats(dir: &str) -> Result<(), String> {
    let store = ShardStore::open(dir)?;
    let stats = store.stats()?;
    println!("📦 Shard Store: {}", dir);
    println!("   Shards: {}", stats.shard_count);
    println!("   Total:  {} bytes ({:.2} MB)", stats.total_bytes, stats.total_bytes as f64 / 1_048_576.0);
    Ok(())
}

pub fn store_list(dir: &str) -> Result<(), String> {
    let store = ShardStore::open(dir)?;
    let cids = store.list()?;
    if cids.is_empty() {
        println!("Store is empty.");
    } else {
        println!("📦 {} shards in store:", cids.len());
        for cid in &cids {
            // Show first 16 chars of CID for readability
            let short = if cid.len() > 32 { &cid[..32] } else { cid };
            println!("   {}…", short);
        }
    }
    Ok(())
}

pub fn store_import(manifest_path: &str, from_dir: &str, store_dir: &str) -> Result<(), String> {
    let store = ShardStore::open(store_dir)?;

    let manifest_data = fs::read_to_string(manifest_path)
        .map_err(|e| format!("Failed to read manifest: {}", e))?;
    let manifest: NexusManifest = serde_json::from_str(&manifest_data)
        .map_err(|e| format!("Invalid manifest: {}", e))?;

    let mut imported = 0;
    let mut skipped = 0;

    for cid_hex in &manifest.shards.shards {
        if store.has(cid_hex) {
            skipped += 1;
            continue;
        }

        // Look for the shard file in from_dir
        // Shard files can be named: <cid_hex> or <cid_hex>.shard
        let shard_path = Path::new(from_dir).join(cid_hex);
        let shard_path_alt = Path::new(from_dir).join(format!("{}.shard", cid_hex));

        let data = if shard_path.exists() {
            fs::read(&shard_path).map_err(|e| format!("Read shard: {}", e))?
        } else if shard_path_alt.exists() {
            fs::read(&shard_path_alt).map_err(|e| format!("Read shard: {}", e))?
        } else {
            return Err(format!("Shard not found: {} (looked in {})", cid_hex, from_dir));
        };

        let cid = compute_cid(&data);
        let shard = Shard { cid, data };
        store.put(&shard)?;
        imported += 1;
    }

    println!("✓ Imported {} shards ({} already present)", imported, skipped);
    Ok(())
}

pub fn store_verify(dir: &str) -> Result<(), String> {
    let store = ShardStore::open(dir)?;
    let cids = store.list()?;

    let mut ok = 0;
    let mut corrupt = 0;

    for cid_hex in &cids {
        match store.get(cid_hex) {
            Ok(Some(_)) => ok += 1,
            Ok(None) => corrupt += 1,
            Err(_) => corrupt += 1,
        }
    }

    if corrupt == 0 {
        println!("✓ All {} shards verified OK", ok);
    } else {
        println!("⚠ {} OK, {} CORRUPT", ok, corrupt);
    }
    Ok(())
}

// --- Relay Server ---

pub async fn run_relay(
    vault_path: &str,
    port: u16,
    max_circuits: u32,
    max_reservations_per_peer: u32,
) -> Result<(), String> {
    let passphrase = prompt_passphrase("Vault passphrase: ")?;
    let (keypair, _pre_kp) = load_keys(vault_path, &passphrase)?;

    // Convert identity key to libp2p keypair
    let libp2p_keypair = keypair.to_libp2p_keypair();

    let config = RelayConfig {
        listen_addrs: vec![
            format!("/ip4/0.0.0.0/tcp/{}", port),
            format!("/ip4/0.0.0.0/udp/{}/quic-v1", port),
        ],
        max_reservations_per_peer,
        max_circuits,
        ..Default::default()
    };

    println!("🔁 NEXUS Relay Server");
    println!("  PeerId:  {}", libp2p_keypair.public().to_peer_id());
    println!("  Port:    {}", port);
    println!("  Max circuits: {}", max_circuits);
    println!("  Max reservations/peer: {}", max_reservations_per_peer);
    println!();

    let mut server = RelayServer::start(libp2p_keypair, config).await
        .map_err(|e| format!("Failed to start relay: {}", e))?;

    // Print events as they arrive
    loop {
        match server.event_rx.recv().await {
            Some(event) => match event {
                RelayServerEvent::Listening(addr) => {
                    println!("  📡 Listening: {}/p2p/{}", addr, server.peer_id);
                }
                RelayServerEvent::PeerConnected(peer) => {
                    println!("  ✅ Peer connected: {}", &peer[..16]);
                }
                RelayServerEvent::PeerDisconnected(peer) => {
                    println!("  ❌ Peer disconnected: {}", &peer[..16]);
                }
                RelayServerEvent::ReservationAccepted { peer } => {
                    println!("  📋 Reservation accepted: {}", &peer[..16]);
                }
                RelayServerEvent::ReservationExpired { peer } => {
                    println!("  ⏰ Reservation expired: {}", &peer[..16]);
                }
                RelayServerEvent::CircuitOpened { src, dst } => {
                    println!("  🔗 Circuit opened: {} → {}", &src[..16], &dst[..16]);
                }
                                RelayServerEvent::CircuitClosed { src, dst } => {
                    println!("  🔌 Circuit closed: {} → {}", &src[..16], &dst[..16]);
                }
                RelayServerEvent::PublicIpDetected(ip) => {
                    println!("  🌐 Public IP detected: {}", ip);
                }
            },
            None => break,
        }
    }

    Ok(())
}
