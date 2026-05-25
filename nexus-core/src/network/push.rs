//! Push session handler — manages authorized push transfers
//!
//! Sits between the network layer (NodeEvent) and the storage layer,
//! performing auth checks and managing push session state.

use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::access::{authorize_push, AuthError, ContactStore, FolderStore, GroupStore};

/// Unique identifier for an in-progress push session
pub type SessionId = String;

/// State of a push session (receiver side)
#[derive(Debug, Clone)]
pub struct PushSession {
    /// Session ID
    pub id: SessionId,
    /// Sender's DID
    pub sender_did: String,
    /// Target folder path
    pub target_folder: String,
    /// Original filename
    pub filename: String,
    /// Expected total size
    pub total_size: u64,
    /// Expected number of shards
    pub shard_count: usize,
    /// Manifest hash (for verification)
    pub manifest_hash: String,
    /// Shards received so far (index → (cid, data))
    pub received_shards: HashMap<usize, (String, Vec<u8>)>,
    /// When this session was created
    pub created_at: u64,
    /// Maximum session lifetime (seconds)
    pub timeout_secs: u64,
}

impl PushSession {
    /// Check if all shards have been received
    pub fn is_complete(&self) -> bool {
        self.received_shards.len() >= self.shard_count
    }

    /// Check if this session has timed out
    pub fn is_expired(&self) -> bool {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        now - self.created_at > self.timeout_secs
    }

    /// Total bytes received so far
    pub fn bytes_received(&self) -> u64 {
        self.received_shards.values().map(|(_, d)| d.len() as u64).sum()
    }
}

/// Manages active push sessions (receiver side)
#[derive(Debug)]
pub struct PushSessionManager {
    /// Active sessions by ID
    sessions: HashMap<SessionId, PushSession>,
    /// Nonce replay cache (nonces seen in the last 60 seconds)
    nonce_cache: HashMap<Vec<u8>, u64>,
    /// Maximum concurrent sessions
    max_sessions: usize,
    /// Maximum session timeout (seconds)
    session_timeout: u64,
    /// Rate limiter: (DID → (last_push_time, count_in_window))
    rate_limits: HashMap<String, (u64, u32)>,
    /// Max pushes per DID per window
    max_pushes_per_window: u32,
    /// Rate limit window (seconds)
    rate_window_secs: u64,
}

impl Default for PushSessionManager {
    fn default() -> Self {
        Self {
            sessions: HashMap::new(),
            nonce_cache: HashMap::new(),
            max_sessions: 16,
            session_timeout: 300, // 5 minutes per session
            rate_limits: HashMap::new(),
            max_pushes_per_window: 60,
            rate_window_secs: 3600, // 1 hour window
        }
    }
}

/// Errors that can occur during push handling
#[derive(Debug, thiserror::Error)]
pub enum PushError {
    #[error("auth failed: {0}")]
    AuthFailed(#[from] AuthError),

    #[error("request expired (timestamp too old)")]
    Expired,

    #[error("nonce replay detected")]
    NonceReplay,

    #[error("rate limit exceeded for {did}")]
    RateLimited { did: String },

    #[error("too many concurrent sessions ({max})")]
    TooManySessions { max: usize },

    #[error("unknown session: {id}")]
    UnknownSession { id: String },

    #[error("session expired: {id}")]
    SessionExpired { id: String },

    #[error("unexpected shard index {index} (expected < {expected})")]
    InvalidShardIndex { index: usize, expected: usize },

    #[error("duplicate shard index {index}")]
    DuplicateShard { index: usize },

    #[error("session not complete (have {have}/{need} shards)")]
    Incomplete { have: usize, need: usize },
}

impl PushSessionManager {
    pub fn new() -> Self {
        Self::default()
    }

    /// Configure max concurrent sessions
    pub fn with_max_sessions(mut self, max: usize) -> Self {
        self.max_sessions = max;
        self
    }

    /// Configure rate limits
    pub fn with_rate_limit(mut self, max_per_window: u32, window_secs: u64) -> Self {
        self.max_pushes_per_window = max_per_window;
        self.rate_window_secs = window_secs;
        self
    }

