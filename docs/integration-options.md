# Integration Options — Phase 0 Completion

**Date:** 2026-05-04

Based on research into `umbral-pre` (0.11) and `rust-libp2p` (0.57), here are the integration decisions needed to finish Phase 0 and plan Phase 1.

---

## Decision 1: Key Architecture

`umbral-pre` uses **secp256k1** (k256). Our identity system uses **Ed25519**. These are incompatible curves.

### Option A: Dual Keypairs (Recommended)
```
Identity Vault contains:
├── Ed25519 key → DID, libp2p PeerId, message signing
└── secp256k1 key → PRE operations (encrypt, kfrag, decrypt)
```
- **Pro:** Each curve is used for what it's best at. Clean separation.
- **Pro:** libp2p identity = Ed25519 (native support, no conversion)
- **Pro:** PRE operations stay on secp256k1 as designed/audited
- **Con:** Two keys to manage (slightly larger vault)

### Option B: All secp256k1
- Switch identity to secp256k1, drop Ed25519
- **Pro:** Single key for everything
- **Con:** libp2p strongly prefers Ed25519 for PeerId. Secp256k1 PeerIds work but are non-standard.
- **Con:** secp256k1 signatures are larger and slower for identity ops

### Option C: Derive secp256k1 from Ed25519 seed
- Use same 32-byte seed → generate both keys deterministically
- **Pro:** Single seed in vault, two derived keys
- **Con:** Coupling between unrelated crypto systems (bad practice)
- **Con:** No standard for this derivation — custom crypto is a red flag

**Recommendation: Option A.** Two keys, one vault. Clean, audited, standard.

---

## Decision 2: PRE Threshold Parameters for Direct Sharing

NEXUS's PRE model in the SRD describes threshold re-encryption with proxy Ursulas. But for v0.1, we likely want **direct sharing** (no proxy infrastructure yet).

### Option A: Threshold=1, Shares=1 (Simplest)
- Alice generates 1 kfrag, gives it directly to Bob
- Bob can self-re-encrypt (no proxy needed)
- **Pro:** Works immediately, no infrastructure
- **Con:** Threshold security is lost (single point of compromise)

### Option B: Threshold=1, Shares=N (Future-Ready)
- Alice generates N kfrags but only 1 is needed
- Distribute to N mirrors as future proxies
- Any single mirror can re-encrypt for Bob
- **Pro:** Forward-compatible with proxy network
- **Con:** Slightly more complex, still single-threshold

### Option C: Threshold=T, Shares=N (Full Design)
- Requires T out of N proxies to cooperate
- Need proxy infrastructure running
- **Pro:** Maximum security (collusion resistance)
- **Con:** Can't implement until mirror network exists (Phase 2+)

**Recommendation: Start with Option A for Phase 0 tests. Design interfaces to support Option C.** The API should accept `threshold` and `shares` parameters from day one, but v0.1 uses (1,1).

---

## Decision 3: What umbral-pre Encrypts

`umbral-pre::encrypt()` takes arbitrary plaintext and handles both key encapsulation AND symmetric encryption (ChaCha20Poly1305 internally).

### Option A: Let umbral encrypt the DEK (Recommended)
```
File body → AES-256-GCM(DEK) → encrypted shards
DEK (32 bytes) → umbral_pre::encrypt(alice_pk, DEK) → (Capsule, encrypted_DEK)
```
- Capsule + encrypted_DEK stored in manifest
- To share: generate kfrags → proxy re-encrypts Capsule → Bob decrypts DEK → Bob decrypts shards
- **Pro:** Clean layer separation. Our AES-GCM for bulk data, umbral for key wrapping.
- **Pro:** Can re-share without re-encrypting file body

### Option B: Let umbral encrypt file directly
- Skip our AES-GCM layer entirely
- **Con:** umbral uses ChaCha20Poly1305 (not AES). Inconsistent with rest of codebase.
- **Con:** Can't shard-then-encrypt (need to encrypt before sharding)
- **Con:** Re-encrypting large files is expensive

**Recommendation: Option A.** Umbral wraps the DEK only. Bulk encryption stays AES-256-GCM.

---

## Decision 4: Rust Edition

libp2p 0.57 requires edition 2024. Our workspace is currently 2021.

### Option A: Bump whole workspace to 2024 (Recommended)
- Rust 1.95 fully supports it
- No breaking changes for our existing code
- Keeps everything uniform

### Option B: Per-crate edition
- Keep nexus-core at 2021, add nexus-net at 2024
- Possible but unnecessarily complex

**Recommendation: Option A.** Bump to 2024 when we add libp2p (Phase 1).

---

## Decision 5: Identity ↔ libp2p PeerId Relationship

### Option A: DID IS PeerId (Recommended)
```
Ed25519 secret → Ed25519 public → DID = did:nexus:<base58(pubkey)>
                                 → PeerId = PeerId::from(pubkey)
```
- Same key, two representations
- **Pro:** No mapping table needed. DID resolution = peer discovery.
- **Pro:** Simplifies the whole system

### Option B: Separate PeerId from DID
- Different keys for networking vs identity
- **Pro:** Could rotate network identity without changing DID
- **Con:** Need mapping infrastructure, more complex

**Recommendation: Option A.** One key, one identity, two formats.

---

## Summary of Recommended Integration Path

```
┌─────────────── Identity Vault ───────────────┐
│  Ed25519 key → DID + libp2p PeerId + signing │
│  secp256k1 key → PRE (encrypt/kfrag/decrypt) │
│  Argon2id encryption of vault file            │
└──────────────────────────────────────────────┘

┌─────────── File Encryption Flow ─────────────┐
│  1. Generate random DEK (32 bytes)            │
│  2. Encrypt file body: AES-256-GCM(DEK)      │
│  3. Shard encrypted data (content-addressed)  │
│  4. Wrap DEK: umbral::encrypt(alice_pk, DEK)  │
│  5. Store: Capsule + encrypted_DEK in manifest│
└──────────────────────────────────────────────┘

┌─────────── Sharing Flow (PRE) ───────────────┐
│  1. Alice generates kfrags for Bob's PK       │
│  2. kfrag delivered to Bob (direct) or proxy  │
│  3. Capsule re-encrypted using kfrag          │
│  4. Bob decrypts DEK from re-encrypted capsule│
│  5. Bob decrypts shards using recovered DEK   │
└──────────────────────────────────────────────┘
```

## Immediate Next Steps (Phase 0 Completion)

1. Add `umbral-pre = { version = "0.11", features = ["default-rng", "serde"] }` to nexus-core
2. Add `secp256k1` keypair to identity module (alongside Ed25519)
3. Implement `crypto::pre` module with: encrypt_dek, generate_kfrags, reencrypt, decrypt_dek
4. Write integration test: full end-to-end encrypt → share → decrypt flow
5. Update vault to store both keypairs

## Phase 1 Prep (Networking)

6. Bump workspace edition to 2024
7. Create `nexus-net` crate with libp2p
8. Map Ed25519 identity → libp2p PeerId
9. Implement Kademlia peer discovery
10. Implement request-response for shard/kfrag delivery
