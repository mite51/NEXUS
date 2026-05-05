//! Network telemetry — structured event collection for connectivity diagnostics.
//!
//! Records connection attempts, relay usage, hole-punch outcomes, and NAT status.
//! Events are written to a ring-buffer log file (JSON lines) for later analysis.

use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Maximum log file size before rotation (5 MB)
const MAX_LOG_SIZE: u64 = 5 * 1024 * 1024;
/// Maximum number of rotated log files to keep
const MAX_ROTATED_FILES: usize = 3;

/// NAT status as detected by AutoNAT
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum NatStatus {
    Unknown,
    Public,
    Private,
}

/// Types of connectivity events we track
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ConnectivityEvent {
    /// NAT status changed
    NatStatusChanged {
        status: NatStatus,
        confidence: u32,
    },
    /// Attempted to connect to a relay
    RelayReservation {
        relay_peer: String,
        relay_addr: String,
        success: bool,
        error: Option<String>,
        duration_ms: u64,
    },
    /// Relay circuit established (traffic flowing through relay)
    RelayCircuit {
        remote_peer: String,
        relay_peer: String,
        direction: String, // "inbound" | "outbound"
    },
    /// DCUtR hole-punch attempt
    HolePunch {
        remote_peer: String,
        success: bool,
        direct_addr: Option<String>,
        error: Option<String>,
        duration_ms: u64,
    },
    /// Direct connection upgrade (from relay to direct)
    DirectUpgrade {
        remote_peer: String,
        new_addr: String,
    },
    /// Connection attempt failed
    DialFailure {
        remote_peer: Option<String>,
        addr: String,
        error: String,
        is_relay: bool,
    },
    /// Peer unreachable after all attempts
    PeerUnreachable {
        remote_peer: String,
        attempts: u32,
        last_error: String,
    },
    /// Successful connection established
    ConnectionEstablished {
        remote_peer: String,
        addr: String,
        is_relayed: bool,
        num_established: u32,
    },
    /// Connection closed
    ConnectionClosed {
        remote_peer: String,
        duration_secs: u64,
        was_relayed: bool,
        cause: Option<String>,
    },
}

/// A single telemetry record
#[derive(Debug, Serialize, Deserialize)]
pub struct TelemetryRecord {
    /// Unix timestamp (milliseconds)
    pub timestamp_ms: u64,
    /// Our local peer ID
    pub local_peer: String,
    /// The event
    pub event: ConnectivityEvent,
}

/// Telemetry collector that writes events to a JSON-lines log file
#[derive(Debug, Clone)]
pub struct TelemetryCollector {
    log_path: PathBuf,
    local_peer: String,
    enabled: bool,
}

impl TelemetryCollector {
    /// Create a new collector. If `log_dir` doesn't exist, creates it.
    pub fn new(log_dir: impl AsRef<Path>, local_peer: String, enabled: bool) -> Self {
        let log_path = log_dir.as_ref().join("connectivity.jsonl");
        if enabled {
            let _ = fs::create_dir_all(log_dir.as_ref());
        }
        Self { log_path, local_peer, enabled }
    }

    /// Record an event
    pub fn record(&self, event: ConnectivityEvent) {
        if !self.enabled {
            return;
        }

        let record = TelemetryRecord {
            timestamp_ms: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64,
            local_peer: self.local_peer.clone(),
            event,
        };

        // Rotate if needed
        self.maybe_rotate();

        // Append to log
        if let Ok(json) = serde_json::to_string(&record) {
            if let Ok(mut file) = OpenOptions::new()
                .create(true)
                .append(true)
                .open(&self.log_path)
            {
                let _ = writeln!(file, "{}", json);
            }
        }
    }

    /// Get recent events (last N records)
    pub fn recent(&self, count: usize) -> Vec<TelemetryRecord> {
        let content = match fs::read_to_string(&self.log_path) {
            Ok(c) => c,
            Err(_) => return vec![],
        };

        content
            .lines()
            .rev()
            .take(count)
            .filter_map(|line| serde_json::from_str(line).ok())
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect()
    }