    /// Validate and accept a push request.
    /// Returns a session ID on success.
    pub fn accept_push(
        &mut self,
        sender_did: &str,
        target_folder: &str,
        filename: &str,
        total_size: u64,
        shard_count: usize,
        manifest_hash: &str,
        nonce: &[u8],
        timestamp: u64,
        contacts: &ContactStore,
        folders: &FolderStore,
        groups: &GroupStore,
    ) -> Result<SessionId, PushError> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        // 1. Timestamp check (60-second validity window)
        if now.saturating_sub(timestamp) > 60 {
            return Err(PushError::Expired);
        }

        // 2. Nonce replay check
        self.cleanup_nonces(now);
        if self.nonce_cache.contains_key(nonce) {
            return Err(PushError::NonceReplay);
        }

        // 3. Rate limit check
        if self.is_rate_limited(sender_did, now) {
            return Err(PushError::RateLimited {
                did: sender_did.to_string(),
            });
        }

        // 4. Auth check (contact known? permission sufficient?)
        authorize_push(sender_did, target_folder, None, contacts, folders, groups)?;

        // 5. Capacity check
        self.cleanup_expired_sessions();
        if self.sessions.len() >= self.max_sessions {
            return Err(PushError::TooManySessions {
                max: self.max_sessions,
            });
        }

        // All checks passed — create session
        let session_id = generate_session_id();

        let session = PushSession {
            id: session_id.clone(),
            sender_did: sender_did.to_string(),
            target_folder: target_folder.to_string(),
            filename: filename.to_string(),
            total_size,
            shard_count,
            manifest_hash: manifest_hash.to_string(),
            received_shards: HashMap::new(),
            created_at: now,
            timeout_secs: self.session_timeout,
        };

        self.sessions.insert(session_id.clone(), session);
        self.nonce_cache.insert(nonce.to_vec(), now);
        self.record_push(sender_did, now);

        Ok(session_id)
    }

    /// Receive a shard for an active session
    pub fn receive_shard(
        &mut self,
        session_id: &str,
        shard_index: usize,
        cid: String,
        data: Vec<u8>,
    ) -> Result<bool, PushError> {
        let session = self.sessions.get_mut(session_id)
            .ok_or_else(|| PushError::UnknownSession {
                id: session_id.to_string(),
            })?;

        if session.is_expired() {
            self.sessions.remove(session_id);
            return Err(PushError::SessionExpired {
                id: session_id.to_string(),
            });
        }

        if shard_index >= session.shard_count {
            return Err(PushError::InvalidShardIndex {
                index: shard_index,
                expected: session.shard_count,
            });
        }

        if session.received_shards.contains_key(&shard_index) {
            return Err(PushError::DuplicateShard { index: shard_index });
        }

        session.received_shards.insert(shard_index, (cid, data));

        Ok(session.is_complete())
    }

    /// Finalize a push session — returns the session data for storage
    pub fn finalize(&mut self, session_id: &str) -> Result<PushSession, PushError> {
        let session = self.sessions.get(session_id)
            .ok_or_else(|| PushError::UnknownSession {
                id: session_id.to_string(),
            })?;

        if !session.is_complete() {
            return Err(PushError::Incomplete {
                have: session.received_shards.len(),
                need: session.shard_count,
            });
        }

        // Remove from active sessions
        Ok(self.sessions.remove(session_id).unwrap())
    }

    /// Cancel/remove a session
    pub fn cancel(&mut self, session_id: &str) -> bool {
        self.sessions.remove(session_id).is_some()
    }

    /// Get an active session
    pub fn get(&self, session_id: &str) -> Option<&PushSession> {
        self.sessions.get(session_id)
    }

    /// Number of active sessions
    pub fn active_sessions(&self) -> usize {
        self.sessions.len()
    }

    // --- Internal helpers ---

    fn cleanup_nonces(&mut self, now: u64) {
        // Remove nonces older than 120 seconds (2x the validity window)
        self.nonce_cache.retain(|_, ts| now - *ts < 120);
    }

    fn cleanup_expired_sessions(&mut self) {
        self.sessions.retain(|_, s| !s.is_expired());
    }

