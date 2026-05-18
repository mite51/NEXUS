# NEXUS Access Control & Push Architecture - Draft v0.1

> **Status:** Planning draft - not yet implemented
> **Date:** 2026-05-18
> **Goal:** Bring back push (authorized only), add access control, groups, folders, and future asset types

---

## 1. Design Principles

1. **Pull remains the default** - public/shared assets are still fetched via `nexus://` links
2. **Push requires explicit authorization** - only contacts with write permission can push to your node
3. **Deny by default** - a new peer has zero access until granted
4. **Revocation is immediate** - removing access takes effect on the next request (no cached grants)
5. **Encryption at rest always** - even "unencrypted on disk" assets are encrypted in transit and in the vault; a symlink/index layer provides OS-level access

---

## 2. Access Control Model

### 2.1 Permission Levels (Bitmask)

```rust
bitflags! {
    struct Permission: u8 {
        const NONE   = 0b0000_0000;
        const READ   = 0b0000_0001;
        const WRITE  = 0b0000_0010;
        const MODIFY = 0b0000_0100;  // overwrite/delete
        // Future bits: ADMIN, SHARE, etc.
    }
}
```

| Bitmask | Alias | Capabilities |
|---------|-------|-------------|
| `0x00` | `none` | No access (blocked/revoked) |
| `0x01` | `read` | Pull assets shared with them |
| `0x03` | `read+write` | Pull + push new assets to your node |
| `0x07` | `read+write+modify` | Pull + push + overwrite/delete existing assets |

Bitmask allows future extension without breaking existing grants (e.g. add `SHARE = 0x08` later).

### 2.1.1 Permission Resolution

Permissions are checked at two layers:

```
Contact-level permission (global default)
  └─ Folder/asset-level permission (takes precedence when set)
```

**Asset/folder-level permissions take precedence over contact-level defaults.** A contact with global `read` CAN be granted `read+write` on a specific folder. The contact-level permission is the default, not a ceiling. This allows granting broad read access while elevating specific folders for collaboration.

Resolution: `effective = asset_grant.unwrap_or(folder_grant.unwrap_or(contact.access))`

### 2.2 Contact Record

```rust
struct Contact {
    did: String,                    // did:nexus:<key>
    label: String,                  // human-friendly name
    peer_id: Option<String>,        // last-known PeerId for routing
    pre_pk: PrePublicKey,           // for PRE re-encryption
    access: AccessLevel,            // global ceiling
    groups: Vec<GroupId>,           // membership list
    created_at: u64,
    updated_at: u64,
}
```

### 2.3 Access Grants (per folder/asset)

```rust
struct AccessGrant {
    target: GrantTarget,            // Asset(id) | Folder(path)
    grantee: Grantee,              // Contact(did) | Group(id)
    level: AccessLevel,            // scoped permission
    expires: Option<u64>,          // optional TTL
}

enum Grantee {
    Contact(String),               // did:nexus:...
    Group(GroupId),
}
```

---

## 3. User Groups

A group is a **logical bucket of DIDs** - no separate key. It exists purely for convenience when granting access to multiple contacts at once.

```rust
struct Group {
    id: GroupId,                    // uuid or short slug
    name: String,                   // "Team Alpha", "Family"
    members: Vec<String>,          // list of DIDs
    default_access: AccessLevel,   // applied when group is granted folder access
}
```

**Why no group key?** A group key would require re-keying on membership change (expensive, complex). Instead, PRE grants are issued per-contact. Groups are just a UI/policy shorthand - "grant read to group X" expands to individual grants at write time.

**Trade-off:** More grants to manage, but revocation is instant (remove one DID, done). No cryptographic coordination needed.

**Mirror note:** Mirrored assets will need their PRE keys regenerated when group membership changes. Since keys are per-contact (not per-group), adding/removing a member from a group means issuing/revoking cfrags for that specific DID on all assets the group can access. For mirrors specifically: the mirror node holds re-encrypted shards - when a member is removed, the mirror's cached cfrags for that DID must be invalidated. On-demand key generation helps here (don't pre-generate cfrags for all group members on every asset; generate when a member actually requests access). For mirrors, the safe approach: mirror stores encrypted shards + per-contact cfrag cache. Revocation = delete that contact's cfrag from mirror. Mirror cannot decrypt without a valid cfrag, so data stays safe even if the mirror is compromised.

---

## 4. Vault Folders

Folders provide logical organization and bulk access control.

