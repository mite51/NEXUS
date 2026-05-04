# NEXUS GUI Design Document

> Design-first approach. Responsiveness and clarity above all.

## Philosophy

NEXUS is a privacy tool. The GUI should feel like a file manager you already know, not a crypto dashboard. The complexity (PRE, sharding, P2P) should be invisible. Users think in files, contacts, and permissions — not capsules, kfrags, and CIDs.

**Design Principles:**
1. **Files first** — Everything revolves around files, not protocols
2. **Zero jargon** — "Share with Bob", not "Generate kfrags for recipient DID"
3. **Progressive disclosure** — Simple by default, power-user details available
4. **Responsive** — Instant feedback. P2P operations show real progress.
5. **Offline-capable** — Everything works locally. Network is optional enhancement.

---

## Architecture

```
┌─────────────────────────────────────────────┐
│                 Tauri Shell                   │
│  ┌─────────────────────────────────────────┐ │
│  │          Svelte + TypeScript UI          │ │
│  │                                          │ │
│  │   Sidebar │ Content Area │ Detail Panel  │ │
│  │           │              │               │ │
│  └─────────────────────────────────────────┘ │
│  ┌─────────────────────────────────────────┐ │
│  │           Tauri Commands (IPC)           │ │
│  │  encrypt, decrypt, share, fetch, send   │ │
│  │  node_start, node_status, identity      │ │
│  └─────────────────────────────────────────┘ │
│  ┌─────────────────────────────────────────┐ │
│  │        nexus-core (Rust backend)         │ │
│  │  crypto | storage | network | identity  │ │
│  └─────────────────────────────────────────┘ │
└─────────────────────────────────────────────┘
```

The Tauri backend exposes nexus-core functions as IPC commands. The Svelte frontend calls them and renders state. No business logic in the frontend.

---

## Screen Layout

### Primary Layout: Three-Panel

```
┌──────────┬──────────────────────┬───────────────┐
│          │                      │               │
│ Sidebar  │    Content Area      │  Detail Panel │
│          │                      │  (contextual) │
│ - Files  │  File grid/list      │               │
│ - Shared │  with thumbnails     │  File info    │
│ - Peers  │  or status rows      │  Permissions  │
│ - Store  │                      │  Actions      │
│          │                      │               │
│          │                      │               │
│ ──────── │                      │               │
│ Identity │                      │               │
│ Node ●   │                      │               │
└──────────┴──────────────────────┴───────────────┘
```

On small screens (mobile/narrow): sidebar collapses to icon rail, detail panel becomes bottom sheet.

### Sidebar Sections

| Section | Purpose |
|---------|---------|
| **My Files** | Files you've encrypted. Your vault. |
| **Shared With Me** | Files others have shared with you (via PRE grants) |
| **Peers** | Connected peers, discovery status |
| **Store** | Local shard store stats, health |

**Bottom of sidebar:**
- Identity badge (DID truncated, copy on click)
- Node status indicator (● green = running, ○ gray = offline)

---

## Views

### 1. My Files (default view)

Grid of encrypted files. Each card shows:
```
┌──────────────────────┐
│  📄  document.pdf     │
│                       │
│  Encrypted 2h ago     │
│  3 shards │ 2.4 MB    │
│  Shared with: 1 peer  │
└──────────────────────┘
```

**Actions (on hover / right-click / detail panel):**
- **Open** — Decrypt and open with system viewer
- **Share** — Pick a contact, generate share grant
- **Send** — Push to a connected peer
- **Export** — Save decrypted copy
- **Delete** — Remove from vault (with confirmation)

**Drag & drop**: Drop files onto the window to encrypt them.

### 2. Shared With Me

Same grid layout, but shows:
- Who shared it (their DID / nickname)
- When received
- Status: "Ready" (shards local) or "Needs fetch" (shards on remote peer)

### 3. Peers

```
┌──────────────────────────────────────────┐
│ 🟢 Alice's Node                          │
│    PeerId: 12D3KooW...abc               │
│    Connected via: mDNS (local)           │
│    Shards available: 47                   │
│                                           │
│ 🟢 Bob's Laptop                          │
│    PeerId: 12D3KooW...xyz               │
│    Connected via: QUIC (direct)          │
│    Last seen: 3m ago                      │
│                                           │
│ ⚪ Mirror-1 (offline)                    │
│    Last seen: 2d ago                      │
└──────────────────────────────────────────┘
```

