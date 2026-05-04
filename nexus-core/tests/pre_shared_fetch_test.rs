//! E2E test: Full PRE shared access over P2P network
//!
//! Proves the NEXUS thesis:
//! Alice encrypts → generates share grant for Bob → Bob fetches shards from Alice's node
//! → Bob decrypts using PRE re-encryption (never sees Alice's private key)

use nexus_core::crypto::{encrypt_data, decrypt_data, generate_dek};
use nexus_core::crypto::pre::{PreKeypair, PreSigner, reencrypt};
use nexus_core::network::{NexusNode, NodeConfig, NodeEvent, NodeCommand};
use nexus_core::network::protocol::NexusResponse;
use nexus_core::storage::{ShardStore, shard_data, reassemble, Shard, compute_cid};
use libp2p::identity::Keypair;
use std::time::Duration;
use tempfile::TempDir;
use tokio::time::timeout;

#[tokio::test]
async fn test_pre_shared_fetch_over_network() {
    // ==========================================================
    // SETUP: Alice and Bob each have identity + PRE keypairs
    // ==========================================================
    let alice_pre = PreKeypair::generate();
    let bob_pre = PreKeypair::generate();

    let alice_libp2p = Keypair::generate_ed25519();
    let bob_libp2p = Keypair::generate_ed25519();

    // Start both nodes
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

    // ==========================================================
    // ALICE: Encrypt a secret file
    // ==========================================================
    let secret_data = b"CLASSIFIED: Proxy re-encryption enables trustless access delegation. This is the NEXUS thesis.";
    let dek = generate_dek();
    let encrypted_body = encrypt_data(secret_data, &dek).unwrap();

    // Shard the encrypted data
    let (manifest, shards) = shard_data(&encrypted_body, 32);
    assert!(shards.len() > 1, "Should produce multiple shards");

    // Alice stores shards in her local store
    let alice_store_dir = TempDir::new().unwrap();
    let alice_store = ShardStore::open(alice_store_dir.path()).unwrap();
    for shard in &shards {
        alice_store.put(shard).unwrap();
    }

    // Alice encrypts the DEK with her PRE key (only she can decrypt)
    let encrypted_dek = alice_pre.encrypt_dek(&dek).unwrap();

    // ==========================================================
    // ALICE: Generate a share grant for Bob (PRE delegation)
    // ==========================================================
    let signer = PreSigner::new();
    let vk = signer.verifying_key();

    // Generate kfrags: threshold=1, shares=1 (simple 1:1 sharing)
    let kfrags = signer.generate_kfrags(&alice_pre, &bob_pre.public_key(), 1, 1).unwrap();

    // Alice (or any proxy) re-encrypts the capsule for Bob
    let cfrag = reencrypt(
        &encrypted_dek,
        &kfrags[0],
        &alice_pre.public_key(),
        &bob_pre.public_key(),
        &vk,
    ).unwrap();

    // The share grant (this is what Bob receives out-of-band)
    let cfrags = vec![cfrag];
    let alice_pk = alice_pre.public_key();
    let verifying_key = vk;

    // ==========================================================
    // BOB: Fetch shards from Alice's node over P2P
    // ==========================================================
    let mut fetched_shards: Vec<Shard> = Vec::new();

    for cid_hex in &manifest.shards {
        // Bob requests the shard
        node_b.request_shard(peer_a, cid_hex.clone()).await.unwrap();

        // Alice handles the request (auto-serve from store)
        let serve_result = timeout(Duration::from_secs(5), async {
            while let Some(event) = node_a.event_rx.recv().await {
                if let NodeEvent::ShardRequested { cid, channel, .. } = event {
                    if cid == *cid_hex {
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
        assert!(serve_result.is_ok() && serve_result.unwrap(), "Alice must serve shard");

        // Bob receives the shard response
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
                // Verify CID integrity
                let computed_cid = compute_cid(&data);
                let computed_hex: String = computed_cid.iter().map(|b| format!("{:02x}", b)).collect();
                assert_eq!(&computed_hex, cid_hex, "CID mismatch — data corrupted in transit");
                fetched_shards.push(Shard { cid: computed_cid, data });
            }
            _ => panic!("Expected Shard response from Alice"),
        }
    }

    // ==========================================================
    // BOB: Reassemble + Decrypt using PRE share grant
    // ==========================================================
    // Reassemble encrypted body from fetched shards
    let reassembled = reassemble(&manifest, &fetched_shards).unwrap();
    assert_eq!(reassembled, encrypted_body, "Reassembly must produce original encrypted blob");

    // Bob decrypts the DEK using re-encrypted cfrags (PRE!)
    // Bob NEVER had Alice's private key — only the cfrags + his own key
    let decrypted_dek = bob_pre.decrypt_dek_reencrypted(
        &encrypted_dek,
        &cfrags,
        &alice_pk,
        &verifying_key,
    ).unwrap();

    assert_eq!(decrypted_dek, dek, "Bob must recover the same DEK via PRE");

    // Decrypt the file body
    let plaintext = decrypt_data(&reassembled, &decrypted_dek).unwrap();
    assert_eq!(plaintext.as_slice(), secret_data);

    println!("✅ NEXUS THESIS PROVEN:");
    println!("   Alice encrypted {} bytes → {} shards", secret_data.len(), shards.len());
    println!("   Bob fetched all shards over P2P network");
    println!("   Bob decrypted using PRE (never saw Alice's private key)");
    println!("   Recovered plaintext matches original ✓");

    node_a.shutdown().await.unwrap();
    node_b.shutdown().await.unwrap();
}
