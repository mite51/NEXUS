# Changelog

All notable changes to NEXUS will be documented in this file.

Format follows [Keep a Changelog](https://keepachangelog.com/).

## [Unreleased]

### Added
- Initial project setup
- PRD v1.0 and SRD v1.0 planning documents
- Project rules and development disciplines (RULES.md)
- Core identity module: Ed25519 keypairs, DID generation (did:nexus:*), Argon2id encrypted vault
- Symmetric encryption: AES-256-GCM with random DEK generation
- Content-addressed sharding with IPFS-compatible CIDs (SHA2-256 multihash)
- **Proxy Re-Encryption (PRE)**: Full Umbral scheme via rust-umbral
  - Owner encryption/decryption of DEKs
  - kfrag generation for access delegation (threshold scheme)
  - Capsule re-encryption by proxies
  - Delegated decryption by recipients
  - Serialization of all PRE types (capsules, kfrags, cfrags)
- CLI client (`nexus` binary): init, identity, encrypt, decrypt, share, decrypt-shared, export-key, node, ping
- Research docs: rust-umbral, libp2p, integration options
- **libp2p Networking (Phase 1)**:
  - NexusBehaviour: Kademlia DHT, GossipSub, Request-Response, mDNS, Relay
  - NexusNode: async swarm with command/event channel architecture
  - Custom /nexus/1.0.0 wire protocol (shard requests, kfrag delivery, ping)
  - Length-prefixed JSON codec (16MB max message)
  - `nexus node` CLI command: starts a persistent P2P daemon
  - `nexus ping` CLI command: connectivity check
  - Ed25519 identity → libp2p PeerId bridge (DID = PeerId, single key)
  - QUIC + TCP+Noise transports, mDNS local discovery
  - Integration tests: discovery, shard request/response, identity consistency
