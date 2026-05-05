# NEXUS — Developer Onboarding & Testing Guide

## What is NEXUS?

A peer-to-peer encrypted file sharing application. You encrypt files locally, split them into content-addressed shards, and send them directly to other peers over libp2p — no servers hold your data.

**Key concepts:**
- **Identity** — Ed25519 keypair stored in an encrypted vault (`vault.json`)
- **Encrypt** — Files are AES-256-GCM encrypted, then split into shards
- **Shards** — Content-addressed chunks stored in `.nexus-store/`
- **Manifests** — `.nexus` files that describe how to reassemble shards
- **Proxy Re-Encryption (PRE)** — Share files without decrypting them (Umbral scheme)
- **libp2p** — P2P networking: mDNS (local), Kademlia (DHT), relay + DCUtR (NAT traversal)

---

## 1. Prerequisites

### All Platforms

- **Rust** (stable): https://rustup.rs
  ```bash
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
  source ~/.cargo/env
  rustc --version  # should be 1.77+
  ```

- **Node.js** (v20+): https://nodejs.org
  ```bash
  node --version   # v20.x
  npm --version    # 10.x
  ```

### macOS

```bash
xcode-select --install
```

### Linux (Ubuntu/Debian)

```bash
sudo apt update
sudo apt install -y \
  build-essential \
  libssl-dev \
  libwebkit2gtk-4.1-dev \
  libgtk-3-dev \
  librsvg2-dev \
  libappindicator3-dev \
  curl \
  wget
```

### Windows

- Install [Visual Studio Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/)
  - Select "Desktop development with C++"
