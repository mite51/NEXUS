//! NEXUS Node — the main networking entry point
//!
//! Manages a libp2p swarm with all NEXUS protocols.
//! Can be started as a background task and communicated with via channels.

use libp2p::{
    identity::Keypair, Multiaddr, PeerId, Swarm, SwarmBuilder,
    noise, tcp, yamux,
    swarm::SwarmEvent,
    gossipsub, autonat, dcutr,
    request_response,
    mdns,
};
use futures::StreamExt;
use std::time::Duration;
use tokio::sync::mpsc;

use super::behaviour::{NexusBehaviour, NexusBehaviourEvent};
use super::protocol::{NexusRequest, NexusResponse};
use super::telemetry::{TelemetryCollector, ConnectivityEvent, NatStatus};

/// Configuration for starting a NEXUS node
#[derive(Debug, Clone)]
pub struct NodeConfig {
    /// Listen addresses (e.g., "/ip4/0.0.0.0/udp/0/quic-v1")
    pub listen_addrs: Vec<String>,
    /// Bootstrap peers (for joining the network)
    pub bootstrap_peers: Vec<(PeerId, Multiaddr)>,
    /// Whether to enable mDNS for local discovery
    pub mdns_enabled: bool,
    /// Relay servers to attempt reservation with
    pub relay_servers: Vec<String>,
    /// Whether to enable telemetry collection
    pub telemetry_enabled: bool,
    /// Directory for telemetry log files
    pub telemetry_dir: Option<String>,
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
            relay_servers: vec![],
            telemetry_enabled: true,
            telemetry_dir: None,
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
    /// A peer pushed a shard to us
    ShardPushed {
        peer: PeerId,
        cid: String,
        data: Vec<u8>,
        channel: request_response::ResponseChannel<NexusResponse>,
    },
    /// A peer pushed a manifest (+ optional share grant) to us
    ManifestPushed {
        peer: PeerId,
        manifest_json: String,
        share_grant_json: Option<String>,
        channel: request_response::ResponseChannel<NexusResponse>,
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
    /// NAT status changed
    NatStatusChanged {
        status: NatStatus,
    },
    /// Relay reservation established
    RelayReserved {
        relay_peer: PeerId,
        relay_addr: Multiaddr,
    },
    /// Hole punch completed (success or failure)
    HolePunchResult {
        remote_peer: PeerId,
        success: bool,
    },
}

/// Commands that can be sent to the node
#[derive(Debug)]
pub enum NodeCommand {
    /// Dial a peer at an address
    Dial(Multiaddr),
    /// Connect to a relay and listen through it
    ListenOnRelay(Multiaddr),
    /// Request a shard from a peer
    RequestShard { peer: PeerId, cid: String },
    /// Push a shard to a peer
    PushShard { peer: PeerId, cid: String, data: Vec<u8> },
    /// Push a manifest to a peer
    PushManifest { peer: PeerId, manifest_json: String, share_grant_json: Option<String> },
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
    /// Get connected peers
    GetConnectedPeers(tokio::sync::oneshot::Sender<Vec<PeerId>>),
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

        // Set up telemetry
        let telemetry_dir = config.telemetry_dir.unwrap_or_else(|| ".nexus-telemetry".to_string());
        let telemetry = TelemetryCollector::new(
            &telemetry_dir,
            local_peer_id.to_string(),
            config.telemetry_enabled,
        );

        // Collect relay servers to dial after startup
        let relay_servers = config.relay_servers.clone();

