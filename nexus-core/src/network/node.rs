//! NEXUS Node — the main networking entry point
//!
//! Manages a libp2p swarm with all NEXUS protocols.
//! Can be started as a background task and communicated with via channels.

use libp2p::{
    identity::Keypair, Multiaddr, PeerId, Swarm, SwarmBuilder,
    noise, tcp, yamux,
    swarm::SwarmEvent,
    gossipsub,
    request_response,
    mdns,
};
use futures::StreamExt;
use std::time::Duration;
use tokio::sync::mpsc;

use super::behaviour::{NexusBehaviour, NexusBehaviourEvent};
use super::protocol::{NexusRequest, NexusResponse};

/// Configuration for starting a NEXUS node
#[derive(Debug, Clone)]
pub struct NodeConfig {
    /// Listen addresses (e.g., "/ip4/0.0.0.0/udp/0/quic-v1")
    pub listen_addrs: Vec<String>,
    /// Bootstrap peers (for joining the network)
    pub bootstrap_peers: Vec<(PeerId, Multiaddr)>,
    /// Whether to enable mDNS for local discovery
    pub mdns_enabled: bool,
}

impl Default for NodeConfig {
    fn default() -> Self {
        Self {
            listen_addrs: vec![
                "/ip4/0.0.0.0/udp/0/quic-v1".to_string(),
                "/ip4/0.0.0.0/tcp/0".to_string(),
            ],
            bootstrap_peers: vec![],
            mdns_enabled: true,
        }
    }
}

/// Events emitted by the NEXUS node for the application layer
#[derive(Debug)]
pub enum NodeEvent {
    /// We're listening on an address
    Listening(Multiaddr),
    /// A new peer was discovered
    PeerDiscovered(PeerId),
    /// A peer disconnected
    PeerDisconnected(PeerId),
    /// Received a shard request from a peer
    ShardRequested {
        peer: PeerId,
        cid: String,
        channel: request_response::ResponseChannel<NexusResponse>,
    },
    /// Received a shard response (data or not-found)
    ShardReceived {
        peer: PeerId,
        response: NexusResponse,
    },
    /// Received kfrags from another peer
    KfragsReceived {
        peer: PeerId,
        manifest_id: String,
        kfrags: Vec<Vec<u8>>,
        verifying_key: Vec<u8>,
        sender_pre_pk: Vec<u8>,
    },
    /// A GossipSub message was received
    GossipMessage {
        topic: String,
        data: Vec<u8>,
        source: Option<PeerId>,
    },
}

/// Commands that can be sent to the node
#[derive(Debug)]
pub enum NodeCommand {
    /// Dial a peer at an address
    Dial(Multiaddr),
    /// Request a shard from a peer
    RequestShard { peer: PeerId, cid: String },
    /// Send kfrags to a peer
    SendKfrags {
        peer: PeerId,
        manifest_id: String,
        kfrags: Vec<Vec<u8>>,
        verifying_key: Vec<u8>,
        sender_pre_pk: Vec<u8>,
    },
    /// Publish a message to a GossipSub topic
    Publish { topic: String, data: Vec<u8> },
    /// Subscribe to a GossipSub topic
    Subscribe(String),
    /// Respond to a shard request
    RespondShard {
        channel: request_response::ResponseChannel<NexusResponse>,
        response: NexusResponse,
    },
    /// Get our listening addresses
    GetListeningAddrs(tokio::sync::oneshot::Sender<Vec<Multiaddr>>),
    /// Shutdown
    Shutdown,
}

/// The NEXUS network node
pub struct NexusNode {
    /// Our peer ID
    pub peer_id: PeerId,
    /// Channel to send commands to the node
    pub command_tx: mpsc::Sender<NodeCommand>,
    /// Channel to receive events from the node
    pub event_rx: mpsc::Receiver<NodeEvent>,
}

