# NEXUS — System Design

## Overview

NEXUS is a decentralized, peer-to-peer encrypted file ownership platform. There are no central servers that hold your data. Files are encrypted locally, split into content-addressed shards, and transferred directly between peers using libp2p networking.

```mermaid
graph TB
    subgraph "Your Machine"
        UI[Desktop App / CLI]
        Core[nexus-core]
        Vault[Identity Vault]
        Store[Shard Store]
    end

    subgraph "Network"
        Relay[Relay Server]
        PeerB[Another Peer]
    end

    UI --> Core
    Core --> Vault
    Core --> Store
    Core <-->|libp2p| Relay
    Core <-->|libp2p| PeerB
    Relay -.->|circuit| PeerB
```

---

## Core Components

### 1. Identity (`nexus-core/src/identity/`)

Every user has a cryptographic identity — no usernames, no passwords, no accounts on someone else's server.

| Component | Purpose |
|-----------|---------|
| **IdentityKeypair** | Ed25519 signing keypair — your "ID card" |
| **DID** | Decentralized Identifier: `did:key:z6Mk...` derived from your public key |
| **PeerId** | libp2p network identifier (also derived from your keypair) |
| **IdentityVault** | Encrypted on-disk storage of your keypair (passphrase-protected via Argon2) |

```mermaid
graph LR
    Passphrase -->|Argon2 KDF| VaultKey
    VaultKey -->|AES-256-GCM| EncryptedKeypair
    EncryptedKeypair -->|stored as| vault.json
    Keypair -->|derive| DID
    Keypair -->|derive| PeerId
    Keypair -->|derive| PreKeypair
```

**Your DID is your identity.** Anyone who knows your DID can encrypt files for you, verify your signatures, or find you on the network. You never hand over credentials to anyone.

---

### 2. Encryption (`nexus-core/src/crypto/`)

NEXUS uses a two-layer encryption scheme:

#### Layer 1: Symmetric (AES-256-GCM)
- A random **Data Encryption Key (DEK)** is generated per file
- The file body is encrypted with this DEK
- Fast, handles any file size

#### Layer 2: Proxy Re-Encryption (Umbral PRE)
- The DEK itself is encrypted under your PRE public key (key encapsulation)
- Only you can decrypt it... unless you **delegate access**

```mermaid
sequenceDiagram
    participant User as You
    participant File as File Data
    participant DEK as Random DEK
    participant PRE as PRE Layer

    User->>DEK: Generate random 256-bit key
    DEK->>File: AES-256-GCM encrypt
    File-->>User: Ciphertext
    User->>PRE: Encrypt DEK under your PRE public key
    PRE-->>User: EncryptedDEK (Umbral capsule)
```

**Why two layers?** So you can share access to a file without decrypting it or re-encrypting it. You generate **kfrags** (key fragments) that let the recipient reconstruct the DEK. The actual file data never moves through any proxy.

---

### 3. Storage (`nexus-core/src/storage/`)

Files aren't stored as blobs — they're **sharded** into content-addressed chunks.

#### Sharding Process

```mermaid
graph LR
    Encrypted[Encrypted File] -->|Split 256KB chunks| S1[Shard 1]
    Encrypted --> S2[Shard 2]
    Encrypted --> S3[Shard 3]
    S1 -->|SHA-256 hash| CID1[CID: 1220ab...]
    S2 -->|SHA-256 hash| CID2[CID: 1220cd...]
    S3 -->|SHA-256 hash| CID3[CID: 1220ef...]
    CID1 --> Store[".nexus-store/"]
    CID2 --> Store
    CID3 --> Store
```

| Component | Purpose |
|-----------|---------|
| **Shard** | 256KB chunk of encrypted data, identified by its CID (content hash) |
| **CID** | SHA2-256 multihash of the shard bytes — guarantees integrity |
| **ShardStore** | On-disk directory (`.nexus-store/`) that maps CID → bytes |
| **ShardManifest** | List of CIDs in order — the "recipe" to reassemble the file |

