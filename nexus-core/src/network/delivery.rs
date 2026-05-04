//! Delivery Worker — background task that processes the send queue
//!
//! Periodically checks for pending sends. For each, attempts to dial
//! the recipient and push shards + manifest. On success, marks delivered.
//! On failure, increments attempt counter and leaves pending for retry.

use std::fs;
use std::str::FromStr;
use std::time::Duration;
use tokio::sync::mpsc;
use libp2p::{Multiaddr, PeerId};

use super::node::NodeCommand;
use super::send_queue::{SendQueue, SendStatus};
use crate::storage::ShardStore;

/// Configuration for the delivery worker
#[derive(Debug, Clone)]
pub struct DeliveryConfig {
    /// How often to check the queue (seconds)
    pub check_interval_secs: u64,
    /// Max attempts before marking as failed
    pub max_attempts: u32,
    /// Path to the send queue file
    pub queue_path: String,
    /// Path to the shard store
    pub store_path: String,
}

impl Default for DeliveryConfig {
    fn default() -> Self {
        Self {
            check_interval_secs: 30,
            max_attempts: 10,
            queue_path: ".nexus-send-queue.json".into(),
            store_path: ".nexus-store".into(),
        }
    }
}

/// Spawns the delivery worker as a background task.
/// Returns a handle that can be used to abort the worker.
pub fn spawn_delivery_worker(
    command_tx: mpsc::Sender<NodeCommand>,
    config: DeliveryConfig,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        delivery_loop(command_tx, config).await;
    })
}

