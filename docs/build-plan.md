# NEXUS Build Plan

**Version:** 1.0
**Date:** May 2026
**Status:** Approved

---

## Architectural Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| **Core language** | Pure Rust with `rust-umbral` | Single binary, no runtime deps, audited PRE, natural Tauri fit |
| **Storage** | Custom CAS with IPFS-compatible CIDs | Privacy by default, no metadata leakage to public DHTs, IPFS export optional |
| **Frontend** | Svelte + TypeScript | Simple mental model, tiny bundles, great DX |
| **License** | GPL-3.0 | Aligned with anti-silo philosophy, compatible with rust-umbral |
| **Platforms** | Windows, Linux, macOS, Android, iOS, Web | Tauri 2.0 (desktop+mobile) + WASM (web) |

---

## Project Structure

```
NEXUS/
├── nexus-core/          # Pure Rust crate — crypto, storage, networking
│   ├── src/
│   │   ├── identity/    # DID, keypairs, vault, social recovery
│   │   ├── crypto/      # PRE (rust-umbral), AES-256-GCM, X3DH, Double Ratchet
│   │   ├── storage/     # Content-addressed sharding, manifests, mirror protocol
│   │   ├── network/     # libp2p, Kademlia DHT, GossipSub, transport
│   │   └── ledger/      # Nexus Credits, transaction history, atomic swaps
│   └── Cargo.toml
├── nexus-tauri/         # Tauri 2.0 shell (desktop + mobile)
│   ├── src-tauri/       # Rust bridge (wraps nexus-core)
│   └── tauri.conf.json
├── nexus-ui/            # Svelte + TypeScript (shared across all platforms)
│   ├── src/
│   │   ├── components/
│   │   ├── routes/
│   │   └── stores/
│   └── package.json
├── nexus-web/           # WASM bridge for browser clients (Phase 6+)
├── docs/
│   ├── Project_Nexus_PRD_v1.0.pdf
│   ├── Project_Nexus_SRD_v1.0.pdf
│   └── build-plan.md    # This file
├── RULES.md
├── CHANGELOG.md
└── README.md
```

### Compilation Targets

```
nexus-core (Rust crate)
├── → Native binary (linked into nexus-tauri for desktop/mobile)
└── → WASM (compiled for nexus-web browser client)

nexus-ui (Svelte)
├── → Bundled into Tauri app (desktop/mobile)
└── → Served as static SPA (web, connects via WASM or WebSocket to home server)
```

---

## Cross-Platform Strategy

| Platform | Runtime | Networking | Notes |
|----------|---------|------------|-------|
| Windows / Linux / macOS | Tauri 2.0 (native) | Full libp2p (QUIC + TCP) | Primary development target |
| Android / iOS | Tauri 2.0 (mobile) | libp2p (QUIC preferred) | Tauri 2.0 mobile support |
| Web browser | WASM + Svelte SPA | WebSocket/WebRTC to relay | Requires home server or public relay for networking |

**Design principle:** `nexus-core` has zero Tauri dependencies. It compiles standalone. Platform-specific code lives only in the shell layers (nexus-tauri, nexus-web).

---

## Identity & Key Rotation

PRE enables identity migration without re-encrypting data:

1. All data shards are content-addressed (identity-agnostic)
2. Access is controlled by kfrags (identity-specific)
3. To rotate keys or rebuild identity:
   - Generate new DID (new keypair)
   - Use old private key to generate kfrags: old_key → new_key
   - All existing encrypted data becomes accessible under new identity
   - No shards are re-encrypted or moved
4. Safety net: Shamir's Secret Sharing (Social Recovery) for lost keys

---

## Build Phases

### Phase 0: Foundation
1. **Project scaffolding** — Cargo workspace, Tauri 2.0, Svelte, CI/CD
2. **DID Identity** — Ed25519 keypair generation, Argon2id-encrypted vault, import/export
3. **Core crypto** — rust-umbral PRE, AES-256-GCM symmetric, key serialization

**Exit criteria:** Can generate identity, encrypt/decrypt a file, generate kfrags, re-encrypt for another identity. All tested.

### Phase 1: Networking
4. **libp2p integration** — Kademlia DHT, bootstrap nodes, peer connection
5. **Transport** — QUIC primary, TCP/TLS fallback, NAT traversal
6. **Encrypted messaging** — X3DH handshake + Double Ratchet for 1:1 channels

**Exit criteria:** Two nodes can discover each other, establish encrypted channel, exchange messages. All tested.

### Phase 2: Storage
7. **Content-addressed sharding** — File splitting, CID generation, local shard store
8. **PRE file sharing** — Encrypt shards, generate kfrags on approval, decrypt flow
9. **Mirror server protocol** — Proof-of-availability challenges, shard replication

**Exit criteria:** Can shard a file, store locally, replicate to a mirror, share with another identity via PRE, verify mirror availability. All tested.

### Phase 3: Nexus Drive (First App)
10. **File sync** — Watch directories, detect changes, shard+encrypt+distribute
11. **Folder sharing** — PRE approval flow, kfrag generation on grant, revocation
12. **Drive UI** — File browser, sharing dialogs, sync status

**Exit criteria:** Working encrypted Dropbox-like app for desktop. Can share folders with other users.

### Phase 4: Nexus Feed (Second App)
13. **GossipSub** — Topic-based pub/sub for feed updates
14. **Feed curation** — Plugin interface for filters (chronological, media-only, vouched-only)
15. **Attention Gate** — Paid inbox, fee-based unsolicited delivery

**Exit criteria:** Working encrypted social feed. Posts visible only to approved graph.

### Phase 5: Economics
16. **Nexus Credits ledger** — Local signed transaction history, peer settlement
17. **One-way bridge** — Crypto→Credits conversion (BTC/USDC ingest)
18. **Mirror incentives** — Automated payouts for proof-of-availability

**Exit criteria:** Credits can be earned (mirror hosting), spent (attention fees), and topped up (crypto bridge).

### Phase 6: Polish & Expansion
19. **Social Recovery** — Shamir's SSS, guardian selection, recovery flow
20. **Plugin marketplace** — Package format, sandboxed WASM runtime
21. **Web client** — nexus-web WASM bridge, WebSocket relay
22. **Mobile** — Tauri 2.0 mobile builds (Android/iOS)
23. **Onboarding UX** — QR codes, magic links, first-run experience

---

## Dependencies (Minimal Set)

### Rust (nexus-core)
| Crate | Purpose | License |
|-------|---------|---------|
| `rust-umbral` | Proxy Re-Encryption (Umbral scheme) | GPL-3.0 |
| `libp2p` | P2P networking, Kademlia, GossipSub | MIT |
| `aes-gcm` | Symmetric encryption (AES-256-GCM) | MIT/Apache-2.0 |
| `ed25519-dalek` | DID keypair generation/signing | BSD-3 |
| `argon2` | Vault passphrase KDF | MIT/Apache-2.0 |
| `sled` or `sqlite` | Local shard/manifest store | MIT/Apache-2.0 |
| `multihash` | IPFS-compatible content addressing | MIT/Apache-2.0 |
| `serde` | Serialization | MIT/Apache-2.0 |

### Frontend (nexus-ui)
| Package | Purpose |
|---------|---------|
| `svelte` | UI framework |
| `typescript` | Type safety |
| `@tauri-apps/api` | Tauri IPC bridge |

**Principle:** Every dependency must justify its existence. Prefer `no_std`-compatible crates where possible for future WASM compilation.
