//! E2E test: Push-based file transfer (nexus send)
//!
//! Alice pushes shards + manifest to Bob's node

use nexus_core::crypto::{encrypt_data, decrypt_data, generate_dek};
use nexus_core::crypto::pre::PreKeypair;
use nexus_core::manifest::NexusManifest;
use nexus_core::network::{NexusNode, NodeConfig, NodeEvent, NodeCommand};
use nexus_core::network::protocol::NexusResponse;
use nexus_core::storage::{ShardStore, shard_data, reassemble, compute_cid, Shard};
use libp2p::identity::Keypair;
use std::time::Duration;
use tempfile::TempDir;
use tokio::time::timeout;

#[tokio::test]
async fn test_send_shards_and_manifest_to_peer() {
    // === Setup: Alice and Bob nodes ===
    let alice_libp2p = Keypair::generate_ed25519();
    let bob_libp2p = Keypair::generate_ed25519();

    let config_a = NodeConfig::default();
    let mut node_a = NexusNode::start(alice_libp2p, config_a).await.unwrap();
    let peer_a = *node_a.peer_id();

    let config_b = NodeConfig::default();
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
    assert!(discovery.is_ok(), "Nodes must discover each other");
    tokio::time::sleep(Duration::from_millis(500)).await;

    // === Alice: Encrypt a file ===
    let secret_data = b"PUSH TEST: Alice sends this file directly to Bob's node.";
    let dek = generate_dek();
    let encrypted_body = encrypt_data(secret_data, &dek).unwrap();

    let (manifest_data, shards) = shard_data(&encrypted_body, 32);
    assert!(shards.len() > 1);

    let alice_pre = PreKeypair::generate();
    let encrypted_dek = alice_pre.encrypt_dek(&dek).unwrap();

    let manifest = NexusManifest {
        owner: "did:nexus:alice_test".to_string(),
        owner_pre_pk: alice_pre.public_key(),
        shards: manifest_data.clone(),
        encrypted_dek: encrypted_dek.clone(),
    };
    let manifest_json = serde_json::to_string(&manifest).unwrap();

    // Bob's store for receiving pushed shards
    let bob_store_dir = TempDir::new().unwrap();
    let bob_store = ShardStore::open(bob_store_dir.path()).unwrap();

    // === Alice: Push each shard to Bob ===
    for cid_hex in &manifest_data.shards {
        let shard = shards.iter().find(|s| {
            let hex: String = s.cid.iter().map(|b| format!("{:02x}", b)).collect();
            hex == *cid_hex
        }).unwrap();

        // Alice pushes shard to Bob
        node_a.push_shard(peer_b, cid_hex.clone(), shard.data.clone()).await.unwrap();

        // Bob receives the push, stores it, and acks
        let push_result = timeout(Duration::from_secs(5), async {
            while let Some(event) = node_b.event_rx.recv().await {
                if let NodeEvent::ShardPushed { cid, data, channel, .. } = event {
                    if cid == *cid_hex {
                        // Store it
                        let computed = compute_cid(&data);
                        bob_store.put(&Shard { cid: computed.to_vec(), data }).unwrap();
                        // Ack
                        let _ = node_b.command_tx.send(NodeCommand::RespondShard {
                            channel,
                            response: NexusResponse::ShardAccepted { cid },
                        }).await;
                        return true;
                    }
                }
            }
            false
        }).await;
        assert!(push_result.is_ok() && push_result.unwrap());

        // Alice waits for ack
        let ack = timeout(Duration::from_secs(5), async {
            while let Some(event) = node_a.event_rx.recv().await {
                if let NodeEvent::ShardReceived { response, .. } = event {
                    return Some(response);
                }
            }
            None
        }).await;
        assert!(matches!(ack, Ok(Some(NexusResponse::ShardAccepted { .. }))));
    }

    // === Alice: Push manifest to Bob ===
    node_a.push_manifest(peer_b, manifest_json.clone(), None).await.unwrap();

    let manifest_push = timeout(Duration::from_secs(5), async {
        while let Some(event) = node_b.event_rx.recv().await {
            if let NodeEvent::ManifestPushed { manifest_json: mj, channel, .. } = event {
                // Ack
                let _ = node_b.command_tx.send(NodeCommand::RespondShard {
                    channel,
                    response: NexusResponse::ManifestAccepted,
                }).await;
                return Some(mj);
            }
        }
        None
    }).await;
    assert!(manifest_push.is_ok());
    let received_manifest_json = manifest_push.unwrap().unwrap();

    // === Bob: Verify received data ===
    let received_manifest: NexusManifest = serde_json::from_str(&received_manifest_json).unwrap();
    assert_eq!(received_manifest.owner, "did:nexus:alice_test");
    assert_eq!(received_manifest.shards.shards.len(), manifest_data.shards.len());

    // Bob has all shards in his store
    let mut fetched_shards = Vec::new();
    for cid_hex in &received_manifest.shards.shards {
        let shard = bob_store.get(cid_hex).unwrap().unwrap();
        fetched_shards.push(shard);
    }

    // Reassemble and decrypt (as owner for this test)
    let reassembled = reassemble(&received_manifest.shards, &fetched_shards).unwrap();
    let decrypted = decrypt_data(&reassembled, &dek).unwrap();
    assert_eq!(decrypted.as_slice(), secret_data);

    println!("✅ Push transfer test passed:");
    println!("   Alice pushed {} shards + manifest to Bob", shards.len());
    println!("   Bob stored, reassembled, and decrypted successfully");

    node_a.shutdown().await.unwrap();
    node_b.shutdown().await.unwrap();
}