        // Spawn the event loop
        tokio::spawn(async move {
            // Dial relay servers for reservation
            for relay_addr_str in &relay_servers {
                if let Ok(addr) = relay_addr_str.parse::<Multiaddr>() {
                    // Listen through the relay (creates a reservation)
                    let relay_listen = addr.clone()
                        .with(libp2p::multiaddr::Protocol::P2pCircuit);
                    match swarm.listen_on(relay_listen.clone()) {
                        Ok(_) => {
                            telemetry.record(ConnectivityEvent::RelayReservation {
                                relay_peer: "unknown".to_string(),
                                relay_addr: relay_addr_str.clone(),
                                success: true,
                                error: None,
                                duration_ms: 0,
                            });
                        }
                        Err(e) => {
                            telemetry.record(ConnectivityEvent::RelayReservation {
                                relay_peer: "unknown".to_string(),
                                relay_addr: relay_addr_str.clone(),
                                success: false,
                                error: Some(e.to_string()),
                                duration_ms: 0,
                            });
                        }
                    }
                }
            }

            loop {
                tokio::select! {
                    // Handle commands from the application
                    Some(cmd) = command_rx.recv() => {
                        match cmd {
                            NodeCommand::Dial(addr) => {
                                let _ = swarm.dial(addr);
                            }
                            NodeCommand::ListenOnRelay(addr) => {
                                let relay_listen = addr.with(libp2p::multiaddr::Protocol::P2pCircuit);
                                let _ = swarm.listen_on(relay_listen);
                            }
                            NodeCommand::RequestShard { peer, cid } => {
                                swarm.behaviour_mut().request_response.send_request(
                                    &peer,
                                    NexusRequest::GetShard { cid },
                                );
                            }
                            NodeCommand::PushShard { peer, cid, data } => {
                                swarm.behaviour_mut().request_response.send_request(
                                    &peer,
                                    NexusRequest::PushShard { cid, data },
                                );
                            }
                            NodeCommand::PushManifest { peer, manifest_json, share_grant_json } => {
                                swarm.behaviour_mut().request_response.send_request(
                                    &peer,
                                    NexusRequest::PushManifest { manifest_json, share_grant_json },
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
                            NodeCommand::GetConnectedPeers(tx) => {
                                let peers: Vec<PeerId> = swarm.connected_peers().cloned().collect();
                                let _ = tx.send(peers);
                            }
                            NodeCommand::Shutdown => break,
                        }
                    }
                    // Handle swarm events
                    event = swarm.select_next_some() => {
                        handle_swarm_event(&mut swarm, event, &event_tx, &telemetry).await;
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

    /// Push a shard to a peer
    pub async fn push_shard(&self, peer: PeerId, cid: String, data: Vec<u8>) -> Result<(), mpsc::error::SendError<NodeCommand>> {
        self.command_tx.send(NodeCommand::PushShard { peer, cid, data }).await
    }

    /// Push a manifest (+ optional share grant) to a peer
    pub async fn push_manifest(&self, peer: PeerId, manifest_json: String, share_grant_json: Option<String>) -> Result<(), mpsc::error::SendError<NodeCommand>> {
        self.command_tx.send(NodeCommand::PushManifest { peer, manifest_json, share_grant_json }).await
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

    /// Get currently connected peers
    pub async fn connected_peers(&self) -> Result<Vec<PeerId>, Box<dyn std::error::Error>> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.command_tx.send(NodeCommand::GetConnectedPeers(tx)).await?;
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
    telemetry: &TelemetryCollector,
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
                        NexusRequest::PushShard { cid, data } => {
                            let _ = event_tx.send(NodeEvent::ShardPushed {
                                peer,
                                cid,
                                data,
                                channel,
                            }).await;
                        }
                        NexusRequest::PushManifest { manifest_json, share_grant_json } => {
                            let _ = event_tx.send(NodeEvent::ManifestPushed {
                                peer,
                                manifest_json,
                                share_grant_json,
                                channel,
                            }).await;
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
        // AutoNAT status changes
        SwarmEvent::Behaviour(NexusBehaviourEvent::Autonat(autonat::Event::StatusChanged { old: _, new })) => {
            let status = match new {
                autonat::NatStatus::Public(_) => NatStatus::Public,
                autonat::NatStatus::Private => NatStatus::Private,
                autonat::NatStatus::Unknown => NatStatus::Unknown,
            };
            telemetry.record(ConnectivityEvent::NatStatusChanged {
                status: status.clone(),
                confidence: 0,
            });
            let _ = event_tx.send(NodeEvent::NatStatusChanged { status }).await;
        }
        // DCUtR hole-punch events
        SwarmEvent::Behaviour(NexusBehaviourEvent::Dcutr(dcutr::Event { remote_peer_id, result })) => {
            let success = result.is_ok();
            let error = result.err().map(|e| format!("{:?}", e));
            telemetry.record(ConnectivityEvent::HolePunch {
                remote_peer: remote_peer_id.to_string(),
                success,
                direct_addr: None,
                error,
                duration_ms: 0,
            });
            let _ = event_tx.send(NodeEvent::HolePunchResult {
                remote_peer: remote_peer_id,
                success,
            }).await;
        }
        // Connection established (track relay vs direct)
        SwarmEvent::ConnectionEstablished { peer_id, endpoint, num_established, .. } => {
            let addr = endpoint.get_remote_address().to_string();
            let is_relayed = addr.contains("/p2p-circuit/");
            telemetry.record(ConnectivityEvent::ConnectionEstablished {
                remote_peer: peer_id.to_string(),
                addr,
                is_relayed,
                num_established: num_established.get(),
            });
            if num_established.get() == 1 {
                let _ = event_tx.send(NodeEvent::PeerDiscovered(peer_id)).await;
            }
        }
        // Connection closed
        SwarmEvent::ConnectionClosed { peer_id, num_established, .. } => {
            telemetry.record(ConnectivityEvent::ConnectionClosed {
                remote_peer: peer_id.to_string(),
                duration_secs: 0,
                was_relayed: false,
                cause: None,
            });
            if num_established == 0 {
                let _ = event_tx.send(NodeEvent::PeerDisconnected(peer_id)).await;
            }
        }
        // Outgoing connection error
        SwarmEvent::OutgoingConnectionError { peer_id, error, .. } => {
            let err_str = format!("{}", error);
            telemetry.record(ConnectivityEvent::DialFailure {
                remote_peer: peer_id.map(|p| p.to_string()),
                addr: "unknown".to_string(),
                error: err_str,
                is_relay: false,
            });
        }
        _ => {}
    }
}
