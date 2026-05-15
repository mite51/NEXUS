use std::sync::Arc;
use tokio::sync::Mutex;
use serde::Serialize;

use nexus_core::network::{RelayServer, RelayConfig, RelayServerEvent, PeerId, Libp2pKeypair};

/// Shared relay server state managed by Tauri
pub struct RelayState {
    inner: Arc<Mutex<RelayInner>>,
}

struct RelayInner {
    server: Option<RelayHandle>,
}

struct RelayHandle {
    peer_id: PeerId,
    event_handle: tokio::task::JoinHandle<()>,
    stats: Arc<Mutex<RelayStats>>,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct RelayStats {
    pub running: bool,
    pub peer_id: Option<String>,
    pub public_ip: Option<String>,
    pub listen_addrs: Vec<String>,
    pub connected_peers: u32,
    pub active_reservations: u32,
    pub total_circuits: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct RelayInfo {
    pub running: bool,
    pub peer_id: Option<String>,
    pub stats: RelayStats,
}

impl RelayState {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(RelayInner {
                server: None,
            })),
        }
    }

    pub async fn start(
        &self,
        port: u16,
        max_circuits: u32,
        max_reservations_per_peer: u32,
    ) -> Result<String, String> {
        let mut inner = self.inner.lock().await;
        if inner.server.is_some() {
            return Err("Relay already running".into());
        }

        // Relay uses its own identity separate from the user vault.
        // This avoids PeerId collision when the node and relay share a vault.
        let key_path = ".nexus-relay-key";
        let keypair = if std::path::Path::new(key_path).exists() {
            let bytes = std::fs::read(key_path)
                .map_err(|e| format!("Failed to read relay key: {}", e))?;
            Libp2pKeypair::from_protobuf_encoding(&bytes)
                .map_err(|e| format!("Failed to decode relay key: {}", e))?
        } else {
            let kp = Libp2pKeypair::generate_ed25519();
            let bytes = kp.to_protobuf_encoding()
                .map_err(|e| format!("Failed to encode relay key: {}", e))?;
            std::fs::write(key_path, &bytes)
                .map_err(|e| format!("Failed to write relay key: {}", e))?;
            eprintln!("  Generated new relay identity (saved to {})", key_path);
            kp
        };
        let peer_id = keypair.public().to_peer_id();

        let config = RelayConfig {
            listen_addrs: vec![
                format!("/ip4/0.0.0.0/tcp/{}", port),
                format!("/ip4/0.0.0.0/udp/{}/quic-v1", port),
            ],
            max_reservations_per_peer,
            max_circuits,
            ..Default::default()
        };

        let mut server = RelayServer::start(keypair, config).await
            .map_err(|e| format!("Failed to start relay: {}", e))?;

        eprintln!("[relay_state] Relay started! PeerId={}, port={}", peer_id, port);

                let stats = Arc::new(Mutex::new(RelayStats {
            running: true,
            peer_id: Some(peer_id.to_string()),
            public_ip: None,
            listen_addrs: vec![],
            connected_peers: 0,
            active_reservations: 0,
            total_circuits: 0,
        }));

        // Spawn event handler
        let stats_clone = stats.clone();
        let event_handle = tokio::spawn(async move {
            while let Some(event) = server.event_rx.recv().await {
                let mut s = stats_clone.lock().await;
                match event {
                    RelayServerEvent::Listening(addr) => {
                        s.listen_addrs.push(addr);
                    }
                    RelayServerEvent::PublicIpDetected(ip) => {
                        s.public_ip = Some(ip);
                    }
                    RelayServerEvent::PeerConnected(_) => {
                        s.connected_peers += 1;
                    }
                    RelayServerEvent::PeerDisconnected(_) => {
                        s.connected_peers = s.connected_peers.saturating_sub(1);
                    }
                    RelayServerEvent::ReservationAccepted { .. } => {
                        s.active_reservations += 1;
                    }
                    RelayServerEvent::ReservationExpired { .. } => {
                        s.active_reservations = s.active_reservations.saturating_sub(1);
                    }
                    RelayServerEvent::CircuitOpened { .. } => {
                        s.total_circuits += 1;
                    }
                    RelayServerEvent::CircuitClosed { .. } => {}
                }
            }
        });

        inner.server = Some(RelayHandle {
            peer_id,
            event_handle,
            stats,
        });

        Ok(peer_id.to_string())
    }

    pub async fn stop(&self) -> Result<(), String> {
        let mut inner = self.inner.lock().await;
        if let Some(handle) = inner.server.take() {
            handle.event_handle.abort();
        }
        Ok(())
    }

    pub async fn info(&self) -> RelayInfo {
        let inner = self.inner.lock().await;
        match &inner.server {
            None => RelayInfo {
                running: false,
                peer_id: None,
                stats: RelayStats::default(),
            },
            Some(handle) => {
                let stats = handle.stats.lock().await.clone();
                RelayInfo {
                    running: true,
                    peer_id: Some(handle.peer_id.to_string()),
                    stats,
                }
            }
        }
    }
}
