//! Composite network behaviour for NEXUS nodes

use libp2p::{
    autonat, dcutr, gossipsub, identify, kad, mdns, relay,
    request_response::{self, ProtocolSupport},
    swarm::NetworkBehaviour,
};
use std::time::Duration;

use super::protocol::{NexusCodec, NexusProtocol};

/// Combined network behaviour for a NEXUS node
#[derive(NetworkBehaviour)]
pub struct NexusBehaviour {
    /// Kademlia DHT — peer discovery and DID resolution
    pub kademlia: kad::Behaviour<kad::store::MemoryStore>,
    /// GossipSub — feed pub/sub
    pub gossipsub: gossipsub::Behaviour,
    /// Request-Response — shard exchange + kfrag delivery
    pub request_response: request_response::Behaviour<NexusCodec>,
    /// mDNS — local network peer discovery
    pub mdns: mdns::tokio::Behaviour,
    /// Identify — exchange peer metadata on connect
    pub identify: identify::Behaviour,
    /// Relay client — for NAT traversal fallback
    pub relay_client: relay::client::Behaviour,
    /// DCUtR — Direct Connection Upgrade through Relay (hole punching)
    pub dcutr: dcutr::Behaviour,
    /// AutoNAT — detect whether we're behind NAT
    pub autonat: autonat::Behaviour,
}

impl NexusBehaviour {
    /// Create a new NexusBehaviour with default configs
    pub fn new(
        local_peer_id: libp2p::PeerId,
        keypair: &libp2p::identity::Keypair,
        relay_behaviour: relay::client::Behaviour,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        // Kademlia DHT
        let store = kad::store::MemoryStore::new(local_peer_id);
        let mut kademlia = kad::Behaviour::new(local_peer_id, store);
        kademlia.set_mode(Some(kad::Mode::Server));

        // GossipSub
        let gossipsub_config = gossipsub::ConfigBuilder::default()
            .heartbeat_interval(Duration::from_secs(10))
            .validation_mode(gossipsub::ValidationMode::Strict)
            .build()
            .map_err(|e| format!("gossipsub config: {}", e))?;

        let gossipsub = gossipsub::Behaviour::new(
            gossipsub::MessageAuthenticity::Signed(keypair.clone()),
            gossipsub_config,
        )
        .map_err(|e| format!("gossipsub: {}", e))?;

        // Request-Response (NEXUS protocol)
        let request_response = request_response::Behaviour::new(
            [(NexusProtocol, ProtocolSupport::Full)],
            request_response::Config::default()
                .with_request_timeout(Duration::from_secs(30)),
        );

        // mDNS (local discovery)
        let mdns = mdns::tokio::Behaviour::new(
            mdns::Config::default(),
            local_peer_id,
        )?;

        // Identify
        let identify = identify::Behaviour::new(
            identify::Config::new(
                "/nexus/id/1.0.0".to_string(),
                keypair.public(),
            )
            .with_push_listen_addr_updates(true),
        );

        // DCUtR — allows hole punching through relays
        let dcutr = dcutr::Behaviour::new(local_peer_id);

        // AutoNAT — probe whether we're reachable from the internet
        let autonat = autonat::Behaviour::new(
            local_peer_id,
            autonat::Config {
                retry_interval: Duration::from_secs(60),
                refresh_interval: Duration::from_secs(300),
                confidence_max: 3,
                throttle_server_period: Duration::from_secs(15),
                ..Default::default()
            },
        );

        Ok(Self {
            kademlia,
            gossipsub,
            request_response,
            mdns,
            identify,
            relay_client: relay_behaviour,
            dcutr,
            autonat,
        })
    }
}