**Actions:**
- Ping peer
- Send file to peer
- Add bootstrap address manually

### 4. Store

```
┌──────────────────────────────────────────┐
│ 📦 Local Shard Store                      │
│                                           │
│ Shards: 142                               │
│ Size:   34.2 MB                           │
│ Health: ✓ All verified                    │
│                                           │
│ [Verify All]  [Clean Orphans]            │
└──────────────────────────────────────────┘
```

---

## Core Interactions

### Encrypt a File

```
User drags file onto window
  → Progress bar: "Encrypting..."
  → Progress bar: "Sharding... (4 shards)"
  → Progress bar: "Storing..."
  → Toast: "✓ document.pdf encrypted"
  → File appears in My Files grid
```

Time target: < 1s for files under 10MB. Always show progress for larger files.

### Share a File

```
User clicks Share on a file
  → Contact picker appears (known peers / paste DID)
  → Select recipient
  → "Generating share grant..."
  → Toast: "✓ Shared with Bob"
  → If Bob is online: offer to send now
```

### Fetch a Shared File

```
"Shared With Me" shows a file with status "Needs fetch"
  → User clicks "Fetch"
  → Progress: "Connecting to peer..."
  → Progress: "Fetching shard 1/4..."
  → Progress: "Fetching shard 2/4..."
  → Progress: "Decrypting..."
  → Toast: "✓ secret.pdf received"
```

### Send a File

```
User drags file onto a peer in Peers view
  → Or: clicks Send on a file, picks recipient
  → Progress: "Connecting..."
  → Progress: "Pushing shard 1/4..."
  → Progress: "Delivering manifest..."
  → Toast: "✓ Sent to Bob"
```

---

## Visual Design

### Color Palette

| Role | Color | Usage |
|------|-------|-------|
| Background | `#0f0f0f` | Main window |
| Surface | `#1a1a1a` | Cards, panels |
| Border | `#2a2a2a` | Subtle borders |
| Text primary | `#e0e0e0` | Main text |
| Text secondary | `#888888` | Labels, metadata |
| Accent | `#6366f1` | Actions, links (indigo) |
| Success | `#22c55e` | Connected, verified |
| Warning | `#f59e0b` | Pending, offline |
| Error | `#ef4444` | Failed, disconnected |

Dark theme by default. Light theme later if requested.

### Typography

- **Headings**: Inter, 600 weight
- **Body**: Inter, 400 weight
- **Monospace** (DIDs, CIDs): JetBrains Mono, 400 weight
- **Size scale**: 12px (caption) / 14px (body) / 16px (subtitle) / 20px (title)

### Animations

- Card hover: subtle lift (transform + shadow)
- Progress bars: smooth fill with pulse during network wait
- Toast notifications: slide in from top-right, auto-dismiss 3s
- Panel transitions: 200ms ease-out

### Responsive Breakpoints

| Breakpoint | Layout |
|-----------|--------|
| > 1200px | Three-panel (sidebar + content + detail) |
| 800-1200px | Two-panel (sidebar + content, detail as overlay) |
| < 800px | Single panel (bottom nav, stacked views) |
| Mobile (Tauri mobile) | Bottom tab bar, full-screen views, sheets |

---

## Tauri IPC Commands

These map directly to nexus-core operations:

```rust
// Identity
#[tauri::command] fn get_identity(vault: &str, pass: &str) -> Result<IdentityInfo>
#[tauri::command] fn create_identity(vault: &str, pass: &str) -> Result<IdentityInfo>
#[tauri::command] fn export_public_key(vault: &str, pass: &str) -> Result<String>

// File operations
#[tauri::command] fn encrypt_file(path: &str, vault: &str, pass: &str) -> Result<EncryptResult>
#[tauri::command] fn decrypt_file(manifest: &str, vault: &str, pass: &str) -> Result<String>
#[tauri::command] fn decrypt_shared(manifest: &str, share: &str, vault: &str, pass: &str) -> Result<String>

// Sharing
#[tauri::command] fn share_file(manifest: &str, recipient_pk: &str, vault: &str, pass: &str) -> Result<ShareResult>

// Network
#[tauri::command] fn start_node(vault: &str, pass: &str) -> Result<NodeInfo>
#[tauri::command] fn stop_node() -> Result<()>
#[tauri::command] fn get_node_status() -> Result<NodeStatus>
#[tauri::command] fn get_peers() -> Result<Vec<PeerInfo>>

// Transfer
#[tauri::command] fn fetch_file(manifest: &str, peer: &str, share: Option<&str>, vault: &str, pass: &str) -> Result<String>
#[tauri::command] fn send_file(manifest: &str, peer: &str, share: Option<&str>, vault: &str, pass: &str) -> Result<()>

// Store
#[tauri::command] fn get_store_stats() -> Result<StoreStats>
#[tauri::command] fn list_shards() -> Result<Vec<String>>
#[tauri::command] fn verify_store() -> Result<VerifyResult>
```

