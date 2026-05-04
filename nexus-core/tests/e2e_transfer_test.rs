//! Integration test: End-to-end network shard transfer
//!
//! Alice encrypts a file → stores shards locally → Bob fetches shards over P2P

use nexus_core::network::{NexusNode, NodeConfig, NodeEvent, NodeCommand};
use nexus_core::network::protocol::NexusResponse;
use nexus_core::storage::{ShardStore, shard_data, reassemble};
use nexus_core::crypto::{encrypt_data, decrypt_data, generate_dek};
use libp2p::identity::Keypair;
use std::time::Duration;
use tempfile::TempDir;
use tokio::time::timeout;

#[tokio::test]
async fn test_encrypt_store_fetch_decrypt_over_network() {
    // === Setup: Two nodes ===
    let keypair_a = Keypair::generate_ed25519();
    let config_a = NodeConfig::default();
    let mut node_a = NexusNode::start(keypair_a, config_a).await.unwrap();
    let peer_a = *node_a.peer_id();

    let keypair_b = Keypair::generate_ed25519();
    let config_b = NodeConfig::default();
    let mut node_b = NexusNode::start(keypair_b, config_b).await.unwrap();
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

    // === Alice: Encrypt a file and store shards ===
    let original_data = b"TOP SECRET: NEXUS network transfer test data. This payload proves shard transfer works!";
    let dek = generate_dek();
    let encrypted = encrypt_data(original_data, &dek).unwrap();

    // Shard the encrypted data
    let (manifest, shards) = shard_data(&encrypted, 32); // Small shards for testing
    assert!(shards.len() > 1, "Should produce multiple shards");

    // Alice stores shards locally
    let alice_store_dir = TempDir::new().unwrap();
    let alice_store = ShardStore::open(alice_store_dir.path()).unwrap();
    for shard in &shards {
        alice_store.put(shard).unwrap();
    }

    // === Bob: Fetch shards from Alice over the network ===
    let mut fetched_shards = Vec::new();
    for cid_hex in &manifest.shards {
        // Bob requests the shard from Alice
        node_b.request_shard(peer_a, cid_hex.clone()).await.unwrap();

        // Alice receives the request, looks up shard, responds
        let fetch_result = timeout(Duration::from_secs(5), async {
            while let Some(event) = node_a.event_rx.recv().await {
                if let NodeEvent::ShardRequested { cid, channel, .. } = event {
                    if cid == *cid_hex {
                        // Look up in Alice's store
                        let shard = alice_store.get(&cid).unwrap().unwrap();
                        let _ = node_a.command_tx.send(NodeCommand::RespondShard {
                            channel,
                            response: NexusResponse::Shard {
                                cid: cid.clone(),
                                data: shard.data,
                            },
                        }).await;
                        return true;
                    }
                }
            }
            false
        }).await;
        assert!(fetch_result.is_ok() && fetch_result.unwrap());

        // Bob waits for the response
        let shard_response = timeout(Duration::from_secs(5), async {
            while let Some(event) = node_b.event_rx.recv().await {
                if let NodeEvent::ShardReceived { response, .. } = event {
                    return Some(response);
                }
            }
            None
        }).await;
        
        match shard_response {
            Ok(Some(NexusResponse::Shard { data, .. })) => {
                let cid = nexus_core::storage::compute_cid(&data);
                fetched_shards.push(nexus_core::storage::Shard { cid, data });
            }
            _ => panic!("Expected shard response for CID {}", cid_hex),
        }
    }

    // === Bob: Reassemble and decrypt ===
    let reassembled = reassemble(&manifest, &fetched_shards).unwrap();
    let decrypted = decrypt_data(&reassembled, &dek).unwrap();
    assert_eq!(decrypted, original_data);

    println!("✅ End-to-end network transfer: encrypt → shard → store → fetch → reassemble → decrypt");
    println!("   Original: {} bytes, Shards: {}, Encrypted: {} bytes", original_data.len(), shards.len(), encrypted.len());

    node_a.shutdown().await.unwrap();
    node_b.shutdown().await.unwrap();
}
