//! Network-integrated store — bridges local ShardStore with P2P shard exchange
//!
//! The `NetworkStore` wraps a local `ShardStore` and a `NexusNode`, providing:
//! - Local shard storage + serving to peers
//! - Network shard fetching (request from peers when not available locally)
//! - DHT announcement of locally-held shards

use std::path::Path;
use std::time::Duration;
use tokio::sync::mpsc;

use crate::network::{NexusNode, NodeCommand, NodeEvent};
use crate::network::protocol::{NexusResponse};
use crate::storage::{ShardStore, Shard, compute_cid};

/// A network-aware shard store
pub struct NetworkStore {
    /// Local shard storage
    pub local: ShardStore,
    /// Command channel to the running node
    node_cmd: mpsc::Sender<NodeCommand>,
}

/// Result of a shard fetch operation
#[derive(Debug)]
pub enum FetchResult {
    /// Found locally
    Local(Shard),
    /// Fetched from a peer
    Remote(Shard),
    /// Not found anywhere
    NotFound,
}

impl NetworkStore {
    /// Create a NetworkStore backed by a local directory + connected to a running node
    pub fn new(store_path: impl AsRef<Path>, node: &NexusNode) -> Result<Self, String> {
        let local = ShardStore::open(store_path)?;
        Ok(Self {
            local,
            node_cmd: node.command_tx.clone(),
        })
    }

    /// Store a shard locally and announce it to the DHT
    pub async fn put(&self, shard: &Shard) -> Result<String, String> {
        let cid_hex = self.local.put(shard)?;

        // Announce to Kademlia that we hold this shard
        // (key = CID bytes, value = our peer ID serialized — handled by swarm)
        // For now, we publish availability via gossipsub
        let announce_msg = format!("HAVE:{}", cid_hex);
        let _ = self.node_cmd.send(NodeCommand::Publish {
            topic: "nexus/shards".to_string(),
            data: announce_msg.into_bytes(),
        }).await;

        Ok(cid_hex)
    }

    /// Store raw data (compute CID, store, announce)
    pub async fn put_data(&self, data: &[u8]) -> Result<String, String> {
        let cid = compute_cid(data);
        let shard = Shard { cid, data: data.to_vec() };
        self.put(&shard).await
    }

    /// Get a shard — tries local first, then network
    pub async fn get(
        &self,
        cid_hex: &str,
        node_events: &mut mpsc::Receiver<NodeEvent>,
        known_peers: &[libp2p::PeerId],
    ) -> Result<FetchResult, String> {
        // Try local first
        if let Some(shard) = self.local.get(cid_hex)? {
            return Ok(FetchResult::Local(shard));
        }

        // Try fetching from known peers
        for peer in known_peers {
            let _ = self.node_cmd.send(NodeCommand::RequestShard {
                peer: *peer,
                cid: cid_hex.to_string(),
            }).await;
        }

        // Wait for a response (timeout 10s)
        let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
        loop {
            match tokio::time::timeout_at(deadline, node_events.recv()).await {
                Ok(Some(NodeEvent::ShardRequested { .. })) => {
                    // We're serving, not receiving — skip
                    continue;
                }
                Ok(Some(_)) => {
                    // Other events — skip for now
                    continue;
                }
                Ok(None) | Err(_) => {
                    return Ok(FetchResult::NotFound);
                }
            }
        }
    }

    /// Check if we have a shard locally
    pub fn has_local(&self, cid_hex: &str) -> bool {
        self.local.has(cid_hex)
    }

    /// Handle incoming shard requests from peers
    /// Returns the response to send back
    pub fn handle_shard_request(&self, cid: &str) -> NexusResponse {
        match self.local.get(cid) {
            Ok(Some(shard)) => NexusResponse::Shard {
                cid: cid.to_string(),
                data: shard.data,
            },
            _ => NexusResponse::ShardNotFound {
                cid: cid.to_string(),
            },
        }
    }
}
