use std::sync::Arc;
use tokio::sync::Mutex;
use serde::Serialize;

use nexus_core::identity::IdentityKeypair;
use nexus_core::network::{RelayServer, RelayConfig, RelayServerEvent, PeerId};

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
        identity: IdentityKeypair,
        port: u16,
        max_circuits: u32,
        max_reservations_per_peer: u32,
    ) -> Result<String, String> {
        let mut inner = self.inner.lock().await;
        if inner.server.is_some() {
            return Err("Relay already running".into());
        }

        let keypair = identity.to_libp2p_keypair();
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

        let stats = Arc::new(Mutex::new(RelayStats {
            running: true,
            peer_id: Some(peer_id.to_string()),
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
