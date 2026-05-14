//! Integration test: Relay reservation and circuit relay pull
//!
//! Tests that:
//! 1. A relay server accepts reservations from nodes
//! 2. Two NATted nodes can exchange data through the relay
//! 3. Full pull-asset flow works over a relayed connection

use nexus_core::crypto::{encrypt_data, generate_dek};
use nexus_core::crypto::pre::{PreKeypair, PreSigner, reencrypt, public_pre_keypair, PUBLIC_DID};
use nexus_core::identity::IdentityKeypair;
use nexus_core::manifest::{NexusManifest, ShareGrant};
use nexus_core::network::{NexusNode, NodeConfig, NodeEvent, NodeCommand, RelayServer, RelayConfig, RelayServerEvent};
use nexus_core::network::protocol::NexusResponse;
use nexus_core::storage::{ShardStore, AssetStore, compute_cid};
use nexus_core::storage::shard::{self, Shard};
use std::time::Duration;
use tempfile::TempDir;
use tokio::time::timeout;

/// Test that a relay server accepts reservations from a connecting node
#[tokio::test]
async fn test_relay_reservation() {
    // Start relay server
    let relay_kp = libp2p::identity::Keypair::generate_ed25519();
    let relay_peer_id = relay_kp.public().to_peer_id();

    let relay_config = RelayConfig {
        listen_addrs: vec!["/ip4/127.0.0.1/tcp/0".to_string()],
        public_address: Some("127.0.0.1".to_string()),
        ..Default::default()
    };

    let mut relay = RelayServer::start(relay_kp, relay_config).await
        .expect("Relay should start");

    // Wait for relay to start listening
    let relay_addr = timeout(Duration::from_secs(5), async {
        loop {
            match relay.event_rx.recv().await {
                Some(RelayServerEvent::Listening(addr)) => return addr,
                Some(_) => continue,
                None => panic!("Relay event channel closed"),
            }
        }
    }).await.expect("Relay should start listening");

    println!("Relay listening: {}", relay_addr);

    // Start a node that connects to the relay
    let node_kp = IdentityKeypair::generate();
    let node_libp2p = node_kp.to_libp2p_keypair();

    let relay_multiaddr = format!("{}/p2p/{}", relay_addr, relay_peer_id);
    println!("Node connecting to relay: {}", relay_multiaddr);

    let node_config = NodeConfig {
        listen_addrs: vec!["/ip4/127.0.0.1/tcp/0".to_string()],
        bootstrap_peers: vec![],
        relay_servers: vec![relay_multiaddr],
        mdns_enabled: false,
        telemetry_enabled: false,
        telemetry_dir: None,
    };

    let _node = NexusNode::start(node_libp2p, node_config).await
        .expect("Node should start");

    // Wait for relay to accept reservation
    let reservation_accepted = timeout(Duration::from_secs(10), async {
        loop {
            match relay.event_rx.recv().await {
                Some(RelayServerEvent::ReservationAccepted { peer }) => {
                    println!("Relay accepted reservation from: {}", &peer[..16]);
                    return true;
                }
                Some(RelayServerEvent::PeerConnected(peer)) => {
                    println!("Relay: peer connected: {}", &peer[..16]);
                }
                Some(other) => {
                    println!("Relay event: {:?}", other);
                }
                None => return false,
            }
        }
    }).await;

    assert!(reservation_accepted.is_ok(), "Reservation should be accepted within timeout");
    assert!(reservation_accepted.unwrap(), "Reservation should succeed");
    println!("\n✅ Relay reservation test passed!");
    relay.shutdown().await;
}

