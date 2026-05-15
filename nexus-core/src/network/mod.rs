//! Network module — libp2p peer-to-peer networking
//!
//! Provides:
//! - Node identity derived from Ed25519 keypair
//! - Swarm with Kademlia DHT, GossipSub, Request-Response
//! - mDNS for local peer discovery
//! - QUIC + TCP transports
//! - NAT traversal via relay, DCUtR hole-punching, and AutoNAT
//! - Connectivity telemetry for diagnosing real-world failures

pub mod node;
pub mod behaviour;
pub mod protocol;
pub mod send_queue;
pub mod delivery;
pub mod telemetry;
pub mod relay_server;

pub use node::{NexusNode, NodeConfig, NodeEvent, NodeCommand};
pub use behaviour::NexusBehaviour;
pub use send_queue::{SendQueue, QueuedSend, SendStatus};
pub use delivery::{spawn_delivery_worker, DeliveryConfig};
pub use telemetry::{TelemetryCollector, TelemetryStats, ConnectivityEvent, NatStatus};
pub use relay_server::{RelayServer, RelayConfig, RelayServerEvent, detect_public_ip};
pub use libp2p::PeerId;
pub use libp2p::Multiaddr;
pub use libp2p::identity::Keypair as Libp2pKeypair;