```rust
struct VaultFolder {
    path: String,                   // e.g. "/projects/alpha"
    label: Option<String>,         // display name
    default_access: AccessLevel,   // for new assets in this folder
    grants: Vec<AccessGrant>,      // who can access this folder
    inherit: bool,                 // child folders inherit grants?
}
```

### Folder rules:
- `/` is the root - global grants live here
- Push requests **must specify a target folder** (no dumping into root by default)
- A contact can only push to folders where they have `write` or higher
- Folder grants cascade to contained assets unless overridden at asset level

---

## 5. Push Protocol (Authorized)

### 5.1 Flow

```
Sender (has write access)              Receiver (your node)
─────────────────────────────────────────────────────────────
1. PushRequest { did, folder, manifest_preview }
                                        2. Verify: is DID in contacts?
                                           Check: access >= write for folder?
                                           → Reject if unauthorized
                                        3. PushAccepted { session_id }
4. Stream shards to receiver
                                        5. Assemble, verify CIDs
                                        6. Store in target folder
                                        7. PushComplete { asset_id }
```

### 5.2 Push Request Message

```rust
struct PushRequest {
    sender_did: String,
    target_folder: String,          // "/projects/alpha"
    filename: String,               // original name
    total_size: u64,
    shard_count: usize,
    manifest_hash: String,          // so receiver can verify integrity
    asset_type: AssetType,          // File, Stream, ChatSession, etc.
}
```

### 5.3 Authorization Check (receiver side)

```rust
fn authorize_push(req: &PushRequest, contacts: &ContactStore) -> Result<()> {
    let contact = contacts.get(&req.sender_did)?;

    // Global ceiling
    if contact.access < AccessLevel::ReadWrite {
        return Err(Unauthorized);
    }

    // Folder-level check
    let folder_grant = grants.effective_level(&req.sender_did, &req.target_folder);
    if folder_grant < AccessLevel::ReadWrite {
        return Err(Unauthorized);
    }

    Ok(())
}
```

---

## 6. Revocation

- **Remove contact** → all grants invalidated, PRE cfrags deleted, future requests rejected
- **Downgrade access** → takes effect immediately; pending pushes are cancelled
- **Remove from group** → recalculate effective permissions on next request
- **Revoke folder grant** → contact can no longer push to or pull from that folder
- **No re-encryption needed** - existing data stays encrypted with original DEK; access is gate-checked at request time, not at the crypto layer

> Note: If a contact previously *pulled* data, they already have it. Revocation prevents future access, not retroactive secrecy. For true revocation of previously-shared data, you'd need to re-key the asset (re-encrypt with new DEK, new PRE grants). That's a future enhancement.

---

## 7. Asset Types (Extensible)

```rust
enum AssetType {
    File,                           // encrypted blob (current default)
    LinkedFile,                     // unencrypted on disk, encrypted in transit
    ChatSession,                    // message history
    LiveStream,                     // audio/video (real-time)
}
```

### 7.1 LinkedFile / LinkedFolder (unencrypted on disk)

For shared projects, documents that need OS search/indexing:

- **On disk:** plaintext at a known path (e.g. `~/Projects/alpha/`) - can be a single file OR a directory
- **In vault:** index entry pointing to disk path + metadata
- **In transit:** encrypted normally (shard + encrypt + PRE); folders are zipped before encryption
- **Sync model:** file watcher detects changes → re-encrypt + push to authorized peers

```rust
struct LinkedAsset {
    disk_path: PathBuf,            // file or directory
    is_directory: bool,            // if true, zip before encrypt/push
    asset_id: String,              // vault reference
    folder: String,                // vault folder for ACL
    auto_sync: bool,               // push on change?
    last_hash: String,             // detect modifications
    push_live: bool,               // when receiving a push: auto-extract to disk_path?
}
```

**Key insight:** The vault doesn't *contain* the file - it *indexes* it. The file lives where the OS expects it. Nexus handles sync and access control as an overlay.

**Push behavior for linked assets:**
- `push_live = true` → incoming push automatically replaces the on-disk content (dangerous but convenient for trusted collaborators)
- `push_live = false` (default) → incoming push goes to vault only; user manually decides when to update local copy
- For non-git-tracked linked folders, `push_live = false` is strongly recommended

**Directory handling:**
- Linked directories are zipped (deflate) before sharding/encryption for transit
- On the vault side, stored as a single asset (the zip)
- Extraction to disk is a separate explicit step (unless `push_live = true`)

### 7.2 ChatSession

- Treated as a special asset type with append-only semantics
- Messages encrypted individually (each message = mini-asset)
- Contacts with `read` see history; `write` can send messages
- Could reuse the push protocol for message delivery

