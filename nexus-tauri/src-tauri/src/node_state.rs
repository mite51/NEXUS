use std::sync::Arc;
use tokio::sync::{Mutex, mpsc};
use serde::Serialize;
use tauri::{AppHandle, Emitter};

use nexus_core::identity::IdentityKeypair;
use nexus_core::network::{NexusNode, NodeConfig, NodeCommand, NodeEvent, PeerId};
use nexus_core::network::{spawn_delivery_worker, DeliveryConfig};
use nexus_core::storage::ReceivedFiles;

const RECEIVED_FILES_PATH: &str = ".nexus-received.json";
const RECEIVED_MANIFESTS_DIR: &str = ".nexus-received-manifests";

/// Shared node state managed by Tauri
pub struct NodeState {
    inner: Arc<Mutex<NodeInner>>,
}

struct NodeInner {
    node: Option<NodeHandle>,
}

/// Active node handles
struct NodeHandle {
    peer_id: PeerId,
    command_tx: mpsc::Sender<NodeCommand>,
    delivery_handle: tokio::task::JoinHandle<()>,
    event_handle: tokio::task::JoinHandle<()>,
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
            })),
        }
    }

    pub async fn start(&self, identity: IdentityKeypair, config: NodeConfig, app_handle: AppHandle) -> Result<String, String> {
        let mut inner = self.inner.lock().await;
        if inner.node.is_some() {
            return Err("Node already running".into());
        }

        let keypair = identity.to_libp2p_keypair();
        let mut node = NexusNode::start(keypair, config).await
            .map_err(|e| format!("Failed to start node: {}", e))?;

        let peer_id = node.peer_id;
        let command_tx = node.command_tx.clone();

        // Start delivery worker
        let delivery_config = DeliveryConfig::default();
        let delivery_handle = spawn_delivery_worker(
            command_tx.clone(),
            delivery_config,
        );

        // Take event_rx and spawn event handler
        let event_rx = std::mem::replace(&mut node.event_rx, {
            let (_, rx) = mpsc::channel(1);
            rx
        });
        let event_handle = spawn_event_handler(event_rx, command_tx.clone(), app_handle);

        inner.node = Some(NodeHandle {
            peer_id,
            command_tx: node.command_tx,
            delivery_handle,
            event_handle,
        });

        Ok(peer_id.to_string())
    }

    pub async fn stop(&self) -> Result<(), String> {
        let mut inner = self.inner.lock().await;

        if let Some(handle) = inner.node.take() {
            handle.delivery_handle.abort();
            handle.event_handle.abort();
            handle.command_tx.send(NodeCommand::Shutdown).await
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
            Some(handle) => {
                // Query listening addrs
                let addrs = {
                    let (tx, rx) = tokio::sync::oneshot::channel();
                    if handle.command_tx.send(NodeCommand::GetListeningAddrs(tx)).await.is_ok() {
                        rx.await.unwrap_or_default()
                    } else {
                        vec![]
                    }
                };

                // Query connected peers
                let peers = {
                    let (tx, rx) = tokio::sync::oneshot::channel();
                    if handle.command_tx.send(NodeCommand::GetConnectedPeers(tx)).await.is_ok() {
                        rx.await.unwrap_or_default()
                    } else {
                        vec![]
                    }
                };

                NodeInfo {
                    running: true,
                    peer_id: Some(handle.peer_id.to_string()),
                    listen_addrs: addrs.iter().map(|a| a.to_string()).collect(),
                    connected_peers: peers.iter().map(|p| p.to_string()).collect(),
                }
            }
        }
    }

    #[allow(dead_code)]
    pub async fn command_tx(&self) -> Option<mpsc::Sender<NodeCommand>> {
        let inner = self.inner.lock().await;
        inner.node.as_ref().map(|n| n.command_tx.clone())
    }
}

/// Payload for file-received event
#[derive(Debug, Clone, Serialize)]
struct FileReceivedPayload {
    filename: String,
    from: String,
}

/// Spawn a task that processes incoming node events
fn spawn_event_handler(
    mut event_rx: mpsc::Receiver<NodeEvent>,
    command_tx: mpsc::Sender<NodeCommand>,
    app_handle: AppHandle,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        // Ensure the received manifests directory exists
        let _ = std::fs::create_dir_all(RECEIVED_MANIFESTS_DIR);

        while let Some(event) = event_rx.recv().await {
            match event {
                NodeEvent::ShardPushed { peer: _, cid, data, channel } => {
                    // Store the shard locally
                    let store = nexus_core::storage::ShardStore::open(".nexus-store").ok();
                    if let Some(store) = store {
                        let shard = nexus_core::storage::shard::Shard {
                            cid: nexus_core::storage::compute_cid(&data),
                            data,
                        };
                        let _ = store.put(&shard);
                    }
                    // Respond OK
                    let _ = command_tx.send(NodeCommand::RespondShard {
                        channel,
                        response: nexus_core::network::protocol::NexusResponse::ShardAccepted { cid },
                    }).await;
                }
                NodeEvent::ManifestPushed { peer, manifest_json, share_grant_json, channel } => {
                    // Parse manifest to get filename
                    let filename = serde_json::from_str::<serde_json::Value>(&manifest_json)
                        .ok()
                        .and_then(|v| v["shards"]["filename"].as_str().map(|s| s.to_string()))
                        .unwrap_or_else(|| "unknown".into());

                    let owner_did = serde_json::from_str::<serde_json::Value>(&manifest_json)
                        .ok()
                        .and_then(|v| v["owner"].as_str().map(|s| s.to_string()))
                        .unwrap_or_else(|| peer.to_string());

                    // Save manifest to disk
                    let ts = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs();
                    let manifest_filename = format!("{}-{}.nexus",
                        ts,
                        filename.replace('/', "_")
                    );
                    let manifest_path = format!("{}/{}", RECEIVED_MANIFESTS_DIR, manifest_filename);
                    let _ = std::fs::write(&manifest_path, &manifest_json);

                    // Record in received files
                    let received = ReceivedFiles::open(RECEIVED_FILES_PATH);
                    let _ = received.add(
                        owner_did,
                        peer.to_string(),
                        filename,
                        manifest_path,
                        share_grant_json,
                    );

                    // Respond OK to the sender
                    let _ = command_tx.send(NodeCommand::RespondShard {
                        channel,
                        response: nexus_core::network::protocol::NexusResponse::ManifestAccepted,
                    }).await;

                    // Emit event to frontend for notification
                    let _ = app_handle.emit("nexus://file-received", FileReceivedPayload {
                        filename: serde_json::from_str::<serde_json::Value>(&manifest_json)
                            .ok()
                            .and_then(|v| v["shards"]["filename"].as_str().map(|s| s.to_string()))
                            .unwrap_or_else(|| "file".into()),
                        from: peer.to_string(),
                    });
                }
                // Other events we don't handle yet
                _ => {}
            }
        }
    })
}
