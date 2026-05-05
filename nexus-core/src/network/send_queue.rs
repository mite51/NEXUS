//! Send Queue — local queue for outbound file transfers
//!
//! When a send is initiated and the recipient is offline, the transfer
//! is queued locally. A background task periodically checks if the peer
//! becomes reachable, and pushes shards + manifest when it does.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Status of a queued send
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum SendStatus {
    /// Waiting for recipient to come online
    Pending,
    /// Currently pushing shards
    InProgress,
    /// Successfully delivered
    Delivered,
    /// Failed after retries
    Failed { reason: String },
}

/// A queued outbound transfer
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueuedSend {
    /// Unique ID for this send
    pub id: String,
    /// Recipient's DID (for display)
    pub recipient_did: String,
    /// Recipient's PeerId (for network connection)
    pub recipient_peer_id: String,
    /// Last known multiaddr for the recipient (optional)
    pub recipient_addr: Option<String>,
    /// Path to the .nexus manifest file
    pub manifest_path: String,
    /// Original filename (for display)
    pub filename: String,
    /// Share grant JSON (if sharing via PRE, otherwise None)
    pub share_grant_json: Option<String>,
    /// Shard CIDs to push
    pub shard_cids: Vec<String>,
    /// Current status
    pub status: SendStatus,
    /// When the send was queued (unix timestamp ms)
    pub queued_at: u64,
    /// When it was last attempted (unix timestamp ms)
    pub last_attempt: Option<u64>,
    /// Number of delivery attempts
    pub attempts: u32,
}

/// The on-disk send queue
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SendQueueFile {
    pub sends: Vec<QueuedSend>,
}

/// Manages the send queue
pub struct SendQueue {
    path: PathBuf,
}

impl SendQueue {
    /// Open or create a send queue at the given path
    pub fn open(path: impl AsRef<Path>) -> Self {
        Self {
            path: path.as_ref().to_path_buf(),
        }
    }

    /// Load the queue from disk
    pub fn load(&self) -> SendQueueFile {
        fs::read_to_string(&self.path)
            .ok()
            .and_then(|json| serde_json::from_str(&json).ok())
            .unwrap_or_default()
    }

    /// Save the queue to disk
    pub fn save(&self, queue: &SendQueueFile) -> Result<(), String> {
        let json = serde_json::to_string_pretty(queue)
            .map_err(|e| format!("Serialization failed: {}", e))?;
        fs::write(&self.path, json)
            .map_err(|e| format!("Failed to write queue: {}", e))?;
        Ok(())
    }

    /// Add a new send to the queue
    pub fn enqueue(
        &self,
        recipient_did: String,
        recipient_peer_id: String,
        recipient_addr: Option<String>,
        manifest_path: String,
        filename: String,
        share_grant_json: Option<String>,
        shard_cids: Vec<String>,
    ) -> Result<QueuedSend, String> {
        let mut queue = self.load();

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        let id = format!("{:x}", now);

        let send = QueuedSend {
            id: id.clone(),
            recipient_did,
            recipient_peer_id,
            recipient_addr,
            manifest_path,
            filename,
            share_grant_json,
            shard_cids,
            status: SendStatus::Pending,
            queued_at: now,
            last_attempt: None,
            attempts: 0,
        };

        queue.sends.push(send.clone());
        self.save(&queue)?;
        Ok(send)
    }

    /// Get all pending sends
    pub fn pending(&self) -> Vec<QueuedSend> {
        self.load()
            .sends
            .into_iter()
            .filter(|s| s.status == SendStatus::Pending)
            .collect()
    }

    /// Mark a send as in-progress
    pub fn mark_in_progress(&self, id: &str) -> Result<(), String> {
        let mut queue = self.load();
        if let Some(send) = queue.sends.iter_mut().find(|s| s.id == id) {
            send.status = SendStatus::InProgress;
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64;
            send.last_attempt = Some(now);
            send.attempts += 1;
        }
        self.save(&queue)
    }

    /// Mark a send as delivered
    pub fn mark_delivered(&self, id: &str) -> Result<(), String> {
        let mut queue = self.load();
        if let Some(send) = queue.sends.iter_mut().find(|s| s.id == id) {
            send.status = SendStatus::Delivered;
        }
        self.save(&queue)
    }

    /// Mark a send as failed
    pub fn mark_failed(&self, id: &str, reason: String) -> Result<(), String> {
        let mut queue = self.load();
        if let Some(send) = queue.sends.iter_mut().find(|s| s.id == id) {
            send.status = SendStatus::Failed { reason };
        }
        self.save(&queue)
    }