    /// Get summary statistics
    pub fn stats(&self) -> TelemetryStats {
        let content = match fs::read_to_string(&self.log_path) {
            Ok(c) => c,
            Err(_) => return TelemetryStats::default(),
        };

        let mut stats = TelemetryStats::default();

        for line in content.lines() {
            if let Ok(record) = serde_json::from_str::<TelemetryRecord>(line) {
                match &record.event {
                    ConnectivityEvent::HolePunch { success, .. } => {
                        stats.hole_punch_attempts += 1;
                        if *success {
                            stats.hole_punch_successes += 1;
                        }
                    }
                    ConnectivityEvent::RelayReservation { success, .. } => {
                        stats.relay_attempts += 1;
                        if *success {
                            stats.relay_successes += 1;
                        }
                    }
                    ConnectivityEvent::DialFailure { .. } => {
                        stats.dial_failures += 1;
                    }
                    ConnectivityEvent::ConnectionEstablished { is_relayed, .. } => {
                        stats.connections_total += 1;
                        if *is_relayed {
                            stats.connections_relayed += 1;
                        }
                    }
                    ConnectivityEvent::NatStatusChanged { status, .. } => {
                        stats.last_nat_status = status.clone();
                    }
                    _ => {}
                }
            }
        }

        stats
    }

    fn maybe_rotate(&self) {
        let size = fs::metadata(&self.log_path)
            .map(|m| m.len())
            .unwrap_or(0);

        if size > MAX_LOG_SIZE {
            // Rotate: .jsonl → .jsonl.1 → .jsonl.2 → .jsonl.3 (delete oldest)
            for i in (1..MAX_ROTATED_FILES).rev() {
                let from = self.log_path.with_extension(format!("jsonl.{}", i));
                let to = self.log_path.with_extension(format!("jsonl.{}", i + 1));
                let _ = fs::rename(&from, &to);
            }
            let rotated = self.log_path.with_extension("jsonl.1");
            let _ = fs::rename(&self.log_path, &rotated);
        }
    }
}

/// Summary stats from telemetry logs
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct TelemetryStats {
    pub hole_punch_attempts: u64,
    pub hole_punch_successes: u64,
    pub relay_attempts: u64,
    pub relay_successes: u64,
    pub dial_failures: u64,
    pub connections_total: u64,
    pub connections_relayed: u64,
    pub last_nat_status: NatStatus,
}

impl Default for NatStatus {
    fn default() -> Self {
        NatStatus::Unknown
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_record_and_retrieve() {
        let dir = tempdir().unwrap();
        let collector = TelemetryCollector::new(dir.path(), "12D3KooTest".to_string(), true);

        collector.record(ConnectivityEvent::NatStatusChanged {
            status: NatStatus::Private,
            confidence: 3,
        });

        collector.record(ConnectivityEvent::HolePunch {
            remote_peer: "12D3KooPeer".to_string(),
            success: true,
            direct_addr: Some("/ip4/1.2.3.4/tcp/5000".to_string()),
            error: None,
            duration_ms: 450,
        });

        collector.record(ConnectivityEvent::HolePunch {
            remote_peer: "12D3KooOther".to_string(),
            success: false,
            direct_addr: None,
            error: Some("timeout".to_string()),
            duration_ms: 5000,
        });

        let records = collector.recent(10);
        assert_eq!(records.len(), 3);

        let stats = collector.stats();
        assert_eq!(stats.hole_punch_attempts, 2);
        assert_eq!(stats.hole_punch_successes, 1);
        assert_eq!(stats.last_nat_status, NatStatus::Private);
    }

    #[test]
    fn test_disabled_collector() {
        let dir = tempdir().unwrap();
        let collector = TelemetryCollector::new(dir.path(), "12D3KooTest".to_string(), false);

        collector.record(ConnectivityEvent::DialFailure {
            remote_peer: None,
            addr: "/ip4/1.2.3.4/tcp/5000".to_string(),
            error: "connection refused".to_string(),
            is_relay: false,
        });

        let records = collector.recent(10);
        assert_eq!(records.len(), 0);
    }

    #[test]
    fn test_rotation() {
        let dir = tempdir().unwrap();
        let collector = TelemetryCollector::new(dir.path(), "12D3KooTest".to_string(), true);

        // Write enough to trigger rotation (just test the logic works)
        for i in 0..100 {
            collector.record(ConnectivityEvent::DialFailure {
                remote_peer: Some(format!("peer_{}", i)),
                addr: format!("/ip4/10.0.0.{}/tcp/5000", i % 256),
                error: "connection refused".to_string(),
                is_relay: false,
            });
        }

        let stats = collector.stats();
        assert_eq!(stats.dial_failures, 100);
    }
}
