use std::sync::Arc;
use tokio::sync::{Mutex, mpsc};
use serde::Serialize;

use nexus_core::identity::IdentityKeypair;
use nexus_core::network::{NexusNode, NodeConfig, NodeCommand};
use nexus_core::network::{spawn_delivery_worker, DeliveryConfig};

/// Shared node state managed by Tauri
pub struct NodeState {
    inner: Arc<Mutex<NodeInner>>,
}

struct NodeInner {
    node: Option<NexusNode>,
    delivery_handle: Option<tokio::task::JoinHandle<()>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct NodeInfo {
    pub running: bool,
    pub peer_id: Option<String>,
    pub listen_addrs: Vec<String>,
    pub connected_peers: Vec<String>,
}

impl NodeState {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(NodeInner {
                node: None,
                delivery_handle: None,
            })),
        }
    }

    pub async fn start(&self, identity: IdentityKeypair, config: NodeConfig) -> Result<String, String> {
        let mut inner = self.inner.lock().await;
        if inner.node.is_some() {
            return Err("Node already running".into());
        }

        let keypair = identity.to_libp2p_keypair();
        let node = NexusNode::start(keypair, config).await
            .map_err(|e| format!("Failed to start node: {}", e))?;

        let peer_id = node.peer_id.to_string();

        // Start delivery worker
        let delivery_config = DeliveryConfig::default();
        let delivery_handle = spawn_delivery_worker(
            node.command_tx.clone(),
            delivery_config,
        );

        inner.node = Some(node);
        inner.delivery_handle = Some(delivery_handle);

        Ok(peer_id)
    }

    pub async fn stop(&self) -> Result<(), String> {
        let mut inner = self.inner.lock().await;

        // Stop delivery worker
        if let Some(handle) = inner.delivery_handle.take() {
            handle.abort();
        }

        // Stop node
        if let Some(node) = inner.node.take() {
            node.shutdown().await
                .map_err(|e| format!("Shutdown failed: {}", e))?;
        }

        Ok(())
    }

    pub async fn info(&self) -> NodeInfo {
        let inner = self.inner.lock().await;
        match &inner.node {
            None => NodeInfo {
                running: false,
                peer_id: None,
                listen_addrs: vec![],
                connected_peers: vec![],
            },
            Some(node) => {
                let addrs = node.listening_addrs().await.unwrap_or_default();
                let peers = node.connected_peers().await.unwrap_or_default();

                NodeInfo {
                    running: true,
                    peer_id: Some(node.peer_id.to_string()),
                    listen_addrs: addrs.iter().map(|a| a.to_string()).collect(),
                    connected_peers: peers.iter().map(|p| p.to_string()).collect(),
                }
            }
        }
    }

    pub async fn command_tx(&self) -> Option<mpsc::Sender<NodeCommand>> {
        let inner = self.inner.lock().await;
        inner.node.as_ref().map(|n| n.command_tx.clone())
    }
}
