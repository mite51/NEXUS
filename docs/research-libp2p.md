# Research: rust-libp2p (Networking Integration)

**Date:** 2026-05-04
**Status:** Ready for integration (Phase 1)

## Crate Info

- **Crate name:** `libp2p` (umbrella) with individual sub-crates
- **Version:** `0.57.0` (latest)
- **MSRV:** Rust 1.88.0, **Edition 2024** — we have Rust 1.95 ✅
- **License:** MIT ✅
- **Repo:** https://github.com/libp2p/rust-libp2p

## ⚠️ Edition 2024 Requirement

libp2p 0.57 requires Rust edition 2024. Our workspace is currently `edition = "2021"`.

**Options:**
- **A) Bump workspace to edition 2024**: Simple, Rust 1.95 supports it
- **B) Keep 2021 for nexus-core, use 2024 only for nexus-net**: Crate-level edition override
- **C) Use older libp2p**: 0.54.x was last edition 2021 compatible, but misses newer features

**Recommendation: Option A** — just bump to 2024. No breaking changes for our existing code.

## Minimal Feature Set for NEXUS

| NEXUS Need | libp2p Module | Crate |
|-----------|---------------|-------|
| Peer discovery | Kademlia DHT | `libp2p-kad` |
| Direct messaging | Request-Response | `libp2p-request-response` |
| Feed pub/sub | GossipSub | `libp2p-gossipsub` |
| NAT traversal | DCUtR (hole punching) | `libp2p-dcutr` |
| Relay (fallback) | Circuit Relay v2 | `libp2p-relay` |
| Transport (primary) | QUIC | `libp2p-quic` |
| Transport (fallback) | TCP + Noise | `libp2p-tcp` + `libp2p-noise` |
| Peer identity | Ed25519 identity | `libp2p-identity` |
| Muxing | Yamux | `libp2p-yamux` |
| Local discovery | mDNS | `libp2p-mdns` |

## Cargo.toml Dependencies

```toml
[dependencies]
libp2p = { version = "0.57", features = [
    "tokio",
    "quic",
    "tcp",
    "noise",
    "yamux",
    "kad",
    "gossipsub",
    "request-response",
    "dcutr",
    "relay",
    "mdns",
    "identify",
    "ed25519",
    "macros",
] }
tokio = { version = "1", features = ["full"] }
```

## Minimal Swarm Example

```rust
use libp2p::{
    gossipsub, kad, noise, quic, tcp, yamux,
    identity, Multiaddr, SwarmBuilder,
};
use std::time::Duration;

#[tokio::main]
async fn main() {
    // Generate or load identity keypair
    let local_key = identity::Keypair::generate_ed25519();
    let local_peer_id = local_key.public().to_peer_id();
    println!("Local peer ID: {local_peer_id}");

    // Build swarm with Kademlia + GossipSub
    let mut swarm = SwarmBuilder::with_existing_identity(local_key)
        .with_tokio()
        .with_quic()
        .with_tcp(tcp::Config::default(), noise::Config::new, yamux::Config::default)
        .with_behaviour(|key| {
            let gossipsub_config = gossipsub::ConfigBuilder::default()
                .build()
                .unwrap();
            let gossipsub = gossipsub::Behaviour::new(
                gossipsub::MessageAuthenticity::Signed(key.clone()),
                gossipsub_config,
            ).unwrap();

            let kad = kad::Behaviour::new(
                local_peer_id,
                kad::store::MemoryStore::new(local_peer_id),
            );

            MyBehaviour { gossipsub, kad }
        })
        .unwrap()
        .with_swarm_config(|cfg| cfg.with_idle_connection_timeout(Duration::from_secs(60)))
        .build();

    // Listen on QUIC
    swarm.listen_on("/ip4/0.0.0.0/udp/0/quic-v1".parse().unwrap()).unwrap();

    // Event loop
    loop {
        match swarm.select_next_some().await {
            // Handle events...
            _ => {}
        }
    }
}

#[derive(libp2p::swarm::NetworkBehaviour)]
struct MyBehaviour {
    gossipsub: gossipsub::Behaviour,
    kad: kad::Behaviour<kad::store::MemoryStore>,
}
```

## Platform Compatibility

### Desktop (Tauri 2.0) ✅
- Linux/macOS/Windows all fully supported
- QUIC via Quinn (native UDP sockets)
- TCP + Noise as fallback
- mDNS for LAN discovery

### Mobile (Tauri 2.0 Mobile) ⚠️
| Feature | Android | iOS |
|---------|---------|-----|
| QUIC | ✅ Works (UDP sockets available) | ✅ Works |
| TCP | ✅ | ✅ |
| mDNS | ⚠️ May need network permissions | ⚠️ Limited in background |
| Background networking | ⚠️ Restricted by OS power management | ⚠️ Very restricted |
| Hole punching (DCUtR) | ✅ | ✅ |

**Key mobile issues:**
- iOS aggressively kills background network connections
- Both platforms require careful handling of app lifecycle (pause/resume)
- Relay servers become more important for mobile (devices go offline frequently)

**Mitigation:** For mobile, treat home server as primary relay. Don't rely on direct P2P staying alive.

### Compile Times & Binary Size
- Full libp2p with all features: **~3-5 minutes** initial compile
- Binary size contribution: **~5-10 MB** (release, stripped)
- Feature-gating is effective — only pull what you use

## Key Architectural Decisions

### 1. Identity Mapping
libp2p uses Ed25519 `PeerId` by default — aligns with our DID system.

**Integration path:** Derive libp2p identity from the same Ed25519 key used for `did:nexus:*`. This means PeerId = DID = single identity.

### 2. Kademlia for DID Resolution
Store DID → multiaddr mapping in the DHT. When Bob wants to find Alice:
1. Hash Alice's DID to a Kademlia key
2. Look up in DHT → get her current network addresses
3. Dial directly (or via relay if behind NAT)

### 3. Request-Response for Direct Messaging
Use libp2p's `request-response` protocol for:
- Shard requests (Bob asks Alice's mirror for a shard)
- kfrag delivery (Alice sends kfrag to Bob)
- Direct encrypted messages

### 4. GossipSub for Feeds
Each user's feed is a GossipSub topic (topic = DID hash). Subscribers get new posts via pub/sub.

## Gotchas

1. **Ring vs AWS-LC**: libp2p's QUIC transport uses `quinn` which depends on `ring` or `aws-lc-rs` for TLS. On some platforms, `ring` has build issues. May need `aws-lc-rs` feature flag.
2. **Yamux vs Mplex**: Mplex is deprecated. Use Yamux only.
3. **DNS resolution**: If using QUIC, DNS resolution must happen before dialing. Use `libp2p-dns` wrapper.
4. **Bootstrap nodes**: Kademlia needs at least one bootstrap node to join the network. We'll need to run initial bootstrap infrastructure.