    /// Reset a failed send back to pending (for manual retry)
    pub fn retry(&self, id: &str) -> Result<(), String> {
        let mut queue = self.load();
        if let Some(send) = queue.sends.iter_mut().find(|s| s.id == id) {
            send.status = SendStatus::Pending;
        }
        self.save(&queue)
    }

    /// Remove a send from the queue (e.g., after delivery or cancellation)
    pub fn remove(&self, id: &str) -> Result<(), String> {
        let mut queue = self.load();
        queue.sends.retain(|s| s.id != id);
        self.save(&queue)
    }

    /// Get all sends (any status)
    pub fn all(&self) -> Vec<QueuedSend> {
        self.load().sends
    }

    /// Maximum number of delivery attempts before marking as permanently failed
    pub const MAX_ATTEMPTS: u32 = 5;

    /// Get sends that are ready for retry (pending + backoff elapsed)
    /// Uses exponential backoff: 30s, 60s, 120s, 240s, 480s
    pub fn ready_for_retry(&self) -> Vec<QueuedSend> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        self.load()
            .sends
            .into_iter()
            .filter(|s| s.status == SendStatus::Pending)
            .filter(|s| {
                match s.last_attempt {
                    None => true, // Never attempted
                    Some(last) => {
                        let backoff_ms = 30_000u64 * (1u64 << s.attempts.min(4));
                        now.saturating_sub(last) >= backoff_ms
                    }
                }
            })
            .collect()
    }

    /// Auto-fail sends that exceed MAX_ATTEMPTS
    pub fn expire_stale(&self) -> Result<Vec<String>, String> {
        let mut queue = self.load();
        let mut expired = Vec::new();
        for send in queue.sends.iter_mut() {
            if send.status == SendStatus::Pending && send.attempts >= Self::MAX_ATTEMPTS {
                send.status = SendStatus::Failed {
                    reason: format!("Gave up after {} attempts", send.attempts),
                };
                expired.push(send.id.clone());
            }
        }
        if !expired.is_empty() {
            self.save(&queue)?;
        }
        Ok(expired)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn temp_queue() -> (SendQueue, PathBuf) {
        let path = std::env::temp_dir().join(format!("nexus-send-queue-test-{}.json", std::process::id()));
        let _ = fs::remove_file(&path);
        (SendQueue::open(&path), path)
    }

    #[test]
    fn test_enqueue_and_list() {
        let (queue, path) = temp_queue();

        let send = queue.enqueue(
            "did:nexus:abc123".into(),
            "12D3KooWTest".into(),
            Some("/ip4/192.168.1.5/tcp/9000".into()),
            "./test.nexus".into(),
            "test.txt".into(),
            None,
            vec!["cid1".into(), "cid2".into()],
        ).unwrap();

        assert_eq!(send.status, SendStatus::Pending);
        assert_eq!(queue.pending().len(), 1);
        assert_eq!(queue.all().len(), 1);

        let _ = fs::remove_file(path);
    }

    #[test]
    fn test_status_transitions() {
        let (queue, path) = temp_queue();

        let send = queue.enqueue(
            "did:nexus:bob".into(),
            "12D3KooWBob".into(),
            None,
            "./file.nexus".into(),
            "file.pdf".into(),
            None,
            vec!["shard1".into()],
        ).unwrap();

        queue.mark_in_progress(&send.id).unwrap();
        let all = queue.all();
        assert_eq!(all[0].status, SendStatus::InProgress);
        assert_eq!(all[0].attempts, 1);

        queue.mark_delivered(&send.id).unwrap();
        let all = queue.all();
        assert_eq!(all[0].status, SendStatus::Delivered);

        let _ = fs::remove_file(path);
    }

    #[test]
    fn test_failed_and_retry() {
        let (queue, path) = temp_queue();

        let send = queue.enqueue(
            "did:nexus:carol".into(),
            "12D3KooWCarol".into(),
            None,
            "./big.nexus".into(),
            "big.zip".into(),
            None,
            vec!["s1".into(), "s2".into(), "s3".into()],
        ).unwrap();

        queue.mark_failed(&send.id, "Connection refused".into()).unwrap();
        assert_eq!(queue.pending().len(), 0);

        queue.retry(&send.id).unwrap();
        assert_eq!(queue.pending().len(), 1);

        let _ = fs::remove_file(path);
    }

    #[test]
    fn test_remove() {
        let (queue, path) = temp_queue();

        let send = queue.enqueue(
            "did:nexus:dave".into(),
            "12D3KooWDave".into(),
            None,
            "./doc.nexus".into(),
            "doc.md".into(),
            None,
            vec!["c1".into()],
        ).unwrap();

        assert_eq!(queue.all().len(), 1);
        queue.remove(&send.id).unwrap();
        assert_eq!(queue.all().len(), 0);

        let _ = fs::remove_file(path);
    }
}
