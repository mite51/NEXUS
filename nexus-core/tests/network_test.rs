//! Integration tests: NEXUS networking

use nexus_core::network::{NexusNode, NodeConfig, NodeEvent, NodeCommand};
use nexus_core::network::protocol::NexusResponse;
use libp2p::identity::Keypair;
use std::time::Duration;
use tokio::time::timeout;

#[tokio::test]
async fn test_two_nodes_discover_via_mdns() {
    let keypair_a = Keypair::generate_ed25519();
    let config_a = NodeConfig::default();
    let mut node_a = NexusNode::start(keypair_a, config_a).await.unwrap();

    let keypair_b = Keypair::generate_ed25519();
    let config_b = NodeConfig::default();
    let mut node_b = NexusNode::start(keypair_b, config_b).await.unwrap();

    let discovery_timeout = Duration::from_secs(15);
    let discovered = timeout(discovery_timeout, async {
        loop {
            tokio::select! {
                Some(event) = node_a.event_rx.recv() => {
                    if let NodeEvent::PeerDiscovered(peer) = event {
                        if peer == *node_b.peer_id() {
                            return true;
                        }
                    }
                }
                Some(event) = node_b.event_rx.recv() => {
                    if let NodeEvent::PeerDiscovered(peer) = event {
                        if peer == *node_a.peer_id() {
                            return true;
                        }
                    }
                }
            }
        }
    })
    .await;

    assert!(discovered.is_ok(), "Nodes should discover each other via mDNS");
    node_a.shutdown().await.unwrap();
    node_b.shutdown().await.unwrap();
}

#[tokio::test]
async fn test_node_starts_and_listens() {
    let keypair = Keypair::generate_ed25519();
    let config = NodeConfig::default();
    let mut node = NexusNode::start(keypair, config).await.unwrap();

    let listen_timeout = Duration::from_secs(5);
    let listening = timeout(listen_timeout, async {
        while let Some(event) = node.event_rx.recv().await {
            if let NodeEvent::Listening(addr) = event {
                return Some(addr);
            }
        }
        None
    })
    .await;

    assert!(listening.is_ok(), "Node should start listening");
    println!("Node listening on: {}", listening.unwrap().unwrap());
    node.shutdown().await.unwrap();
}

#[tokio::test]
async fn test_shard_request_response() {
    // Node A has a shard, Node B requests it
    let keypair_a = Keypair::generate_ed25519();
    let config_a = NodeConfig::default();
    let mut node_a = NexusNode::start(keypair_a, config_a).await.unwrap();
    let peer_a = *node_a.peer_id();

    let keypair_b = Keypair::generate_ed25519();
    let config_b = NodeConfig::default();
    let mut node_b = NexusNode::start(keypair_b, config_b).await.unwrap();
    let peer_b = *node_b.peer_id();

    // Wait for BOTH nodes to discover each other (mDNS auto-dials)
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
    assert!(discovery.is_ok(), "Peers must discover each other");

    // Give connections a moment to stabilize
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Node B requests a shard from Node A
    let test_cid = "deadbeef01234567".to_string();
    let test_data = b"hello nexus shard data!".to_vec();
    let test_cid_clone = test_cid.clone();
    let test_data_clone = test_data.clone();

    node_b.request_shard(peer_a, test_cid.clone()).await.unwrap();

    // Node A receives the request and responds
    let response_sent = timeout(Duration::from_secs(5), async {
        while let Some(event) = node_a.event_rx.recv().await {
            if let NodeEvent::ShardRequested { cid, channel, .. } = event {
                if cid == test_cid_clone {
                    let _ = node_a.command_tx.send(NodeCommand::RespondShard {
                        channel,
                        response: NexusResponse::Shard {
                            cid: test_cid_clone.clone(),
                            data: test_data_clone.clone(),
                        },
                    }).await;
                    return true;
                }
            }
        }
        false
    }).await;

    assert!(response_sent.is_ok() && response_sent.unwrap(), "Node A should respond to shard request");

    node_a.shutdown().await.unwrap();
    node_b.shutdown().await.unwrap();
}

#[tokio::test]
async fn test_identity_to_peer_id_consistency() {
    // Verify that the same Ed25519 key produces the same PeerId every time
    use nexus_core::identity::IdentityKeypair;

    let identity = IdentityKeypair::generate();
    let peer_id_1 = identity.peer_id();
    let peer_id_2 = identity.peer_id();
    assert_eq!(peer_id_1, peer_id_2);

    // Roundtrip through bytes
    let secret = identity.to_secret_bytes();
    let restored = IdentityKeypair::from_secret_bytes(&secret);
    assert_eq!(identity.peer_id(), restored.peer_id());
}