/// Test full pull-asset flow through a relay circuit (no mDNS, relay-only connectivity)
#[tokio::test]
async fn test_pull_through_relay() {
    // ============================================================
    // Setup relay
    // ============================================================
    let relay_kp = libp2p::identity::Keypair::generate_ed25519();
    let relay_peer_id = relay_kp.public().to_peer_id();

    let relay_config = RelayConfig {
        listen_addrs: vec!["/ip4/127.0.0.1/tcp/0".to_string()],
        public_address: Some("127.0.0.1".to_string()),
        ..Default::default()
    };

    let mut relay = RelayServer::start(relay_kp, relay_config).await
        .expect("Relay should start");

    let relay_addr = timeout(Duration::from_secs(5), async {
        loop {
            match relay.event_rx.recv().await {
                Some(RelayServerEvent::Listening(addr)) => return addr,
                Some(_) => continue,
                None => panic!("Relay channel closed"),
            }
        }
    }).await.expect("Relay should listen");

    let relay_multiaddr = format!("{}/p2p/{}", relay_addr, relay_peer_id);
    println!("Relay: {}", relay_multiaddr);

    // ============================================================
    // Setup Alice (owner) with asset store
    // ============================================================
    let alice_identity = IdentityKeypair::generate();
    let alice_pre = PreKeypair::generate();
    let alice_did = alice_identity.did();
    let alice_libp2p = alice_identity.to_libp2p_keypair();
    let alice_peer_id = alice_libp2p.public().to_peer_id();

    let alice_dir = TempDir::new().unwrap();
    let alice_store = AssetStore::open(alice_dir.path()).unwrap();
    let alice_shard_store = ShardStore::open(alice_dir.path()).unwrap();

    // ============================================================
    // Step 1: Alice encrypts + stores a file, marks public
    // ============================================================
    let original_data = b"Relay test: secret data transmitted through relay circuit";
    let dek = generate_dek();
    let encrypted_body = encrypt_data(original_data, &dek).unwrap();
    let (shard_manifest, shards) = shard::shard_data(&encrypted_body, 1024);

    for shard in &shards {
        alice_shard_store.put(shard).unwrap();
    }

    let encrypted_dek = alice_pre.encrypt_dek(&dek).unwrap();
    let manifest = NexusManifest {
        owner: alice_did.clone(),
        owner_pre_pk: alice_pre.public_key(),
        encrypted_dek,
        shards: shard_manifest,
    };
    let manifest_bytes = serde_json::to_vec(&manifest).unwrap();
    let asset_id = alice_store.put_manifest(&manifest_bytes).unwrap();

    // Mark public
    let public_kp = public_pre_keypair();
    let signer = PreSigner::new();
    let vk = signer.verifying_key();
    let kfrags = signer.generate_kfrags(&alice_pre, &public_kp.public_key(), 1, 1).unwrap();
    let cfrag = reencrypt(&manifest.encrypted_dek, &kfrags[0], &alice_pre.public_key(), &public_kp.public_key(), &vk).unwrap();
    let grant = ShareGrant {
        recipient: PUBLIC_DID.to_string(),
        recipient_pre_pk: public_kp.public_key(),
        cfrags: vec![cfrag],
        verifying_key: vk,
        manifest_ref: String::new(),
    };
    let rfrag_bytes = serde_json::to_vec(&grant).unwrap();
    alice_store.put_rfrag(&asset_id, PUBLIC_DID, &rfrag_bytes).unwrap();
    alice_store.set_public(&asset_id, true).unwrap();
    println!("Asset {} stored + marked public", &asset_id[..16]);

    // ============================================================
    // Step 2: Start both nodes with relay (mDNS DISABLED — relay only)
    // ============================================================
    let alice_config = NodeConfig {
        listen_addrs: vec!["/ip4/127.0.0.1/tcp/0".to_string()],
        bootstrap_peers: vec![],
        relay_servers: vec![relay_multiaddr.clone()],
        mdns_enabled: false,
        telemetry_enabled: false,
        telemetry_dir: None,
    };

    let bob_identity = IdentityKeypair::generate();
    let bob_did = bob_identity.did();
    let bob_libp2p = bob_identity.to_libp2p_keypair();

    let bob_config = NodeConfig {
        listen_addrs: vec!["/ip4/127.0.0.1/tcp/0".to_string()],
        bootstrap_peers: vec![],
        relay_servers: vec![relay_multiaddr.clone()],
        mdns_enabled: false,
        telemetry_enabled: false,
        telemetry_dir: None,
    };

    let mut node_a = NexusNode::start(alice_libp2p, alice_config).await.unwrap();
    let mut node_b = NexusNode::start(bob_libp2p, bob_config).await.unwrap();

    // Wait for Alice to get her relay reservation
    let alice_reserved = timeout(Duration::from_secs(10), async {
        loop {
            match node_a.event_rx.recv().await {
                Some(NodeEvent::RelayReserved { .. }) => return true,
                Some(_) => continue,
                None => return false,
            }
        }
    }).await;
    assert!(alice_reserved.is_ok() && alice_reserved.unwrap(), "Alice must get relay reservation");
    println!("Alice has relay reservation ✓");

    // Drain Bob's startup events
    tokio::time::sleep(Duration::from_millis(500)).await;
    while let Ok(Some(_)) = timeout(Duration::from_millis(50), node_b.event_rx.recv()).await {}

    // ============================================================
    // Step 3: Bob dials Alice through the relay circuit
    // ============================================================
    let circuit_addr: libp2p::Multiaddr = format!(
        "{}/p2p/{}/p2p-circuit/p2p/{}",
        relay_addr, relay_peer_id, alice_peer_id
    ).parse().unwrap();
    println!("Bob dialing Alice via circuit: {}", circuit_addr);
    node_b.dial(circuit_addr).await.unwrap();

    // Wait for Bob to connect to Alice
    let connected = timeout(Duration::from_secs(10), async {
        loop {
            tokio::time::sleep(Duration::from_millis(200)).await;
            let peers = node_b.connected_peers().await.unwrap_or_default();
            if peers.iter().any(|p| *p == alice_peer_id) {
                return true;
            }
        }
    }).await;
    assert!(connected.is_ok() && connected.unwrap(), "Bob must connect to Alice via relay");
    println!("Bob connected to Alice via relay ✓");

    // Drain events from the connection establishment
    while let Ok(Some(_)) = timeout(Duration::from_millis(50), node_a.event_rx.recv()).await {}
    while let Ok(Some(_)) = timeout(Duration::from_millis(50), node_b.event_rx.recv()).await {}

    // ============================================================
    // Step 4: Bob pulls asset from Alice
    // ============================================================
    let signature = bob_identity.sign(asset_id.as_bytes());
    node_b.pull_asset(alice_peer_id, asset_id.clone(), bob_did.clone(), signature).await.unwrap();
    println!("Bob sent pull request");

    let result = timeout(Duration::from_secs(15), async {
        let mut response: Option<NexusResponse> = None;
        loop {
            tokio::select! {
                Some(event) = node_a.event_rx.recv() => {
                    if let NodeEvent::PullAssetRequested { asset_id: req_id, requester_did: req_did, channel, .. } = event {
                        let is_public = alice_store.is_public(&req_id).unwrap_or(false);
                        let rfrag = if is_public {
                            alice_store.get_rfrag(&req_id, PUBLIC_DID).unwrap()
                        } else {
                            alice_store.get_rfrag(&req_id, &req_did).unwrap()
                        };
                        let manifest = alice_store.get_manifest(&req_id).unwrap().unwrap();
                        let parsed: NexusManifest = serde_json::from_slice(&manifest).unwrap();
                        let mut shard_data = Vec::new();
                        for cid in &parsed.shards.shards {
                            shard_data.push(alice_shard_store.get(cid).unwrap().unwrap().data);
                        }
                        let resp = NexusResponse::Asset {
                            asset_id: req_id,
                            rfrag: rfrag.unwrap(),
                            manifest,
                            shards: shard_data,
                        };
                        let _ = node_a.command_tx.send(NodeCommand::RespondShard { channel, response: resp }).await;
                        println!("Alice served asset");
                    }
                }
                Some(event) = node_b.event_rx.recv() => {
                    if let NodeEvent::ShardReceived { response: resp, .. } = event {
                        response = Some(resp);
                        break;
                    }
                }
            }
        }
        response.unwrap()
    }).await.expect("Pull should complete through relay");

    // ============================================================
    // Step 5: Bob decrypts
    // ============================================================
    match result {
        NexusResponse::Asset { rfrag, manifest: manifest_bytes, shards: received_shards, .. } => {
            let grant: ShareGrant = serde_json::from_slice(&rfrag).unwrap();
            let recv_manifest: NexusManifest = serde_json::from_slice(&manifest_bytes).unwrap();

            for (expected_cid, shard_data) in recv_manifest.shards.shards.iter().zip(&received_shards) {
                let computed = compute_cid(shard_data);
                let computed_hex: String = computed.iter().map(|b| format!("{:02x}", b)).collect();
                assert_eq!(computed_hex, *expected_cid);
            }

            let public_kp = public_pre_keypair();
            let decrypted_dek = public_kp.decrypt_dek_reencrypted(
                &recv_manifest.encrypted_dek,
                &grant.cfrags,
                &recv_manifest.owner_pre_pk,
                &grant.verifying_key,
            ).unwrap();

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

            let encrypted_body = shard::reassemble(&recv_manifest.shards, &shard_objs).unwrap();
            let plaintext = nexus_core::crypto::decrypt_data(&encrypted_body, &decrypted_dek).unwrap();

            assert_eq!(plaintext, original_data);
            println!("\n🎉 RELAY PULL TEST PASSED: \"{}\"", String::from_utf8_lossy(&plaintext));
        }
        NexusResponse::AssetDenied { reason, .. } => panic!("Denied: {}", reason),
        _ => panic!("Unexpected response"),
    }

    node_a.shutdown().await.ok();
    node_b.shutdown().await.ok();
    relay.shutdown().await;
}
