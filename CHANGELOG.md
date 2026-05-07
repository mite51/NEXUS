# Changelog

All notable changes to NEXUS will be documented in this file.

Format follows [Keep a Changelog](https://keepachangelog.com/).

## [Unreleased]

### Added — Networking & Infrastructure
- **Relay server** (`nexus relay` CLI + Tauri toggle): configurable relay for NAT traversal
- **NAT traversal pipeline**: AutoNAT → relay reservation → DCUtR hole-punching → relay fallback
- **Relay public IP detection**: probes ifconfig.me/api.ipify.org for external address
- **Auto-start settings**: `auto_start_node` and `auto_start_relay` config booleans
- **Logs panel**: real-time event feed from node backend (last 500 entries)
- **`nexus get-shard` CLI**: single-shard fetch for debugging

### Added — Contact & Identity Management
- **Contact editing**: inline edit mode with save/cancel
- **Invite-based contacts**: auto-generated PRE keypair per invite
- **Join handshake**: `create-join-request` / `accept-join-request` / `apply-join-response` CLI commands for offline contact exchange
- **Join flow UI**: tabbed panel in Tauri for creating and accepting join requests
- **Contact struct extended**: `peer_id`, `relay_addrs`, `pre_seed_encrypted`, `invite_pending`, `notes`
- **HKDF-based PRE key derivation** (`032276c`): `PreKeypair::derive_for_peer(vault_seed, peer_id)` uses HKDF-SHA256 (salt: `nexus-pre-peer-v1`, info: peer_id). Deterministic — delete + re-add same peer → same key. No separate seed storage needed.

### Added — Pull-Only Sharing (replaces push model)
- **AssetStore module**: `SHA-256(manifest_bytes)` as asset_id, stores manifests + rfrags alongside shards
- **`PullAssetRequested` protocol**: peer sends signed request → node checks rfrag → streams asset back
- **SharePanel component**: overlay showing share link + authorized users + add/remove
- **Share links**: `nexus://<peer-id>/asset/<asset-id>` — dumb pointer, node does auth
- **`get_share_info` command**: returns asset_id, link, list of authorized DIDs
- **`revoke_share` command**: deletes rfrag to instantly revoke access
- **Asset serving in Tauri node**: on pull request, validates DID → serves rfrag + manifest + shards
- **`share_file` rewrite**: now writes rfrag to `.nexus-store/rfrags/<asset-id>/<did>.rfrag` + caches manifest

### Changed — Architecture
- **Sharing model: push → pull** (`e3565c7`): The push model required both parties online simultaneously, put upload burden on sender, offered no revocation, and couldn't support "share a link." Pull-only inverts this: recipients fetch when *they're* ready, from the owner's node. Simpler, more secure, supports revocation and offline link sharing.
- **HKDF for PRE keys** (`032276c`): Previously PRE keypairs were random per-contact, requiring separate storage and making key recovery impossible. HKDF derivation from vault seed means the vault *is* the backup — deterministic re-derivation from `(vault_seed, peer_id)`.
- **Node/Relay controls moved to Settings**: was a separate sidebar view, now unified under Settings → Network section
- **PeersView merged into SettingsView**: removed from sidebar

### Removed
- **Push delivery infrastructure** (`e3565c7`):
  - "Send to Peer" button, SendQueueView (outbox), delivery worker
  - `queue_send`, `list_send_queue`, `cancel_send`, `retry_send` commands
  - `send_queue.rs`, `delivery.rs` modules (dead code, pending deletion)
  - CLI `send` subcommand (~145 lines of push logic)
  - Rationale: push required both peers online, no revocation, no link-sharing. Pull-only is strictly better for our threat model.

### Fixed
- CI pipeline: scoped to `nexus-core` + `nexus-cli` only (Tauri needs webkit/GTK headers)
- Bootstrap peer dialing (wasn't actually connecting on startup)
- Relay reservation timing (was calling listen_on before relay connection established)
- Node start error reporting (better error messages surfaced to UI)
- Icon button opacity bumped 0.4→0.6 for visibility

### UI/UX
- Compact single-line contact rows with avatar + badges + inline actions
- Consistent Node/Relay settings layout
- Sidebar simplified: removed Outbox, removed Peers (merged into Settings)
- Dark theme refinements

---

## [0.1.0] — Initial Release (Phase 1–3 Foundation)

### Added
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
- **libp2p Networking**:
  - NexusBehaviour: Kademlia DHT, GossipSub, Request-Response, mDNS, Relay
  - NexusNode: async swarm with command/event channel architecture
  - Custom /nexus/1.0.0 wire protocol (shard requests, kfrag delivery, ping)
  - Length-prefixed JSON codec (16MB max message)
  - Ed25519 identity → libp2p PeerId bridge (DID = PeerId, single key)
  - QUIC + TCP+Noise transports, mDNS local discovery
- **Storage Layer**:
  - ShardStore: disk-backed content-addressed storage with CID integrity verification
  - NetworkStore: bridges local store with P2P node (local-first, network fallback)
  - `nexus store stats/list/import/verify` CLI commands
  - `nexus encrypt` auto-stores shards in `.nexus-store` for P2P serving
- **`nexus fetch` command**: full receive flow over P2P
  - Connect to peer, request all shards, verify CIDs, reassemble, decrypt
  - Supports both owner decryption and shared access (via `--share` grant)
- **PRE Shared Access E2E Proven**:
  - Alice encrypts → generates PRE share grant → Bob fetches over P2P → decrypts with cfrags
  - Bob never sees Alice's private key — proves the NEXUS thesis
- **NexusManifest extracted to nexus-core**: shared types for CLI and Tauri
- **Tauri 2.0 GUI**:
  - Svelte 5 + TypeScript + Vite 8 frontend
  - Dark theme with responsive layout
  - Setup, unlock, main file grid, store view, detail panel
  - File dialog integration, decrypt flow, IPC layer
