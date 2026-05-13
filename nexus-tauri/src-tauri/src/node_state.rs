use std::sync::Arc;
use tokio::sync::{Mutex, mpsc};
use serde::Serialize;
use tauri::{AppHandle, Emitter};

use nexus_core::identity::IdentityKeypair;
use nexus_core::network::{NexusNode, NodeConfig, NodeCommand, NodeEvent, PeerId};
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
            .map_err(|e| {
                let err_str = e.to_string();
                let detail = if err_str.contains("Address already in use") || err_str.contains("AddrInUse") || err_str.contains("address already in use") || err_str.contains("10048") {
                    format!("Port conflict: another process is already using this port. Change listen port in Settings. ({})", err_str)
                } else if err_str.is_empty() {
                    "Unknown error (empty). Check that no other node is running and the port is free.".to_string()
                } else {
                    err_str
                };
                let _ = app_handle.emit("nexus://node-log", NodeLogPayload {
                    level: "error".into(),
                    source: "Node".into(),
                    message: "Failed to start node".into(),
                    detail: Some(detail.clone()),
                });
                format!("Failed to start node: {}", detail)
            })?;

        let peer_id = node.peer_id;
        let command_tx = node.command_tx.clone();

        // Take event_rx and spawn event handler
        let event_rx = std::mem::replace(&mut node.event_rx, {
            let (_, rx) = mpsc::channel(1);
            rx
        });
        let event_handle = spawn_event_handler(event_rx, command_tx.clone(), app_handle);

        inner.node = Some(NodeHandle {
            peer_id,
            command_tx: node.command_tx,
            event_handle,
        });

        Ok(peer_id.to_string())
    }

    pub async fn stop(&self) -> Result<(), String> {
        let mut inner = self.inner.lock().await;

        if let Some(handle) = inner.node.take() {
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

/// Payload for log events sent to frontend
#[derive(Debug, Clone, Serialize)]
struct NodeLogPayload {
    level: String,
    source: String,
    message: String,
    detail: Option<String>,
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
            // Emit log events for key node events
            match &event {
                NodeEvent::PeerDiscovered(peer) => {
                    let _ = app_handle.emit("nexus://node-log", NodeLogPayload {
                        level: "info".into(),
                        source: "Network".into(),
                        message: format!("Peer connected: {}", &peer.to_string()[..16]),
                        detail: Some(peer.to_string()),
                    });
                }
                NodeEvent::PeerDisconnected(peer) => {
                    let _ = app_handle.emit("nexus://node-log", NodeLogPayload {
                        level: "warn".into(),
                        source: "Network".into(),
                        message: format!("Peer disconnected: {}", &peer.to_string()[..16]),
                        detail: Some(peer.to_string()),
                    });
                }
                NodeEvent::Listening(addr) => {
                    let _ = app_handle.emit("nexus://node-log", NodeLogPayload {
                        level: "info".into(),
                        source: "Network".into(),
                        message: format!("Listening on {}", addr),
                        detail: None,
                    });
                }
                NodeEvent::NatStatusChanged { status } => {
                    let _ = app_handle.emit("nexus://node-log", NodeLogPayload {
                        level: "info".into(),
                        source: "NAT".into(),
                        message: format!("NAT status: {:?}", status),
                        detail: None,
                    });
                }
                NodeEvent::RelayReserved { relay_peer, relay_addr } => {
                    let _ = app_handle.emit("nexus://node-log", NodeLogPayload {
                        level: "success".into(),
                        source: "Relay".into(),
                        message: format!("Relay reserved via {}", &relay_peer.to_string()[..16]),
                        detail: Some(relay_addr.to_string()),
                    });
                }
                NodeEvent::HolePunchResult { remote_peer, success } => {
                    let _ = app_handle.emit("nexus://node-log", NodeLogPayload {
                        level: if *success { "success" } else { "warn" }.into(),
                        source: "DCUtR".into(),
                        message: format!("Hole punch {}: {}", if *success { "succeeded" } else { "failed" }, &remote_peer.to_string()[..16]),
                        detail: Some(remote_peer.to_string()),
                    });
                }
                NodeEvent::ShardRequested { peer, cid, .. } => {
                    let _ = app_handle.emit("nexus://node-log", NodeLogPayload {
                        level: "info".into(),
                        source: "Shard".into(),
                        message: format!("Shard requested by {}", &peer.to_string()[..16]),
                        detail: Some(format!("CID: {}", cid)),
                    });
                }
                NodeEvent::ManifestPushed { peer, .. } => {
                    let _ = app_handle.emit("nexus://node-log", NodeLogPayload {
                        level: "info".into(),
                        source: "Transfer".into(),
                        message: format!("Manifest received from {}", &peer.to_string()[..16]),
                        detail: Some(peer.to_string()),
                    });
                }
                _ => {}
            }

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
                NodeEvent::PullAssetRequested { peer, asset_id, requester_did, signature: _, channel } => {
                    // Serve asset if rfrag exists for requester
                    let store = nexus_core::storage::AssetStore::open(".nexus-store").ok();
                    let response = if let Some(store) = store {
                        if let Ok(Some(rfrag)) = store.get_rfrag(&asset_id, &requester_did) {
                            // Authorized via rfrag — serve asset
                            match store.get_manifest(&asset_id) {
                                Ok(Some(manifest_bytes)) => {
                                    // Load shards from manifest
                                    let shard_store = nexus_core::storage::ShardStore::open(".nexus-store").ok();
                                    let mut shard_data = Vec::new();
                                    let mut shards_ok = true;
                                    if let Ok(manifest) = serde_json::from_slice::<nexus_core::manifest::NexusManifest>(&manifest_bytes) {
                                        for cid in &manifest.shards.shards {
                                            if let Some(ref ss) = shard_store {
                                                if let Ok(Some(shard)) = ss.get(cid) {
                                                    shard_data.push(shard.data);
                                                } else {
                                                    shards_ok = false;
                                                    break;
                                                }
                                            } else {
                                                shards_ok = false;
                                                break;
                                            }
                                        }
                                    } else {
                                        shards_ok = false;
                                    }
                                    if shards_ok {
                                        let _ = app_handle.emit("nexus://node-log", NodeLogPayload {
                                            level: "info".into(),
                                            source: "Pull".into(),
                                            message: format!("Serving asset to {}", &peer.to_string()[..16]),
                                            detail: Some(format!("asset: {}, did: {}", &asset_id[..16], requester_did)),
                                        });
                                        nexus_core::network::protocol::NexusResponse::Asset {
                                            asset_id,
                                            rfrag,
                                            manifest: manifest_bytes,
                                            shards: shard_data,
                                        }
                                    } else {
                                        nexus_core::network::protocol::NexusResponse::AssetDenied {
                                            asset_id,
                                            reason: "Shards not available".into(),
                                        }
                                    }
                                }
                                _ => nexus_core::network::protocol::NexusResponse::AssetDenied {
                                    asset_id,
                                    reason: "Manifest not found".into(),
                                },
                            }
                        } else if store.is_public(&asset_id).unwrap_or(false) {
                            // Public asset — serve using did:nexus:public rfrag (same as private flow)
                            use nexus_core::crypto::pre::PUBLIC_DID;
                            if let Ok(Some(rfrag)) = store.get_rfrag(&asset_id, PUBLIC_DID) {
                                match store.get_manifest(&asset_id) {
                                    Ok(Some(manifest_bytes)) => {
                                        let shard_store = nexus_core::storage::ShardStore::open(".nexus-store").ok();
                                        let mut shard_data = Vec::new();
                                        let mut shards_ok = true;
                                        if let Ok(manifest) = serde_json::from_slice::<nexus_core::manifest::NexusManifest>(&manifest_bytes) {
                                            for cid in &manifest.shards.shards {
                                                if let Some(ref ss) = shard_store {
                                                    if let Ok(Some(shard)) = ss.get(cid) {
                                                        shard_data.push(shard.data);
                                                    } else {
                                                        shards_ok = false;
                                                        break;
                                                    }
                                                } else {
                                                    shards_ok = false;
                                                    break;
                                                }
                                            }
                                        } else {
                                            shards_ok = false;
                                        }
                                        if shards_ok {
                                            let _ = app_handle.emit("nexus://node-log", NodeLogPayload {
                                                level: "info".into(),
                                                source: "Pull".into(),
                                                message: format!("Serving PUBLIC asset to {}", &peer.to_string()[..16]),
                                                detail: Some(format!("asset: {}", &asset_id[..16])),
                                            });
                                            nexus_core::network::protocol::NexusResponse::Asset {
                                                asset_id,
                                                rfrag,
                                                manifest: manifest_bytes,
                                                shards: shard_data,
                                            }
                                        } else {
                                            nexus_core::network::protocol::NexusResponse::AssetDenied {
                                                asset_id,
                                                reason: "Shards not available".into(),
                                            }
                                        }
                                    }
                                    _ => nexus_core::network::protocol::NexusResponse::AssetDenied {
                                        asset_id,
                                        reason: "Manifest not found".into(),
                                    },
                                }
                            } else {
                                nexus_core::network::protocol::NexusResponse::AssetDenied {
                                    asset_id,
                                    reason: "Public grant not configured".into(),
                                }
                            }
                        } else {
                            nexus_core::network::protocol::NexusResponse::AssetDenied {
                                asset_id,
                                reason: "Unauthorized".into(),
                            }
                        }
                    } else {
                        nexus_core::network::protocol::NexusResponse::AssetDenied {
                            asset_id,
                            reason: "Store unavailable".into(),
                        }
                    };
                    let _ = command_tx.send(nexus_core::network::NodeCommand::RespondShard {
                        channel,
                        response,
                    }).await;
                }
                // Other events we don't handle yet
                _ => {}
            }
        }
    })
}