impl NexusNode {
    /// Start a new NEXUS node with the given identity keypair and config
    pub async fn start(
        keypair: Keypair,
        config: NodeConfig,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let local_peer_id = keypair.public().to_peer_id();

        // Build the swarm
        let mut swarm = SwarmBuilder::with_existing_identity(keypair.clone())
            .with_tokio()
            .with_tcp(
                tcp::Config::default(),
                noise::Config::new,
                yamux::Config::default,
            )?
            .with_quic()
            .with_relay_client(noise::Config::new, yamux::Config::default)?
            .with_behaviour(|key, relay_behaviour| {
                NexusBehaviour::new(local_peer_id, key, relay_behaviour)
                    .expect("behaviour creation should succeed")
            })?
            .with_swarm_config(|cfg| {
                cfg.with_idle_connection_timeout(Duration::from_secs(60))
            })
            .build();

        // Listen on configured addresses
        for addr_str in &config.listen_addrs {
            let addr: Multiaddr = addr_str.parse()
                .map_err(|e| format!("invalid listen addr '{}': {}", addr_str, e))?;
            swarm.listen_on(addr)?;
        }

        // Add bootstrap peers to Kademlia
        for (peer_id, addr) in &config.bootstrap_peers {
            swarm.behaviour_mut().kademlia.add_address(peer_id, addr.clone());
        }

        // Bootstrap Kademlia if we have peers
        if !config.bootstrap_peers.is_empty() {
            swarm.behaviour_mut().kademlia.bootstrap()?;
        }

        // Create channels
        let (command_tx, mut command_rx) = mpsc::channel::<NodeCommand>(256);
        let (event_tx, event_rx) = mpsc::channel::<NodeEvent>(256);

        // Spawn the event loop
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    // Handle commands from the application
                    Some(cmd) = command_rx.recv() => {
                        match cmd {
                            NodeCommand::Dial(addr) => {
                                let _ = swarm.dial(addr);
                            }
                            NodeCommand::RequestShard { peer, cid } => {
                                swarm.behaviour_mut().request_response.send_request(
                                    &peer,
                                    NexusRequest::GetShard { cid },
                                );
                            }
                            NodeCommand::SendKfrags { peer, manifest_id, kfrags, verifying_key, sender_pre_pk } => {
                                swarm.behaviour_mut().request_response.send_request(
                                    &peer,
                                    NexusRequest::DeliverKfrags {
                                        manifest_id,
                                        kfrags,
                                        verifying_key,
                                        sender_pre_pk,
                                    },
                                );
                            }
                            NodeCommand::Publish { topic, data } => {
                                let topic = gossipsub::IdentTopic::new(topic);
                                let _ = swarm.behaviour_mut().gossipsub.publish(topic, data);
                            }
                            NodeCommand::Subscribe(topic) => {
                                let topic = gossipsub::IdentTopic::new(topic);
                                let _ = swarm.behaviour_mut().gossipsub.subscribe(&topic);
                            }
                            NodeCommand::RespondShard { channel, response } => {
                                let _ = swarm.behaviour_mut().request_response.send_response(channel, response);
                            }
                            NodeCommand::GetListeningAddrs(tx) => {
                                let addrs: Vec<Multiaddr> = swarm.listeners().cloned().collect();
                                let _ = tx.send(addrs);
                            }
                            NodeCommand::Shutdown => break,
                        }
                    }
                    // Handle swarm events
                    event = swarm.select_next_some() => {
                        handle_swarm_event(&mut swarm, event, &event_tx).await;
                    }
                }
            }
        });

        Ok(Self {
            peer_id: local_peer_id,
            command_tx,
            event_rx,
        })
    }

    /// Get our PeerId
    pub fn peer_id(&self) -> &PeerId {
        &self.peer_id
    }

    /// Dial a peer
    pub async fn dial(&self, addr: Multiaddr) -> Result<(), mpsc::error::SendError<NodeCommand>> {
        self.command_tx.send(NodeCommand::Dial(addr)).await
    }

    /// Request a shard from a specific peer
    pub async fn request_shard(&self, peer: PeerId, cid: String) -> Result<(), mpsc::error::SendError<NodeCommand>> {
        self.command_tx.send(NodeCommand::RequestShard { peer, cid }).await
    }

    /// Send kfrags to a recipient peer
    pub async fn send_kfrags(
        &self,
        peer: PeerId,
        manifest_id: String,
        kfrags: Vec<Vec<u8>>,
        verifying_key: Vec<u8>,
        sender_pre_pk: Vec<u8>,
    ) -> Result<(), mpsc::error::SendError<NodeCommand>> {
        self.command_tx.send(NodeCommand::SendKfrags {
            peer, manifest_id, kfrags, verifying_key, sender_pre_pk,
        }).await
    }

    /// Publish to a GossipSub topic
    pub async fn publish(&self, topic: String, data: Vec<u8>) -> Result<(), mpsc::error::SendError<NodeCommand>> {
        self.command_tx.send(NodeCommand::Publish { topic, data }).await
    }

    /// Subscribe to a GossipSub topic
    pub async fn subscribe(&self, topic: String) -> Result<(), mpsc::error::SendError<NodeCommand>> {
        self.command_tx.send(NodeCommand::Subscribe(topic)).await
    }

    /// Get current listening addresses
    pub async fn listening_addrs(&self) -> Result<Vec<Multiaddr>, Box<dyn std::error::Error>> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.command_tx.send(NodeCommand::GetListeningAddrs(tx)).await?;
        Ok(rx.await?)
    }

    /// Shut down the node
    pub async fn shutdown(self) -> Result<(), mpsc::error::SendError<NodeCommand>> {
        self.command_tx.send(NodeCommand::Shutdown).await
    }
}