- Install [WebView2](https://developer.microsoft.com/en-us/microsoft-edge/webview2/) (usually pre-installed on Win 10+)

---

## 2. Clone & Build

```bash
git clone https://github.com/mite51/NEXUS.git
cd NEXUS
```

### Build the core library + CLI

```bash
cargo build
```

### Run tests (use single thread — some tests have timing dependencies)

```bash
cargo test -- --test-threads=1
```

You should see: **56 tests passing, 0 failures.**

### Build the frontend

```bash
cd nexus-tauri
npm install
npm run build
cd ..
```

### Run the desktop app (dev mode)

```bash
cd nexus-tauri/src-tauri
cargo tauri dev
```

This starts both the Vite dev server and the Tauri window.

---

## 3. Create Your Identity

### Option A: CLI

```bash
cargo run -p nexus-cli -- init --vault vault.json
```

It will prompt for a passphrase. This creates your encrypted keypair vault.

To see your identity (DID + PeerId):

```bash
cargo run -p nexus-cli -- identity --vault vault.json
```

### Option B: Desktop App

Launch the app → it auto-generates an identity on first run. Your DID is shown in the sidebar.

---

## 4. Encrypt a File

### CLI

```bash
cargo run -p nexus-cli -- encrypt myfile.pdf --vault vault.json
```

Output: `myfile.pdf.nexus` (manifest) + shards written to `.nexus-store/`

### Desktop App

Click the **🔒 Encrypt** button in the toolbar → pick a file → done.

---

## 5. Start a Network Node

### CLI

```bash
cargo run -p nexus-cli -- node --vault vault.json
```

The node will:
1. Listen on TCP + QUIC (random ports)
2. Enable mDNS for local peer discovery
3. Print listen addresses like `/ip4/192.168.1.x/tcp/12345/p2p/12D3Koo...`

### Desktop App

The node starts automatically when the app launches.

---

## 6. End-to-End NAT Traversal Testing

This is the real test: two machines on **different networks** establishing a direct connection.

### What You Need

- **Machine A** — your main computer (behind NAT)
- **Machine B** — another device on a different network (phone hotspot, friend's computer, VPS, etc.)
- A **relay server** (public IP) — you can run one yourself or use a public libp2p relay

### Step 1: Set Up a Relay Server (if you don't have one)

The simplest approach — run a relay on any machine with a public IP (VPS, cloud instance):

```bash
# On a VPS / public machine:
git clone https://github.com/mite51/NEXUS.git
cd NEXUS
cargo run -p nexus-cli -- node --vault relay-vault.json
```

Note the listen address, e.g.:
```
/ip4/YOUR_PUBLIC_IP/tcp/PORT/p2p/12D3KooW...RELAY_PEER_ID
```

**Alternative:** Use a public libp2p relay (less reliable):
```
/dnsaddr/bootstrap.libp2p.io/p2p/QmNnooDu7bfjPFoTZYxMNLWUQJyrVwtbZg5gBMjTezGAJN
```

### Step 2: Configure Relay on Both Machines

Edit (or create) `nexus-config.json` in the NEXUS directory on **both** machines:

```json
{
  "relay_servers": [
    "/ip4/YOUR_PUBLIC_IP/tcp/PORT/p2p/RELAY_PEER_ID"
  ],
  "telemetry_enabled": true
}
```

Or in the Desktop App: **Settings → Relay Servers** → paste the relay address → Save.

### Step 3: Start Nodes on Both Machines

**Machine A:**
```bash
cargo run -p nexus-cli -- node --vault vault-a.json
```

Note Machine A's PeerId (printed at startup).

**Machine B:**
```bash
cargo run -p nexus-cli -- node --vault vault-b.json
```

### Step 4: Connect Through Relay

From Machine B, dial Machine A through the relay:

```bash
# The address format for relay-assisted dial:
# /ip4/RELAY_IP/tcp/RELAY_PORT/p2p/RELAY_PEER_ID/p2p-circuit/p2p/MACHINE_A_PEER_ID
```

The CLI `node` command has an interactive prompt. Type:
```
dial /ip4/RELAY_IP/tcp/RELAY_PORT/p2p/RELAY_PEER_ID/p2p-circuit/p2p/MACHINE_A_PEER_ID
```

### Step 5: Observe Hole-Punch

Watch the console output. You should see:
1. `NAT status: Private` — AutoNAT detected you're behind NAT
2. `Relay reserved: ...` — relay reservation established
3. `Hole punch attempt → MACHINE_A` — DCUtR starts
4. `Hole punch succeeded` — direct connection established! 🎉

If hole-punch fails (symmetric NAT), the connection stays relayed (still works, just slower).

### Step 6: Send a File Across NATs

On Machine A, encrypt a file:
```bash
cargo run -p nexus-cli -- encrypt test.txt --vault vault-a.json
```

Send it to Machine B:
```bash
cargo run -p nexus-cli -- send test.txt.nexus --peer MACHINE_B_PEER_ID --vault vault-a.json
```

On Machine B, check received files appear.

### Step 7: Check Telemetry

After the test, check what happened:

```bash
cat .nexus-telemetry/connectivity.jsonl | python3 -m json.tool
```

Or in the Desktop App: **Settings → Network Health** shows live stats.

---

## 7. What to Look For (Success Criteria)

| Event | Good Sign | Problem |
|-------|-----------|---------|
| NAT Detection | `NatStatus: Private` or `Public` | Stuck on `Unknown` (no AutoNAT server reachable) |
| Relay Reservation | `success: true` | `success: false` (relay unreachable or full) |
| Hole Punch | `success: true` | `success: false` (symmetric NAT — expected sometimes) |
| File Transfer | Shards arrive, manifest received | Timeout (peer offline or relay broken) |
| Retry | Send queue retries with backoff | Stuck in `Failed` after 5 attempts |

---

## 8. Troubleshooting

**"NAT status stays Unknown"**
- AutoNAT needs at least one other peer to probe you. Make sure the relay or another peer is reachable.
- Check firewall isn't blocking UDP (QUIC) entirely.

**"Relay reservation fails"**
- Verify the relay address is correct (including the `/p2p/PEER_ID` suffix).
- Relay may have hit connection limits. Try another.

**"Hole punch fails"**
- Symmetric NAT (common on mobile carriers, corporate networks). Expected — relay fallback kicks in.
- Both peers need to have relay reservations for DCUtR to coordinate.

**"File send times out"**
- Peer may have gone offline. Check send queue status: `cargo run -p nexus-cli -- stats`
- Retry manually from the Desktop App send queue.

**Build errors on Linux:**
- Missing `libwebkit2gtk-4.1-dev` — `sudo apt install libwebkit2gtk-4.1-dev`
- Missing `libssl-dev` — `sudo apt install libssl-dev`

---

## 9. Project Layout Quick Reference

```
NEXUS/
├── nexus-core/          Core library
│   ├── src/
│   │   ├── identity/   Ed25519 keypair, DID, vault
│   │   ├── crypto/     AES-GCM encryption, PRE (Umbral)
│   │   ├── storage/    Shard store, CID computation
│   │   ├── network/    libp2p node, behaviour, telemetry, send queue
│   │   └── manifest.rs Manifest format
│   └── tests/           Integration tests
├── nexus-cli/           CLI binary
├── nexus-tauri/         Desktop app
│   ├── src/             Svelte frontend
│   └── src-tauri/       Tauri/Rust backend (commands, state)
├── nexus-config.json    Runtime config (relay servers, telemetry toggle)
├── .nexus-store/        Local shard storage (created at runtime)
├── .nexus-telemetry/    Connectivity logs (created at runtime)
└── .github/workflows/   CI (build + test + release artifacts)
```

---

## 10. Useful CLI Commands

```bash
# Identity
nexus init --vault vault.json
nexus identity --vault vault.json
nexus export-key --vault vault.json

# Files
nexus encrypt <file> --vault vault.json
nexus decrypt <manifest.nexus> --vault vault.json
nexus share <manifest.nexus> --recipient <DID> --vault vault.json
nexus decrypt-shared <manifest.nexus> --vault vault.json

# Network
nexus node --vault vault.json
nexus ping <multiaddr>
nexus send <manifest.nexus> --peer <peer_id> --vault vault.json
nexus fetch <manifest.nexus> --peer <peer_id> --vault vault.json

# Storage
nexus store list
nexus store verify
nexus stats
```

(Run with `cargo run -p nexus-cli --` prefix during development)

---

## Quick Start (TL;DR)

```bash
# 1. Clone & build
git clone https://github.com/mite51/NEXUS.git && cd NEXUS
cargo build && cargo test -- --test-threads=1

# 2. Create identity
cargo run -p nexus-cli -- init --vault vault.json

# 3. Encrypt a file
echo "hello NEXUS" > test.txt
cargo run -p nexus-cli -- encrypt test.txt --vault vault.json

# 4. Start node
cargo run -p nexus-cli -- node --vault vault.json

# 5. Launch desktop app (optional)
cd nexus-tauri && npm install && cd src-tauri && cargo tauri dev
```

You're in the network. 🚀
