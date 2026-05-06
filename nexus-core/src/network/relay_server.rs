//! NEXUS Relay Server
//!
//! A standalone relay node that helps NATted peers find each other.
//! It provides:
//! - Relay reservations (allow peers to be reachable through us)
//! - AutoNAT probing (help peers discover their NAT status)
//! - Identify (exchange metadata)
//! - Kademlia (peer discovery)

use libp2p::{
    identity::Keypair, Multiaddr, PeerId, SwarmBuilder,
    noise, tcp, yamux,
    swarm::{NetworkBehaviour, SwarmEvent},
    identify, kad, relay, autonat,
};
use futures::StreamExt;
use std::time::Duration;
use tokio::sync::mpsc;

/// Combined behaviour for a relay server
#[derive(NetworkBehaviour)]
pub struct RelayBehaviour {
    /// Relay server — accepts reservations and relays traffic
    pub relay: relay::Behaviour,
    /// Identify — exchange peer metadata on connect
    pub identify: identify::Behaviour,
    /// Kademlia — peer discovery DHT
    pub kademlia: kad::Behaviour<kad::store::MemoryStore>,
    /// AutoNAT server — help peers probe their NAT status
    pub autonat: autonat::Behaviour,
}

/// Configuration for the relay server
#[derive(Debug, Clone)]
pub struct RelayConfig {
    /// Listen addresses
    pub listen_addrs: Vec<String>,
    /// Max reservations per peer
    pub max_reservations_per_peer: u32,
    /// Max circuits (concurrent relayed connections)
    pub max_circuits: u32,
    /// Reservation duration (seconds)
    pub reservation_duration_secs: u64,
    /// Max circuit duration (seconds)
    pub max_circuit_duration_secs: u64,
    /// Max circuit bytes (per circuit)
    pub max_circuit_bytes: u64,
}

impl Default for RelayConfig {
    fn default() -> Self {
        Self {
            listen_addrs: vec![
                "/ip4/0.0.0.0/tcp/4001".to_string(),
                "/ip4/0.0.0.0/udp/4001/quic-v1".to_string(),
            ],
            max_reservations_per_peer: 4,
            max_circuits: 128,
            reservation_duration_secs: 3600,
            max_circuit_duration_secs: 120,
            max_circuit_bytes: 16 * 1024 * 1024, // 16 MB per circuit
        }
    }
}

/// Events emitted by the relay server
#[derive(Debug, Clone)]
pub enum RelayServerEvent {
    Listening(String),
    ReservationAccepted { peer: String },
    ReservationExpired { peer: String },
    CircuitOpened { src: String, dst: String },
    CircuitClosed { src: String, dst: String },
    PeerConnected(String),
    PeerDisconnected(String),
}

/// The relay server node
pub struct RelayServer {
    pub peer_id: PeerId,
    pub event_rx: mpsc::Receiver<RelayServerEvent>,
    shutdown_tx: mpsc::Sender<()>,
}

