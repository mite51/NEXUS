//! Integration test: End-to-end pull-only file sharing
//!
//! Alice encrypts a file → shares with Bob via PRE → Bob pulls entire asset from Alice's node
//! This tests the full pull-only sharing flow including:
//! - Encryption + sharding + storage
//! - PRE share grant (rfrag) generation
//! - PullAsset protocol request
//! - Asset serving (rfrag + manifest + shards)
//! - PRE decryption + reassembly

use nexus_core::crypto::{encrypt_data, decrypt_data, generate_dek};
use nexus_core::crypto::pre::{PreKeypair, PreSigner, reencrypt};
use nexus_core::identity::IdentityKeypair;
use nexus_core::manifest::{NexusManifest, ShareGrant};
use nexus_core::network::{NexusNode, NodeConfig, NodeEvent, NodeCommand};
use nexus_core::network::protocol::NexusResponse;
use nexus_core::storage::{ShardStore, AssetStore, compute_cid};
use nexus_core::storage::shard::{self, Shard};
use std::time::Duration;
use tempfile::TempDir;
use tokio::time::timeout;

#[tokio::test]
async fn test_pull_asset_e2e() {
    // ============================================================
    // Setup: Two identities (Alice = owner, Bob = recipient)
    // ============================================================
    let alice_identity = IdentityKeypair::generate();
    let alice_pre = PreKeypair::generate();
    let alice_did = alice_identity.did();
    let alice_libp2p = alice_identity.to_libp2p_keypair();

    let bob_identity = IdentityKeypair::generate();
    let bob_pre = PreKeypair::generate();
    let bob_did = bob_identity.did();
    let bob_libp2p = bob_identity.to_libp2p_keypair();

    // Setup temp dirs for stores
    let alice_dir = TempDir::new().unwrap();
    let bob_dir = TempDir::new().unwrap();

    let alice_store = AssetStore::open(alice_dir.path()).unwrap();
    let alice_shard_store = ShardStore::open(alice_dir.path()).unwrap();

    // ============================================================
    // Step 1: Alice encrypts a file
    // ============================================================
    let original_data = b"This is a secret document that Alice will share with Bob via the pull-only protocol.";
    let dek = generate_dek();
    let encrypted_body = encrypt_data(original_data, &dek).unwrap();

    // Shard the encrypted data
    let (shard_manifest, shards) = shard::shard_data(&encrypted_body, 32);
    assert!(shards.len() > 1, "Should produce multiple shards");

    // Store shards
    for shard in &shards {
        alice_shard_store.put(shard).unwrap();
    }

    // Encrypt DEK with Alice's PRE key
    let encrypted_dek = alice_pre.encrypt_dek(&dek).expect("DEK encryption should succeed");

    // Build manifest
    let manifest = NexusManifest {
        owner: alice_did.clone(),
        owner_pre_pk: alice_pre.public_key(),
        encrypted_dek,
        shards: shard_manifest,
    };
    let manifest_bytes = serde_json::to_vec(&manifest).unwrap();
    let asset_id = alice_store.put_manifest(&manifest_bytes).unwrap();

    println!("Asset ID: {}", asset_id);
    println!("Manifest: {} bytes", manifest_bytes.len());
    println!("Shards: {}", shards.len());

    // ============================================================
    // Step 2: Alice shares with Bob (generates rfrag)
    // ============================================================
    let signer = PreSigner::new();
    let vk = signer.verifying_key();
    let kfrags = signer.generate_kfrags(&alice_pre, &bob_pre.public_key(), 1, 1).unwrap();

    // Re-encrypt (Alice acts as own proxy for v1)
    let cfrags: Vec<_> = kfrags.iter()
        .map(|kf| reencrypt(&manifest.encrypted_dek, kf, &alice_pre.public_key(), &bob_pre.public_key(), &vk))
        .collect::<Result<Vec<_>, _>>()
        .unwrap();

    let grant = ShareGrant {
        recipient: bob_did.clone(),
        recipient_pre_pk: bob_pre.public_key(),
        cfrags: cfrags.clone(),
        verifying_key: vk.clone(),
        manifest_ref: "test".to_string(),
    };

    let rfrag_bytes = serde_json::to_vec(&grant).unwrap();
    alice_store.put_rfrag(&asset_id, &bob_did, &rfrag_bytes).unwrap();

    println!("Rfrag stored for Bob: {} bytes", rfrag_bytes.len());

    // Verify share link
    let link = AssetStore::share_link(&alice_identity.peer_id().to_string(), &asset_id);
    let (parsed_peer, parsed_asset) = AssetStore::parse_share_link(&link).unwrap();
    assert_eq!(parsed_peer, alice_identity.peer_id().to_string());
    assert_eq!(parsed_asset, asset_id);
    println!("Share link: {}", link);

    // ============================================================
    // Step 3: Start P2P nodes
    // ============================================================
    let config_a = NodeConfig::default();
    let config_b = NodeConfig::default();

    let mut node_a = NexusNode::start(alice_libp2p, config_a).await.unwrap();
    let peer_a = *node_a.peer_id();
    let mut node_b = NexusNode::start(bob_libp2p, config_b).await.unwrap();
    let peer_b = *node_b.peer_id();

    // Wait for mutual discovery
    let discovery = timeout(Duration::from_secs(10), async {
        let mut a_found_b = false;
        let mut b_found_a = false;
        loop {
            tokio::select! {
                Some(event) = node_a.event_rx.recv() => {
                    if let NodeEvent::PeerDiscovered(p) = event {
                        if p == peer_b { a_found_b = true; }
                    }
                }
                Some(event) = node_b.event_rx.recv() => {
                    if let NodeEvent::PeerDiscovered(p) = event {
                        if p == peer_a { b_found_a = true; }
                    }
                }
            }
            if a_found_b && b_found_a { return; }
        }
    }).await;
    assert!(discovery.is_ok(), "Nodes must discover each other via mDNS");
    tokio::time::sleep(Duration::from_millis(500)).await;

    // ============================================================
    // Step 4: Bob sends PullAsset request
    // ============================================================
    let signature = bob_identity.sign(asset_id.as_bytes());

    node_b.pull_asset(
        peer_a,
        asset_id.clone(),
        bob_did.clone(),
        signature,
    ).await.unwrap();

    println!("Bob sent PullAsset request");

    // ============================================================
    // Step 5: Alice receives request, serves asset
    // ============================================================
    // Handle events from both nodes concurrently
    let result = timeout(Duration::from_secs(10), async {
        let mut asset_response: Option<NexusResponse> = None;
        loop {
            tokio::select! {
                Some(event) = node_a.event_rx.recv() => {
                    match event {
                        NodeEvent::PullAssetRequested { peer, asset_id: req_asset_id, requester_did: req_did, signature: _, channel } => {
                            println!("Alice received pull request from {} for asset {}", peer, &req_asset_id[..16]);

                            // Look up rfrag
                            let rfrag = alice_store.get_rfrag(&req_asset_id, &req_did).unwrap();
                            assert!(rfrag.is_some(), "Bob's rfrag should exist");
                            let rfrag = rfrag.unwrap();

                            // Get manifest
                            let manifest = alice_store.get_manifest(&req_asset_id).unwrap().unwrap();

                            // Get shards
                            let parsed_manifest: NexusManifest = serde_json::from_slice(&manifest).unwrap();
                            let mut shard_data = Vec::new();
                            for cid in &parsed_manifest.shards.shards {
                                let s = alice_shard_store.get(cid).unwrap().unwrap();
                                shard_data.push(s.data);
                            }

                            // Send response
                            let response = NexusResponse::Asset {
                                asset_id: req_asset_id,
                                rfrag,
                                manifest,
                                shards: shard_data,
                            };
                            let _ = node_a.command_tx.send(NodeCommand::RespondShard {
                                channel,
                                response,
                            }).await;
                            println!("Alice served the asset");
                        }
                        other => {
                            println!("  [Alice event] {:?}", std::mem::discriminant(&other));
                        }
                    }
                }
                Some(event) = node_b.event_rx.recv() => {
                    match event {
                        NodeEvent::ShardReceived { response, .. } => {
                            asset_response = Some(response);
                            break;
                        }
                        other => {
                            println!("  [Bob event] {:?}", std::mem::discriminant(&other));
                        }
                    }
                }
            }
        }
        asset_response.unwrap()
    }).await.expect("Should receive asset within timeout");

    // ============================================================
    // Step 6: Bob decrypts the received asset
    // ============================================================
    match result {
        NexusResponse::Asset { asset_id: resp_id, rfrag, manifest: manifest_bytes, shards: received_shards } => {
            println!("Bob received asset: {} bytes manifest, {} shards, {} bytes rfrag",
                manifest_bytes.len(), received_shards.len(), rfrag.len());

            assert_eq!(resp_id, asset_id);

            // Parse rfrag (ShareGrant)
            let grant: ShareGrant = serde_json::from_slice(&rfrag).unwrap();
            assert_eq!(grant.recipient, bob_did);

            // Parse manifest
            let recv_manifest: NexusManifest = serde_json::from_slice(&manifest_bytes).unwrap();
            assert_eq!(recv_manifest.shards.shards.len(), received_shards.len());

            // Verify shard CIDs
            for (i, (expected_cid, shard_data)) in recv_manifest.shards.shards.iter().zip(&received_shards).enumerate() {
                let computed = compute_cid(shard_data);
                let computed_hex: String = computed.iter().map(|b| format!("{:02x}", b)).collect();
                assert_eq!(computed_hex, *expected_cid, "CID mismatch at shard {}", i);
            }
            println!("All shard CIDs verified ✓");

            // Decrypt DEK using PRE
            let decrypted_dek = bob_pre.decrypt_dek_reencrypted(
                &recv_manifest.encrypted_dek,
                &grant.cfrags,
                &recv_manifest.owner_pre_pk,
                &grant.verifying_key,
            ).expect("PRE decryption should succeed");

            // Build Shard objects and reassemble
            let shard_objs: Vec<Shard> = recv_manifest.shards.shards.iter()
                .zip(received_shards)
                .map(|(cid_hex, data)| {
                    let cid_bytes: Vec<u8> = (0..cid_hex.len())
                        .step_by(2)
                        .map(|i| u8::from_str_radix(&cid_hex[i..i+2], 16).unwrap())
                        .collect();
                    Shard { cid: cid_bytes, data }
                })
                .collect();

            let encrypted_body = shard::reassemble(&recv_manifest.shards, &shard_objs)
                .expect("Reassembly should succeed");

            let plaintext = decrypt_data(&encrypted_body, &decrypted_dek)
                .expect("Decryption should succeed");

            assert_eq!(plaintext, original_data);
            println!("\n🎉 E2E PULL TEST PASSED: {} bytes decrypted correctly", plaintext.len());
        }
        NexusResponse::AssetDenied { reason, .. } => {
            panic!("Asset denied: {}", reason);
        }
        other => {
            panic!("Unexpected response: {:?}", other);
        }
    }

    // Cleanup
    node_a.shutdown().await.ok();
    node_b.shutdown().await.ok();
}