/// Process a swarm event and emit NodeEvents
async fn handle_swarm_event(
    _swarm: &mut Swarm<NexusBehaviour>,
    event: SwarmEvent<NexusBehaviourEvent>,
    event_tx: &mpsc::Sender<NodeEvent>,
) {
    match event {
        SwarmEvent::NewListenAddr { address, .. } => {
            let _ = event_tx.send(NodeEvent::Listening(address)).await;
        }
        SwarmEvent::Behaviour(NexusBehaviourEvent::Mdns(mdns::Event::Discovered(peers))) => {
            for (peer_id, _addr) in peers {
                let _ = event_tx.send(NodeEvent::PeerDiscovered(peer_id)).await;
            }
        }
        SwarmEvent::Behaviour(NexusBehaviourEvent::RequestResponse(
            request_response::Event::Message { peer, message, .. },
        )) => {
            match message {
                request_response::Message::Request { request, channel, .. } => {
                    match request {
                        NexusRequest::GetShard { cid } => {
                            let _ = event_tx.send(NodeEvent::ShardRequested {
                                peer,
                                cid,
                                channel,
                            }).await;
                        }
                        NexusRequest::DeliverKfrags { manifest_id, kfrags, verifying_key, sender_pre_pk } => {
                            let _ = event_tx.send(NodeEvent::KfragsReceived {
                                peer,
                                manifest_id,
                                kfrags,
                                verifying_key,
                                sender_pre_pk,
                            }).await;
                        }
                        NexusRequest::Ping => {
                            // Auto-respond to pings
                            let _ = _swarm.behaviour_mut().request_response
                                .send_response(channel, NexusResponse::Pong);
                        }
                    }
                }
                request_response::Message::Response { response, .. } => {
                    let _ = event_tx.send(NodeEvent::ShardReceived {
                        peer,
                        response,
                    }).await;
                }
            }
        }
        SwarmEvent::Behaviour(NexusBehaviourEvent::Gossipsub(
            gossipsub::Event::Message { message, .. },
        )) => {
            let _ = event_tx.send(NodeEvent::GossipMessage {
                topic: message.topic.to_string(),
                data: message.data,
                source: message.source,
            }).await;
        }
        SwarmEvent::ConnectionClosed { peer_id, .. } => {
            let _ = event_tx.send(NodeEvent::PeerDisconnected(peer_id)).await;
        }
        _ => {}
    }
}