### Event Streaming

For long operations, use Tauri's event system:

```rust
// Backend emits progress events
app_handle.emit("transfer-progress", TransferProgress {
    operation: "fetch",
    file: "secret.pdf",
    shard_current: 2,
    shard_total: 4,
    bytes_transferred: 524288,
    status: "downloading",
});

// Frontend subscribes
listen('transfer-progress', (event) => {
    updateProgressBar(event.payload);
});
```

---

## State Management (Svelte)

```
stores/
  identity.ts     — current identity (DID, vault status)
  files.ts        — encrypted files list
  shared.ts       — shared-with-me files
  peers.ts        — connected peers
  node.ts         — node running status
  transfers.ts    — active transfers with progress
```

Use Svelte stores (writable/derived). No Redux-like complexity. Each store talks to Tauri IPC directly.

---

## File Structure

```
nexus-tauri/
  src-tauri/
    src/
      main.rs         — Tauri app setup + commands
      commands/
        identity.rs   — Identity IPC handlers
        files.rs      — Encrypt/decrypt handlers  
        network.rs    — Node + transfer handlers
        store.rs      — Store query handlers
      state.rs        — Shared app state (node handle, vault)
    Cargo.toml
    tauri.conf.json
  src/
    App.svelte        — Root layout
    lib/
      components/
        Sidebar.svelte
        FileGrid.svelte
        FileCard.svelte
        DetailPanel.svelte
        PeerList.svelte
        StoreView.svelte
        ProgressBar.svelte
        Toast.svelte
        ContactPicker.svelte
      stores/
        identity.ts
        files.ts
        shared.ts
        peers.ts
        node.ts
        transfers.ts
      views/
        MyFiles.svelte
        SharedWithMe.svelte
        Peers.svelte
        Store.svelte
        Setup.svelte   — First-run identity creation
      utils/
        ipc.ts         — Typed Tauri command wrappers
        format.ts      — Human-readable sizes, dates, DID truncation
    app.css            — Global styles + CSS variables
  package.json
  svelte.config.js
  vite.config.ts
```

---

## Implementation Plan

### Phase A: Shell + Identity (1 session)
- Tauri app scaffold with Svelte
- First-run setup screen (create identity)
- Identity display in sidebar
- Vault unlock flow (passphrase modal)

### Phase B: File Operations (1-2 sessions)
- My Files view with file grid
- Drag-and-drop encrypt
- Click-to-decrypt
- Detail panel with file info

### Phase C: Network Integration (1-2 sessions)
- Start/stop node from GUI
- Peers view with live discovery
- Fetch and send from GUI
- Transfer progress events

### Phase D: Sharing (1 session)
- Contact picker
- Share grant generation
- Shared With Me view
- Receive notifications

### Phase E: Polish (1 session)
- Responsive layout
- Animations and transitions
- Error handling UX
- Toast notifications
- Keyboard shortcuts

---

## Open Questions for Jason

1. **Vault unlock UX**: Always prompt for passphrase on app launch? Or unlock once and keep in memory for the session? (Security vs. convenience tradeoff)
2. **File previews**: Show decrypted thumbnails for images? This means decrypting on hover/load which has security implications.
3. **Contact management**: Simple (paste DID/pubkey) or should we build a contact book with nicknames?
4. **Mobile priority**: Start with desktop and adapt, or design mobile-first?
5. **Tray icon**: Run node in background with system tray? Useful for always-on shard serving.

---

*This document should be reviewed before implementation begins. The design is intentionally opinionated — push back on anything that doesn't feel right.*
