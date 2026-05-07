# Pull-Only Sharing Architecture

## Principles
- No unsolicited push. Data flows only in response to authenticated requests.
- Offline = unavailable (until mirroring is added later).
- Share link is dumb — just a pointer. Recipient fetches when ready.

## File Layout (per encrypted asset)

```
.nexus-store/
  manifests/
    <asset-id>.nexus          ← encrypted shard manifest (CIDs, sizes, original name)
  rfrags/
    <asset-id>/
      <did-base58>.rfrag      ← re-encryption fragment for that user
  shards/
    <cid>.shard               ← encrypted chunks (unchanged)
```

- Manifest is encrypted with the same DEK as the file shards
- rfrag allows a specific recipient to re-derive the DEK via PRE
- Asset ID = CID of the manifest (or a stable hash)

## Share Link Format

```
nexus://<peer-id>/asset/<asset-id>
```

Encodes: who to connect to + what to request. Nothing secret in the link itself.

## Request Protocol (libp2p)

### `/nexus/asset/1.0.0` — Pull protocol

**Request flow:**
1. Requester connects to sharer's node
2. Sends: `AssetRequest { asset_id, requester_did, signature }`
   - Signature = sign(asset_id, requester's identity key) — proves DID ownership
3. Sharer's node checks:
   - Does `rfrags/<asset-id>/<requester-did-base58>.rfrag` exist?
   - Is signature valid for that DID?
4. If authorized → stream: rfrag + encrypted manifest + shards (in order)
5. If not → reject with "unauthorized"

**Recipient side:**
1. Receives rfrag → uses PRE to recover DEK
2. Decrypts manifest → gets shard CIDs + metadata
3. Receives shards → reassembles + decrypts file

### Simplification for v1
- Single streaming response: rfrag || manifest || shard[0] || shard[1] || ...
- Each piece length-prefixed (u32 LE + bytes)
- Requester can verify shard CIDs match manifest after decryption

## UI Changes

### DetailPanel actions (per file):
- 🔓 Decrypt
- 🔗 Share (opens share panel)
- ✏️ Rename
- 📦 Export Bundle
- 🗑 Delete

### Share Panel (replaces contact picker flow):
```
┌─────────────────────────────────────────┐
│ Share: document.pdf                      │
│                                          │
│ Link: nexus://12D3Koo.../asset/Qm...    │
│ [📋 Copy]                                │
│                                          │
│ Authorized users:                        │
│  • Jason (did:nexus:2Sx...)    [Remove]  │
│  • Alice (did:nexus:7Fg...)    [Remove]  │
│                                          │
│ [+ Add User]                             │
└─────────────────────────────────────────┘
```

- **Add User** → contact picker → generates rfrag → saves to disk
- **Remove** → deletes rfrag file (revokes access)
- **Copy link** → clipboard

## What Gets Removed
- `Send to Peer` button
- `queueSend` IPC function
- `send_queue` module in commands.rs
- `delivery` module in nexus-core (push logic)
- CLI `send` command
- Any node event handling for inbound pushed data

## What Gets Added
- `/nexus/asset/1.0.0` pull protocol handler on the node
- `AssetRequest` / `AssetResponse` message types
- Signature verification for requests
- Share panel UI component
- `get_share_link(manifest_path)` → returns nexus:// URL
- `list_shared_users(asset_id)` → lists rfrag files
- `revoke_share(asset_id, did)` → deletes rfrag

## What Gets Modified
- `share_file` → now writes rfrag to `rfrags/<asset-id>/` instead of a standalone .share file
- Node protocol handler → adds the asset pull handler
- Manifest storage → moves to `.nexus-store/manifests/` (encrypted)
- CLI `fetch` → renamed to just the standard way to pull (already pull-based, just needs auth)

## Implementation Order
1. Restructure storage layout (manifests/ + rfrags/ directories)
2. Implement pull protocol on node (request → auth check → stream response)
3. Wire share_file to write rfrags to new location
4. Add share link generation
5. Build share panel UI
6. Remove push infrastructure (send, delivery, queueSend)
7. Update CLI (remove `send`, update `fetch` to use auth)

## Open for Later
- Mirroring (push to trusted mirror nodes)
- Messaging/notifications ("hey, I shared something with you")
- Email fallback notifications
- DHT-based peer discovery (find sharer without knowing their addr)