impl RelayServer {
    /// Start a new relay server
    pub async fn start(
        keypair: Keypair,
        config: RelayConfig,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let local_peer_id = keypair.public().to_peer_id();

        // Configure relay limits
        let relay_config = relay::Config {
            max_reservations_per_peer: config.max_reservations_per_peer as usize,
            max_circuits: config.max_circuits as usize,
            reservation_duration: Duration::from_secs(config.reservation_duration_secs),
            max_circuit_duration: Duration::from_secs(config.max_circuit_duration_secs),
            max_circuit_bytes: config.max_circuit_bytes,
            ..Default::default()
        };

        // Build swarm
        let mut swarm = SwarmBuilder::with_existing_identity(keypair.clone())
            .with_tokio()
            .with_tcp(
                tcp::Config::default(),
                noise::Config::new,
                yamux::Config::default,
            )?
            .with_quic()
            .with_behaviour(|key| {
                // Relay server behaviour
                let relay = relay::Behaviour::new(local_peer_id, relay_config);

                // Identify
                let identify = identify::Behaviour::new(
                    identify::Config::new(
                        "/nexus/relay/1.0.0".to_string(),
                        key.public(),
                    )
                    .with_push_listen_addr_updates(true),
                );

                // Kademlia
                let store = kad::store::MemoryStore::new(local_peer_id);
                let mut kademlia = kad::Behaviour::new(local_peer_id, store);
                kademlia.set_mode(Some(kad::Mode::Server));

                // AutoNAT (server mode — help others probe)
                let autonat = autonat::Behaviour::new(
                    local_peer_id,
                    autonat::Config {
                        throttle_server_period: Duration::from_secs(5),
                        ..Default::default()
                    },
                );

                RelayBehaviour {
                    relay,
                    identify,
                    kademlia,
                    autonat,
                }
            })?
            .with_swarm_config(|cfg| {
                cfg.with_idle_connection_timeout(Duration::from_secs(300))
            })
            .build();

        // Listen on configured addresses
        for addr_str in &config.listen_addrs {
            let addr: Multiaddr = addr_str.parse()
                .map_err(|e| format!("invalid listen addr '{}': {}", addr_str, e))?;
            swarm.listen_on(addr)?;
        }

        let (event_tx, event_rx) = mpsc::channel::<RelayServerEvent>(256);
        let (shutdown_tx, mut shutdown_rx) = mpsc::channel::<()>(1);

        // Spawn event loop
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    event = swarm.select_next_some() => {
                        Self::handle_event(event, &event_tx).await;
                    }
                    _ = shutdown_rx.recv() => {
                        break;
                    }
                }
            }
        });

        Ok(Self {
            peer_id: local_peer_id,
            event_rx,
            shutdown_tx,
        })
    }

    async fn handle_event(
        event: SwarmEvent<RelayBehaviourEvent>,
        event_tx: &mpsc::Sender<RelayServerEvent>,
    ) {
        match event {
            SwarmEvent::NewListenAddr { address, .. } => {
                let _ = event_tx.send(RelayServerEvent::Listening(address.to_string())).await;
            }
            SwarmEvent::ConnectionEstablished { peer_id, .. } => {
                let _ = event_tx.send(RelayServerEvent::PeerConnected(peer_id.to_string())).await;
            }
            SwarmEvent::ConnectionClosed { peer_id, num_established, .. } => {
                if num_established == 0 {
                    let _ = event_tx.send(RelayServerEvent::PeerDisconnected(peer_id.to_string())).await;
                }
            }
            SwarmEvent::Behaviour(RelayBehaviourEvent::Relay(
                relay::Event::ReservationReqAccepted { src_peer_id, .. }
            )) => {
                let _ = event_tx.send(RelayServerEvent::ReservationAccepted {
                    peer: src_peer_id.to_string(),
                }).await;
            }
            SwarmEvent::Behaviour(RelayBehaviourEvent::Relay(
                relay::Event::ReservationTimedOut { src_peer_id, .. }
            )) => {
                let _ = event_tx.send(RelayServerEvent::ReservationExpired {
                    peer: src_peer_id.to_string(),
                }).await;
            }
            SwarmEvent::Behaviour(RelayBehaviourEvent::Relay(
                relay::Event::CircuitReqAccepted { src_peer_id, dst_peer_id, .. }
            )) => {
                let _ = event_tx.send(RelayServerEvent::CircuitOpened {
                    src: src_peer_id.to_string(),
                    dst: dst_peer_id.to_string(),
                }).await;
            }
            SwarmEvent::Behaviour(RelayBehaviourEvent::Relay(
                relay::Event::CircuitClosed { src_peer_id, dst_peer_id, .. }
            )) => {
                let _ = event_tx.send(RelayServerEvent::CircuitClosed {
                    src: src_peer_id.to_string(),
                    dst: dst_peer_id.to_string(),
                }).await;
            }
            _ => {}
        }
    }

    /// Shut down the relay server
    pub async fn shutdown(self) {
        let _ = self.shutdown_tx.send(()).await;
    }
}