### 7.3 LiveStream

- Real-time encrypted audio/video
- Uses ephemeral symmetric keys rotated on interval
- Access checked at stream-join time (contact must have `read` for the stream's folder)
- Likely needs a separate transport (UDP/WebRTC-style) vs the shard-based file protocol
- **Future work** - noted here for architecture awareness, not v1

---

## 8. What This Introduces

| Concept | New? | Storage |
|---------|------|---------|
| Contact with access level | New | `contacts.json` in vault |
| Groups | New | `groups.json` in vault |
| Vault Folders | New | `folders.json` + folder metadata |
| Access Grants | New | embedded in folder/asset metadata |
| Push protocol messages | New | Network layer request/response types |
| Asset types enum | New | Manifest field |
| Linked files index | New | `linked.json` + file watcher |

---

## 9. Open Questions

1. ~~**Push approval mode?**~~ → **Resolved:** Auto-accept if authorized. Reject unknown/unauthorized silently — users sort it out themselves.
2. ~~**Conflict resolution for modify?**~~ → **Resolved:** See §9.1 below (asset versioning).
3. ~~**LiveStream feasibility**~~ → **Resolved:** See §9.2 below (direct connection required).
4. ~~**Quota/rate limiting on push?**~~ → **Resolved:** Simple disk space check. If insufficient space, reject push. No complex quota system.
5. ~~**Group nesting?**~~ → **Resolved:** Flat for v1. No nested groups.
6. ~~**Folder depth limit?**~~ → **Resolved:** Windows MAX_PATH (260 chars) as the virtual path limit. Doesn’t reflect actual on-disk layout but provides a reasonable cross-platform restriction.
7. ~~**Chat session UX**~~ → **Resolved:** Separate UI surface. Will design next.

### 9.1 Asset Versioning (Resolved)

Every push creates a **new revision**, not an overwrite. Assets are unique by content+source+time. The manifest holds the display name and a revision log.

```rust
struct AssetManifest {
    asset_id: String,               // stable ID across revisions
    display_name: String,           // human-visible filename
    current_revision: RevisionId,   // points to latest
    revisions: Vec<Revision>,       // ordered history
    // ... existing fields (owner, shards, pre_pk, etc.)
}

struct Revision {
    revision_id: RevisionId,        // hash of (source_did + timestamp + shard_cids)
    source_did: String,             // who pushed this version
    timestamp: u64,                 // arrival time (receiver's clock)
    shard_cids: Vec<String>,        // this revision's actual data
    size: u64,                      // total bytes for this revision
    message: Option<String>,        // optional commit-style note
}
```

**How it works:**
- First push to a folder+filename → creates asset with revision 0
- Subsequent push to same folder+filename by any authorized writer → appends revision
- Each revision has its own shards (old shards preserved until explicitly pruned)
- `asset_id` stays stable - it's the identity. Revisions are the history.
- `revision_id` = hash(source_did ‖ timestamp ‖ sorted shard_cids) - deterministic, unique

**UI implications:**
- Asset list shows latest revision by default
- Expand/click shows revision history (who, when, size)
- Can restore any previous revision (set `current_revision` pointer)
- Can diff sizes, see who changed what and when
- Optional: prune old revisions to reclaim space (manual or policy-based)

**Comparison to S3 versioning:** Similar to AWS S3 bucket versioning - each PUT creates a new version, old versions remain accessible, you can restore or delete specific versions. Simpler though: no lifecycle policies, no delete markers (just prune).

### 9.2 Live Streams (Resolved)

**Requirement:** Direct connection or dedicated transport. Relay is too slow/limited (128KB circuit cap).

**Approach:**
- Live streams require a **direct connection** (hole-punched or LAN)
- If direct connection fails → stream unavailable (not downgraded to relay)
- Transport: encrypted UDP datagrams over libp2p QUIC, or WebRTC DataChannels if browser interop matters later
- Access check at stream-open time (contact needs `read` on the stream's folder)
- Ephemeral symmetric key negotiated per session, rotated every N seconds
- **Not in v1** - architecture note only. When ready, this becomes its own protocol on top of the existing peer connection layer.

---

## 10. Suggested Implementation Order

1. **Contacts + access levels** (foundation for everything else)
2. **Vault folders** (organize assets, attach grants)
3. **Push protocol** (authorized push with folder targeting)
4. **Groups** (convenience layer on top of contacts)
5. **Linked files** (file watcher + index)
6. **Chat sessions** (append-only asset type)
7. **Live streams** (future - separate protocol work)

---

*This is a draft. Poke holes, add constraints, refine.*
