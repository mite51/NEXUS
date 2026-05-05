//! NAT traversal integration tests
//!
//! Tests relay reservation, DCUtR hole-punching, and telemetry collection
//! in a simulated multi-node environment.

use nexus_core::network::{
    NexusNode, NodeConfig, NodeEvent, NatStatus,
    TelemetryCollector, ConnectivityEvent,
};
use libp2p::identity::Keypair;
use tempfile::tempdir;
use tokio::time::{timeout, Duration};

/// Helper to create a node with a fresh identity
async fn create_test_node(telemetry_dir: &str) -> NexusNode {
    let keypair = Keypair::generate_ed25519();

    let config = NodeConfig {
        listen_addrs: vec!["/ip4/127.0.0.1/tcp/0".to_string()],
        bootstrap_peers: vec![],
        mdns_enabled: true,
        relay_servers: vec![],
        telemetry_enabled: true,
        telemetry_dir: Some(telemetry_dir.to_string()),
    };

    NexusNode::start(keypair, config).await.unwrap()
}

#[tokio::test]
async fn test_telemetry_records_connections() {
    let dir = tempdir().unwrap();
    let telemetry_path = dir.path().join("telemetry");

    let mut node1 = create_test_node(telemetry_path.to_str().unwrap()).await;
    let mut node2 = create_test_node(telemetry_path.to_str().unwrap()).await;

    // Wait for node1 to get a listen address
    let addr = loop {
        match timeout(Duration::from_secs(5), node1.event_rx.recv()).await {
            Ok(Some(NodeEvent::Listening(addr))) => break addr,
            Ok(Some(_)) => continue,
            _ => panic!("node1 did not start listening"),
        }
    };

    // Node2 dials node1
    node2.dial(addr.clone()).await.unwrap();

    // Wait for discovery
    let discovered = timeout(Duration::from_secs(5), async {
        loop {
            match node2.event_rx.recv().await {
                Some(NodeEvent::PeerDiscovered(peer)) if peer == node1.peer_id => return true,
                Some(NodeEvent::Listening(_)) => continue,
                Some(_) => continue,
                None => return false,
            }
        }
    }).await;

    assert!(discovered.unwrap_or(false), "node2 should discover node1");

    // Check telemetry recorded the connection
    let collector = TelemetryCollector::new(
        telemetry_path.to_str().unwrap(),
        "test".to_string(),
        true,
    );
    let stats = collector.stats();
    // At least one connection should have been recorded
    assert!(stats.connections_total >= 1, "Expected at least 1 connection, got {}", stats.connections_total);
}

#[tokio::test]
async fn test_telemetry_collector_stats_aggregation() {
    let dir = tempdir().unwrap();
    let collector = TelemetryCollector::new(dir.path(), "local-peer".to_string(), true);

    // Simulate a series of events
    collector.record(ConnectivityEvent::NatStatusChanged {
        status: NatStatus::Private,
        confidence: 3,
    });

    collector.record(ConnectivityEvent::RelayReservation {
        relay_peer: "relay1".to_string(),
        relay_addr: "/ip4/1.2.3.4/tcp/4001/p2p/relay1".to_string(),
        success: true,
        error: None,
        duration_ms: 120,
    });

    collector.record(ConnectivityEvent::HolePunch {
        remote_peer: "peer-a".to_string(),
        success: true,
        direct_addr: Some("/ip4/5.6.7.8/tcp/9000".to_string()),
        error: None,
        duration_ms: 350,
    });

    collector.record(ConnectivityEvent::HolePunch {
        remote_peer: "peer-b".to_string(),
        success: false,
        direct_addr: None,
        error: Some("timeout after 5s".to_string()),
        duration_ms: 5000,
    });

    collector.record(ConnectivityEvent::DialFailure {
        remote_peer: Some("peer-c".to_string()),
        addr: "/ip4/10.0.0.1/tcp/5000".to_string(),
        error: "connection refused".to_string(),
        is_relay: false,
    });

    collector.record(ConnectivityEvent::ConnectionEstablished {
        remote_peer: "peer-a".to_string(),
        addr: "/ip4/5.6.7.8/tcp/9000".to_string(),
        is_relayed: false,
        num_established: 1,
    });

    collector.record(ConnectivityEvent::ConnectionEstablished {
        remote_peer: "peer-d".to_string(),
        addr: "/ip4/relay/p2p-circuit/peer-d".to_string(),
        is_relayed: true,
        num_established: 1,
    });

    let stats = collector.stats();
    assert_eq!(stats.hole_punch_attempts, 2);
    assert_eq!(stats.hole_punch_successes, 1);
    assert_eq!(stats.relay_attempts, 1);
    assert_eq!(stats.relay_successes, 1);
    assert_eq!(stats.dial_failures, 1);
    assert_eq!(stats.connections_total, 2);
    assert_eq!(stats.connections_relayed, 1);
    assert_eq!(stats.last_nat_status, NatStatus::Private);
}

#[tokio::test]
async fn test_relay_dial_with_bad_address() {
    // If we configure a relay server that's unreachable, it should be
    // recorded in telemetry as a failure but not crash the node
    let dir = tempdir().unwrap();
    let telemetry_path = dir.path().join("telemetry");

    let keypair = Keypair::generate_ed25519();

    let config = NodeConfig {
        listen_addrs: vec!["/ip4/127.0.0.1/tcp/0".to_string()],
        bootstrap_peers: vec![],
        mdns_enabled: false,
        // This relay address is intentionally unreachable
        relay_servers: vec!["/ip4/192.0.2.1/tcp/4001/p2p/12D3KooWDpJ7As7BWAwRMfu1VU2WCqNjvq387JEYKDBj4kx6nXTN".to_string()],
        telemetry_enabled: true,
        telemetry_dir: Some(telemetry_path.to_str().unwrap().to_string()),
    };

    let mut node = NexusNode::start(keypair, config).await.unwrap();

    // Node should still start and listen
    let started = timeout(Duration::from_secs(5), async {
        loop {
            match node.event_rx.recv().await {
                Some(NodeEvent::Listening(_)) => return true,
                Some(_) => continue,
                None => return false,
            }
        }
    }).await;

    assert!(started.unwrap_or(false), "Node should start even with bad relay");

    // Check telemetry recorded the relay attempt
    let collector = TelemetryCollector::new(
        telemetry_path.to_str().unwrap(),
        "test".to_string(),
        true,
    );
    let stats = collector.stats();
    // The relay reservation should have been attempted
    assert_eq!(stats.relay_attempts, 1);
}

#[tokio::test]
async fn test_nat_status_event_propagation() {
    // Verify that AutoNAT events would propagate correctly
    // (In a real network with multiple peers, AutoNAT would probe reachability)
    let dir = tempdir().unwrap();
    let telemetry_path = dir.path().join("telemetry");

    let mut node = create_test_node(telemetry_path.to_str().unwrap()).await;

    // Node starts — wait for listening event
    let started = timeout(Duration::from_secs(3), async {
        loop {
            match node.event_rx.recv().await {
                Some(NodeEvent::Listening(_)) => return true,
                Some(_) => continue,
                None => return false,
            }
        }
    }).await;

    assert!(started.unwrap_or(false));
    // AutoNAT needs peers to probe — in isolation it stays Unknown
    // This test just verifies the node doesn't crash with AutoNAT enabled
}
