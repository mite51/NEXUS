//! Network module — libp2p peer-to-peer networking
//!
//! Provides:
//! - Node identity derived from Ed25519 keypair
//! - Swarm with Kademlia DHT, GossipSub, Request-Response
//! - mDNS for local peer discovery
//! - QUIC + TCP transports

pub mod node;
pub mod behaviour;
pub mod protocol;
pub mod send_queue;
pub mod delivery;

pub use node::{NexusNode, NodeConfig, NodeEvent, NodeCommand};
pub use behaviour::NexusBehaviour;
pub use send_queue::{SendQueue, QueuedSend, SendStatus};
pub use delivery::{spawn_delivery_worker, DeliveryConfig};