async fn delivery_loop(
    command_tx: mpsc::Sender<NodeCommand>,
    config: DeliveryConfig,
) {
    let interval = Duration::from_secs(config.check_interval_secs);

    loop {
        tokio::time::sleep(interval).await;

        let queue = SendQueue::open(&config.queue_path);
        let pending = queue.pending();

        if pending.is_empty() {
            continue;
        }

        for send in pending {
            // Check max attempts
            if send.attempts >= config.max_attempts {
                let _ = queue.mark_failed(
                    &send.id,
                    format!("Max attempts ({}) reached", config.max_attempts),
                );
                continue;
            }

            // Parse recipient PeerId
            let peer_id = match PeerId::from_str(&send.recipient_peer_id) {
                Ok(p) => p,
                Err(e) => {
                    let _ = queue.mark_failed(&send.id, format!("Invalid PeerId: {}", e));
                    continue;
                }
            };

            // Mark in-progress
            let _ = queue.mark_in_progress(&send.id);

            // Try to dial if we have an address
            if let Some(ref addr_str) = send.recipient_addr {
                if let Ok(addr) = addr_str.parse::<Multiaddr>() {
                    let _ = command_tx.send(NodeCommand::Dial(addr)).await;
                    // Give it a moment to connect
                    tokio::time::sleep(Duration::from_millis(500)).await;
                }
            }

            // Load and push shards
            let store = match ShardStore::open(&config.store_path) {
                Ok(s) => s,
                Err(_) => {
                    // Can't read store — revert to pending, try next cycle
                    let _ = queue.mark_failed(&send.id, "Cannot open shard store".into());
                    continue;
                }
            };

            let mut all_shards_sent = true;
            for cid in &send.shard_cids {
                let shard_data = match store.get(cid) {
                    Ok(Some(shard)) => shard.data,
                    _ => {
                        all_shards_sent = false;
                        break;
                    }
                };

                if command_tx.send(NodeCommand::PushShard {
                    peer: peer_id,
                    cid: cid.clone(),
                    data: shard_data,
                }).await.is_err() {
                    all_shards_sent = false;
                    break;
                }
            }

            if !all_shards_sent {
                // Revert to pending — node might be shut down or shard missing
                let mut q = queue.load();
                if let Some(s) = q.sends.iter_mut().find(|s| s.id == send.id) {
                    s.status = SendStatus::Pending;
                }
                let _ = queue.save(&q);
                continue;
            }

            // Push manifest
            let manifest_json = match fs::read_to_string(&send.manifest_path) {
                Ok(json) => json,
                Err(e) => {
                    let _ = queue.mark_failed(&send.id, format!("Cannot read manifest: {}", e));
                    continue;
                }
            };

            let push_result = command_tx.send(NodeCommand::PushManifest {
                peer: peer_id,
                manifest_json,
                share_grant_json: send.share_grant_json.clone(),
            }).await;

            match push_result {
                Ok(_) => {
                    let _ = queue.mark_delivered(&send.id);
                }
                Err(_) => {
                    // Channel closed — node shut down
                    let mut q = queue.load();
                    if let Some(s) = q.sends.iter_mut().find(|s| s.id == send.id) {
                        s.status = SendStatus::Pending;
                    }
                    let _ = queue.save(&q);
                    // Stop processing — no point if node is down
                    break;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::mpsc;
    use std::fs;
    use crate::storage::ShardStore;
    use crate::storage::shard::{shard_data, Shard};

    #[tokio::test]
    async fn test_delivery_worker_processes_pending() {
        let uid = format!("{}-{}", std::process::id(), std::thread::current().name().unwrap_or("t"));
        let queue_path = std::env::temp_dir()
            .join(format!("nexus-delivery-test-{}.json", uid));
        let store_path = std::env::temp_dir()
            .join(format!("nexus-delivery-store-{}", uid));

        // Create a shard store with real data
        let store = ShardStore::open(&store_path).unwrap();
        let test_data = b"hello delivery worker test";
        let (_, shards) = shard_data(test_data, 1024);
        let cid_hex = store.put(&shards[0]).unwrap();

        // Create a manifest referencing the shard
        let manifest_path = std::env::temp_dir()
            .join(format!("nexus-delivery-manifest-{}.nexus", uid));
        let manifest_json = serde_json::json!({
            "owner": "did:nexus:test",
            "owner_pre_pk": { "bytes": [] },
            "shards": {
                "shards": [cid_hex],
                "shard_size": 1024,
                "total_size": test_data.len(),
                "filename": "test.txt"
            },
            "encrypted_dek": { "capsule": [], "ciphertext": [] }
        });
        fs::write(&manifest_path, manifest_json.to_string()).unwrap();

        // Create queue with a pending send
        let queue = SendQueue::open(&queue_path);
        queue.enqueue(
            "did:nexus:recipient".into(),
            "12D3KooWDpJ7As7BWAwRMfu1VU2WCqNjvq387JEYKDBj4kx6nXTN".into(),
            None,
            manifest_path.to_string_lossy().to_string(),
            "test.txt".into(),
            None,
            vec![cid_hex],
        ).unwrap();

        assert_eq!(queue.pending().len(), 1);

        // Create a channel to capture commands
        let (tx, mut rx) = mpsc::channel::<NodeCommand>(32);

        let config = DeliveryConfig {
            check_interval_secs: 1,
            max_attempts: 3,
            queue_path: queue_path.to_string_lossy().to_string(),
            store_path: store_path.to_string_lossy().to_string(),
        };

        // Spawn worker
        let handle = spawn_delivery_worker(tx, config);

        // Wait for the worker to process
        tokio::time::sleep(Duration::from_secs(3)).await;

        // Should have received PushShard + PushManifest commands
        let mut commands = Vec::new();
        while let Ok(cmd) = rx.try_recv() {
            commands.push(cmd);
        }

        // The worker should have attempted to push the shard and manifest
        assert!(commands.len() >= 2, "Expected at least 2 commands, got {}", commands.len());

        // Send should be marked delivered
        let all = queue.all();
        assert_eq!(all[0].status, SendStatus::Delivered);

        handle.abort();

        // Cleanup
        let _ = fs::remove_file(&queue_path);
        let _ = fs::remove_dir_all(&store_path);
        let _ = fs::remove_file(&manifest_path);
    }
}