    fn is_rate_limited(&self, did: &str, now: u64) -> bool {
        if let Some((window_start, count)) = self.rate_limits.get(did) {
            if now - window_start < self.rate_window_secs {
                return *count >= self.max_pushes_per_window;
            }
        }
        false
    }

    fn record_push(&mut self, did: &str, now: u64) {
        let entry = self.rate_limits
            .entry(did.to_string())
            .or_insert((now, 0));

        if now - entry.0 >= self.rate_window_secs {
            // Reset window
            *entry = (now, 1);
        } else {
            entry.1 += 1;
        }
    }
}

/// Generate a random session ID
fn generate_session_id() -> String {
    use std::fmt::Write;
    let mut id = String::with_capacity(32);
    let bytes: [u8; 16] = rand::random();
    for b in &bytes {
        let _ = write!(id, "{:02x}", b);
    }
    id
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::access::contact::Contact;
    use crate::access::folder::{AccessGrant, GrantTarget, Grantee, VaultFolder};
    use crate::access::permission::Permission;
    use tempfile::TempDir;

    fn setup(tmp: &std::path::Path) -> (ContactStore, FolderStore, GroupStore) {
        let mut contacts = ContactStore::open(tmp).unwrap();
        let mut folders = FolderStore::open(tmp).unwrap();
        let groups = GroupStore::open(tmp).unwrap();

        contacts.add(Contact {
            did: "did:nexus:alice".to_string(),
            label: "Alice".to_string(),
            peer_id: None,
            pre_pk: vec![],
            access: Permission::READ_WRITE,
            groups: vec![],
            created_at: 0,
            updated_at: 0,
        }).unwrap();

        contacts.add(Contact {
            did: "did:nexus:readonly".to_string(),
            label: "ReadOnly".to_string(),
            peer_id: None,
            pre_pk: vec![],
            access: Permission::READ,
            groups: vec![],
            created_at: 0,
            updated_at: 0,
        }).unwrap();

        folders.create_folder(VaultFolder {
            path: "/incoming".to_string(),
            label: Some("Incoming".to_string()),
            default_access: Permission::NONE,
            grants: vec![
                AccessGrant {
                    target: GrantTarget::Folder("/incoming".to_string()),
                    grantee: Grantee::Contact("did:nexus:alice".to_string()),
                    level: Permission::READ_WRITE,
                    expires: None,
                },
            ],
            inherit: false,
        }).unwrap();

        (contacts, folders, groups)
    }

    fn now_secs() -> u64 {
        SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs()
    }

    #[test]
    fn test_accept_push_success() {
        let tmp = TempDir::new().unwrap();
        let (contacts, folders, groups) = setup(tmp.path());

        let mut mgr = PushSessionManager::new();
        let result = mgr.accept_push(
            "did:nexus:alice",
            "/incoming",
            "test.bin",
            1024,
            4,
            "hash123",
            b"nonce1",
            now_secs(),
            &contacts,
            &folders,
            &groups,
        );
        assert!(result.is_ok());
        assert_eq!(mgr.active_sessions(), 1);
    }

    #[test]
    fn test_reject_unknown_contact() {
        let tmp = TempDir::new().unwrap();
        let (contacts, folders, groups) = setup(tmp.path());

        let mut mgr = PushSessionManager::new();
        let result = mgr.accept_push(
            "did:nexus:unknown",
            "/incoming",
            "test.bin",
            1024,
            4,
            "hash123",
            b"nonce1",
            now_secs(),
            &contacts,
            &folders,
            &groups,
        );
        assert!(matches!(result, Err(PushError::AuthFailed(_))));
    }

    #[test]
    fn test_reject_insufficient_permission() {
        let tmp = TempDir::new().unwrap();
        let (contacts, folders, groups) = setup(tmp.path());

        let mut mgr = PushSessionManager::new();
        let result = mgr.accept_push(
            "did:nexus:readonly",
            "/incoming",
            "test.bin",
            1024,
            4,
            "hash123",
            b"nonce1",
            now_secs(),
            &contacts,
            &folders,
            &groups,
        );
        assert!(matches!(result, Err(PushError::AuthFailed(_))));
    }

    #[test]
    fn test_reject_expired_timestamp() {
        let tmp = TempDir::new().unwrap();
        let (contacts, folders, groups) = setup(tmp.path());

        let mut mgr = PushSessionManager::new();
        let old_timestamp = now_secs() - 120; // 2 minutes ago
        let result = mgr.accept_push(
            "did:nexus:alice",
            "/incoming",
            "test.bin",
            1024,
            4,
            "hash123",
            b"nonce1",
            old_timestamp,
            &contacts,
            &folders,
            &groups,
        );
        assert!(matches!(result, Err(PushError::Expired)));
    }

    #[test]
    fn test_reject_nonce_replay() {
        let tmp = TempDir::new().unwrap();
        let (contacts, folders, groups) = setup(tmp.path());

        let mut mgr = PushSessionManager::new();
        let nonce = b"unique-nonce-123";

        // First push succeeds
        mgr.accept_push(
            "did:nexus:alice", "/incoming", "a.bin", 100, 1, "h1",
            nonce, now_secs(), &contacts, &folders, &groups,
        ).unwrap();

        // Same nonce rejected
        let result = mgr.accept_push(
            "did:nexus:alice", "/incoming", "b.bin", 100, 1, "h2",
            nonce, now_secs(), &contacts, &folders, &groups,
        );
        assert!(matches!(result, Err(PushError::NonceReplay)));
    }

    #[test]
    fn test_shard_reception() {
        let tmp = TempDir::new().unwrap();
        let (contacts, folders, groups) = setup(tmp.path());

        let mut mgr = PushSessionManager::new();
        let session_id = mgr.accept_push(
            "did:nexus:alice", "/incoming", "test.bin", 1024, 3, "hash",
            b"nonce-rx", now_secs(), &contacts, &folders, &groups,
        ).unwrap();

        // Receive shards
        assert!(!mgr.receive_shard(&session_id, 0, "cid0".into(), vec![1; 100]).unwrap());
        assert!(!mgr.receive_shard(&session_id, 1, "cid1".into(), vec![2; 100]).unwrap());
        assert!(mgr.receive_shard(&session_id, 2, "cid2".into(), vec![3; 100]).unwrap()); // complete!

        // Finalize
        let session = mgr.finalize(&session_id).unwrap();
        assert_eq!(session.received_shards.len(), 3);
        assert_eq!(mgr.active_sessions(), 0);
    }

    #[test]
    fn test_reject_duplicate_shard() {
        let tmp = TempDir::new().unwrap();
        let (contacts, folders, groups) = setup(tmp.path());

        let mut mgr = PushSessionManager::new();
        let session_id = mgr.accept_push(
            "did:nexus:alice", "/incoming", "test.bin", 1024, 3, "hash",
            b"nonce-dup", now_secs(), &contacts, &folders, &groups,
        ).unwrap();

        mgr.receive_shard(&session_id, 0, "cid0".into(), vec![1]).unwrap();
        let result = mgr.receive_shard(&session_id, 0, "cid0".into(), vec![1]);
        assert!(matches!(result, Err(PushError::DuplicateShard { index: 0 })));
    }

    #[test]
    fn test_max_sessions() {
        let tmp = TempDir::new().unwrap();
        let (contacts, folders, groups) = setup(tmp.path());

        let mut mgr = PushSessionManager::new().with_max_sessions(2);

        mgr.accept_push(
            "did:nexus:alice", "/incoming", "a.bin", 100, 1, "h1",
            b"n1", now_secs(), &contacts, &folders, &groups,
        ).unwrap();

        mgr.accept_push(
            "did:nexus:alice", "/incoming", "b.bin", 100, 1, "h2",
            b"n2", now_secs(), &contacts, &folders, &groups,
        ).unwrap();

        let result = mgr.accept_push(
            "did:nexus:alice", "/incoming", "c.bin", 100, 1, "h3",
            b"n3", now_secs(), &contacts, &folders, &groups,
        );
        assert!(matches!(result, Err(PushError::TooManySessions { max: 2 })));
    }
}