**Content-addressing means:**
- Identical data produces identical CIDs (deduplication)
- Any corruption is instantly detectable (hash doesn't match)
- Shards are self-verifying — no trust required between peers

---

### 4. Manifests (`nexus-core/src/manifest.rs`)

A **manifest** (`.nexus` file) is the metadata that ties everything together:

```json
{
  "owner": "did:key:z6Mk...",
  "owner_pre_pk": { "bytes": "..." },
  "shards": {
    "filename": "report.pdf",
    "total_size": 1048576,
    "shard_size": 262144,
    "shards": ["1220ab...", "1220cd...", "1220ef...", "1220gh..."]
  },
  "encrypted_dek": {
    "capsule": "...",
    "ciphertext": "..."
  }
}
```

The manifest is small (< 1 KB typically). It's what you send to someone to give them *access* — without sending the actual file data.

---

### 5. Sharing via Proxy Re-Encryption

This is the magic of NEXUS. You can share a file with someone **without decrypting it** and **without trusting any server**.

```mermaid
sequenceDiagram
    participant Alice as Alice (Owner)
    participant Bob as Bob (Recipient)
    participant Net as Network

    Note over Alice: File already encrypted with her DEK
    Alice->>Alice: Generate kfrags (key fragments) for Bob
    Alice->>Net: Send kfrags + manifest to Bob's peer
    Net->>Bob: Deliver kfrags + manifest
    Bob->>Bob: Use kfrags to re-derive DEK
    Bob->>Net: Request shards (by CID)
    Net->>Bob: Deliver shard data
    Bob->>Bob: Reassemble + decrypt file
```

#### The PRE Flow in Detail

1. **Alice** has an encrypted file (DEK wrapped in her Umbral capsule)
2. **Alice** generates **kfrags** — these let Bob transform the capsule without seeing the DEK
3. **Alice** sends Bob: manifest + kfrags + share grant
4. **Bob** uses kfrags to produce **cfrags** (capsule fragments)
5. **Bob** combines cfrags to "open" the capsule and recover the DEK
6. **Bob** decrypts the file with the DEK

**No proxy server needed.** The "proxy" in "proxy re-encryption" refers to the math — anyone holding kfrags can transform the capsule for Bob, but they never learn the DEK.

---

## Networking (`nexus-core/src/network/`)

### Architecture

Every NEXUS peer runs a **libp2p swarm** — a multiplexed network node that speaks multiple protocols simultaneously:

```mermaid
graph TB
    subgraph "NexusBehaviour (Swarm)"
        KAD[Kademlia DHT<br/>Peer routing]
        GS[GossipSub<br/>Pub/sub feeds]
        RR[Request-Response<br/>Shard exchange]
        MDNS[mDNS<br/>Local discovery]
        ID[Identify<br/>Peer metadata]
        RC[Relay Client<br/>NAT fallback]
        DCUTR[DCUtR<br/>Hole punching]
        AN[AutoNAT<br/>NAT detection]
    end

    KAD --- GS --- RR --- MDNS --- ID --- RC --- DCUTR --- AN
```

### Protocol Purposes

| Protocol | What It Does | When It's Used |
|----------|-------------|----------------|
| **mDNS** | Broadcasts on local network to find peers | Always (LAN discovery) |
| **Kademlia (DHT)** | Distributed hash table for peer routing | Finding peers by PeerId across the internet |
| **GossipSub** | Pub/sub messaging | Feed updates, presence announcements |
| **Request-Response** | Direct message exchange | Shard transfer, manifest push, kfrag delivery, ping |
| **Identify** | Exchange peer metadata on connect | Every new connection |
| **AutoNAT** | Probe reachability from outside | Periodic (every 60s retry, 300s refresh) |
| **Relay Client** | Connect to relay for NAT traversal | When behind NAT |
| **DCUtR** | Upgrade relayed connection to direct | After relay connection established |

---

### What is a Node?

A **node** is your running NEXUS process connected to the p2p network. It:

1. **Listens** on TCP + QUIC ports for incoming connections
2. **Discovers** peers via mDNS (local) or DHT (global)
3. **Serves shards** — responds to `GetShard` requests with data from your store
4. **Receives shards** — accepts `PushShard` messages from senders
5. **Handles sharing** — receives manifests, kfrags, and share grants

```mermaid
stateDiagram-v2
    [*] --> Starting
    Starting --> Listening: Bind TCP/QUIC ports
    Listening --> Discovering: mDNS broadcast + DHT bootstrap
    Discovering --> Connected: Peers found
    Connected --> Serving: Respond to requests
    Connected --> Sending: Push shards to peers
    Connected --> Receiving: Accept incoming data
    Serving --> Connected
    Sending --> Connected
    Receiving --> Connected
```

**You need a node running to:**
- Send or receive files
- Be discoverable by other peers
- Participate in the network at all

---

### What is a Relay?

A **relay** is just a regular NEXUS node running on a machine with a **public IP address**. It solves one problem: **NAT traversal**.

#### The NAT Problem

```mermaid
graph LR
    subgraph "Home Network A"
        A[Peer A<br/>192.168.1.5]
    end
    subgraph "NAT Router A"
        RA[Router A<br/>Public: 73.x.x.x]
    end
    subgraph "Home Network B"
        B[Peer B<br/>192.168.0.10]
    end
    subgraph "NAT Router B"
        RB[Router B<br/>Public: 98.x.x.x]
    end

    A --- RA
    B --- RB
    RA -.-x|Can't reach each other directly| RB
```

Most home/office networks use NAT — your device has a private IP (192.168.x.x) behind a router. Two NATted peers can't connect directly because neither knows the other's actual address.

#### How the Relay Solves This

```mermaid
sequenceDiagram
    participant A as Peer A (behind NAT)
    participant R as Relay (public IP)
    participant B as Peer B (behind NAT)

    Note over R: Relay has public IP — anyone can reach it

    A->>R: Connect + reserve slot
    B->>R: Connect + reserve slot
    A->>R: "I want to reach Peer B"
    R->>B: Forward A's traffic (circuit)
    B->>A: (via relay) "Let's try direct!"

    Note over A,B: DCUtR hole-punch attempt
    A-->>B: Direct UDP packet ✓
    B-->>A: Direct UDP packet ✓
    Note over A,B: Direct connection established!<br/>Relay no longer needed
```

#### Relay Lifecycle

1. **Reservation** — Your node connects to the relay and says "I'm here, route traffic to me." The relay reserves a slot.
2. **Circuit** — When another peer wants to reach you, they connect through the relay's circuit address: `/ip4/RELAY_IP/tcp/PORT/p2p/RELAY_ID/p2p-circuit/p2p/YOUR_ID`
3. **Upgrade (DCUtR)** — Once both peers are connected via relay, DCUtR coordinates a **hole-punch**: both peers simultaneously send UDP packets to each other, punching through their NATs.
4. **Direct** — If hole-punch succeeds, traffic flows directly. If not, relay continues forwarding (slower but functional).

#### Why Not Just Use a Server?

| Approach | Data Privacy | Single Point of Failure | Scales |
|----------|-------------|------------------------|--------|
| **Central server** | Server sees everything | Yes | Needs money |
| **NEXUS relay** | Relay sees nothing (encrypted) | Can use multiple relays | Relay is lightweight |

The relay never sees your file data — it only forwards opaque encrypted packets. You can run multiple relays for redundancy. Anyone can run a relay.

---

### NAT Traversal — The Full Flow

```mermaid
flowchart TD
    Start[Node starts] --> AN[AutoNAT probes]
    AN -->|"Public"| Done[Direct connections work!]
    AN -->|"Private/Unknown"| NeedRelay[Behind NAT]

    NeedRelay --> Reserve[Connect to relay<br/>Reserve slot]
    Reserve --> Wait[Wait for peers]
    Wait --> PeerArrives[Peer connects via relay circuit]
    PeerArrives --> HP[DCUtR hole-punch attempt]
    HP -->|Success| Direct[Direct connection ✓<br/>Relay unused]
    HP -->|Fail| Relayed[Stay on relay circuit<br/>Slower but works]

    Direct --> Transfer[Transfer shards directly]
    Relayed --> Transfer

    subgraph "Every 5 minutes"
        Reconnect[Re-reserve relay slot<br/>Keep reservation alive]
    end
```

---

### Wire Protocol

NEXUS uses a custom request-response protocol (`/nexus/1.0.0`) for all data exchange:

```mermaid
graph LR
    subgraph "Requests"
        GS[GetShard<br/>Pull a shard by CID]
        PS[PushShard<br/>Send a shard]
        PM[PushManifest<br/>Share a manifest]
        DK[DeliverKfrags<br/>Send access delegation]
        PI[Ping<br/>Health check]
    end

    subgraph "Responses"
        SD[Shard data]
        SNF[ShardNotFound]
        SA[ShardAccepted]
        MA[ManifestAccepted]
        KR[KfragsReceived]
        PO[Pong]
    end

    GS --> SD
    GS --> SNF
    PS --> SA
    PM --> MA
    DK --> KR
    PI --> PO
```

Messages are length-prefixed JSON (4-byte big-endian length + JSON payload). Max message size: 16 MB.

---

## Complete File Transfer Flow

### Sending a File

```mermaid
sequenceDiagram
    participant Sender as Sender App
    participant Core as nexus-core
    participant Store as Shard Store
    participant Net as Network (libp2p)
    participant Recv as Recipient Peer

    Sender->>Core: encrypt_file(path)
    Core->>Core: Generate random DEK
    Core->>Core: AES-256-GCM encrypt file
    Core->>Core: Split into 256KB shards
    Core->>Core: Compute CID for each shard
    Core->>Store: Store all shards locally
    Core->>Core: Encrypt DEK with owner's PRE key
    Core->>Core: Write .nexus manifest
    Core-->>Sender: manifest path

    Sender->>Core: share(manifest, recipient_did)
    Core->>Core: Generate kfrags for recipient
    Core->>Core: Create ShareGrant
    Core-->>Sender: share_grant

    Sender->>Net: send(manifest + grant + shards → recipient)
    Net->>Recv: PushManifest { manifest, share_grant }
    Recv-->>Net: ManifestAccepted
    loop For each shard
        Net->>Recv: PushShard { cid, data }
        Recv-->>Net: ShardAccepted
    end
    Note over Recv: File received!
```

### Receiving & Decrypting a Shared File

```mermaid
sequenceDiagram
    participant App as Recipient App
    participant Core as nexus-core
    participant Store as Shard Store

    App->>Core: decrypt_shared(manifest, share_grant)
    Core->>Core: Deserialize cfrags from ShareGrant
    Core->>Core: Verify cfrags against verifying_key
    Core->>Core: Decrypt DEK using cfrags + own secret key
    Core->>Store: Load all shards (by CID from manifest)
    Core->>Core: Reassemble shards in order
    Core->>Core: AES-256-GCM decrypt with DEK
    Core-->>App: Decrypted file bytes
```

---

## Send Queue & Retry Logic

When a recipient is offline, sends are queued locally:

```mermaid
stateDiagram-v2
    [*] --> Pending: Enqueue send
    Pending --> InProgress: Peer reachable, attempt delivery
    InProgress --> Delivered: All shards + manifest accepted
    InProgress --> Pending: Failed, retry later

    Pending --> Failed: 5 attempts exhausted

    Note right of Pending: Exponential backoff:<br/>30s → 60s → 120s → 240s → 480s

    Failed --> Pending: Manual retry
```

---

## Telemetry (`nexus-core/src/network/telemetry.rs`)

Records structured events to `connectivity.jsonl`:

| Event | Recorded When |
|-------|--------------|
| `NatStatusChanged` | AutoNAT updates reachability |
| `RelayReservation` | Relay connect attempt (success/fail) |
| `HolePunch` | DCUtR attempt (success/fail + peer) |
| `ConnectionEstablished` | Any new peer connection (direct vs relayed) |
| `ConnectionClosed` | Peer disconnects |
| `DialFailure` | Outbound connection attempt fails |

Log rotates at 5 MB (keeps 3 rotated files).

---

## Configuration (`nexus-config.json`)

```json
{
  "relay_servers": [
    "/ip4/1.2.3.4/tcp/4001/p2p/12D3KooW..."
  ],
  "telemetry_enabled": true
}
```

| Field | Default | Purpose |
|-------|---------|---------|
| `relay_servers` | `[]` | Multiaddrs of relay nodes to connect to |
| `telemetry_enabled` | `true` | Whether to record connectivity events |

---

## Desktop App Architecture (Tauri 2.0)

```mermaid
graph TB
    subgraph "Frontend (Svelte)"
        Views[Views:<br/>Main, Shared, SendQueue,<br/>Store, Settings]
        IPC[IPC Layer<br/>Tauri invoke()]
    end

    subgraph "Backend (Rust)"
        Cmds[Tauri Commands]
        State[NodeState<br/>Shared AppState]
        NCore[nexus-core]
    end

    Views --> IPC
    IPC --> Cmds
    Cmds --> State
    State --> NCore
```

The frontend talks to Rust via Tauri's IPC (invoke). No HTTP server, no REST API — direct function calls across the WebView bridge.

---

## Security Model Summary

| Property | How |
|----------|-----|
| **Data at rest** | AES-256-GCM (file body) + Argon2 (vault) |
| **Data in transit** | Noise protocol (libp2p transport encryption) |
| **Identity** | Ed25519 keypair (no central authority) |
| **Access control** | Umbral PRE (delegate without revealing key) |
| **Integrity** | SHA2-256 content addressing (tamper-evident) |
| **Privacy from relay** | Relay only forwards encrypted packets |
| **No central point of failure** | Fully peer-to-peer, any node can relay |

---

## Glossary

| Term | Meaning |
|------|---------|
| **CID** | Content IDentifier — SHA2-256 hash of shard data |
| **DEK** | Data Encryption Key — random AES-256 key per file |
| **DID** | Decentralized IDentifier — `did:key:z6Mk...` |
| **DCUtR** | Direct Connection Upgrade through Relay — hole-punch protocol |
| **kfrag** | Key Fragment — enables re-encryption for a recipient |
| **cfrag** | Capsule Fragment — transformed piece that unlocks access |
| **Manifest** | `.nexus` file describing an encrypted file's shards + DEK |
| **NAT** | Network Address Translation — router hides your real IP |
| **PeerId** | libp2p network identity (base58 encoded multihash of public key) |
| **PRE** | Proxy Re-Encryption — Umbral scheme for access delegation |
| **Relay** | Public node that forwards traffic between NATted peers |
| **Shard** | 256KB chunk of encrypted file data |
| **Swarm** | libp2p's multiplexed network connection manager |
