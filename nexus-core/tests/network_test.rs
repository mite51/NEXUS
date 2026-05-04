//! Integration test: two NEXUS nodes discover each other via mDNS

use nexus_core::network::{NexusNode, NodeConfig, NodeEvent};
use libp2p::identity::Keypair;
use std::time::Duration;
use tokio::time::timeout;

#[tokio::test]
async fn test_two_nodes_discover_via_mdns() {
    // Start node A
    let keypair_a = Keypair::generate_ed25519();
    let config_a = NodeConfig::default();
    let mut node_a = NexusNode::start(keypair_a, config_a).await.unwrap();

    // Start node B
    let keypair_b = Keypair::generate_ed25519();
    let config_b = NodeConfig::default();
    let mut node_b = NexusNode::start(keypair_b, config_b).await.unwrap();

    // Wait for node A to discover node B (or vice versa) via mDNS
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

    // Cleanup
    node_a.shutdown().await.unwrap();
    node_b.shutdown().await.unwrap();
}

#[tokio::test]
async fn test_node_starts_and_listens() {
    let keypair = Keypair::generate_ed25519();
    let config = NodeConfig::default();
    let mut node = NexusNode::start(keypair, config).await.unwrap();

    // Should receive at least one Listening event
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
    let addr = listening.unwrap().unwrap();
    println!("Node listening on: {}", addr);

    node.shutdown().await.unwrap();
}
