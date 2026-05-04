# Research: rust-umbral (PRE Integration)

**Date:** 2026-05-04
**Status:** Ready for integration

## Crate Info

- **Crate name:** `umbral-pre`
- **Version:** `0.11.0` (on crates.io)
- **License:** GPL-3.0-only ✅ (matches our project license)
- **Repo:** https://github.com/nucypher/rust-umbral
- **Features:** `no_std` compatible, has `serde` support, uses secp256k1 (k256)
- **Internal crypto:** ChaCha20Poly1305 for DEM, HKDF for key derivation, secp256k1 for EC ops

## Cargo.toml Dependency

```toml
umbral-pre = { version = "0.11", features = ["default-rng", "serde"] }
```

## Key Types

| Type | Description |
|------|-------------|
| `SecretKey` | Private key (secp256k1 scalar) |
| `PublicKey` | Public key (secp256k1 point) |
| `Signer` | Signing key for kfrag authenticity |
| `Capsule` | Encapsulated symmetric key (travels with ciphertext) |
| `KeyFrag` / `VerifiedKeyFrag` | Re-encryption key fragments |
| `CapsuleFrag` / `VerifiedCapsuleFrag` | Re-encrypted capsule fragments |

## Full PRE Flow (from lib.rs docs)

```rust
use umbral_pre::*;

// === Key Generation ===
let alice_sk = SecretKey::random();
let alice_pk = alice_sk.public_key();
let signer = Signer::new(SecretKey::random());
let verifying_pk = signer.verifying_key();

let bob_sk = SecretKey::random();
let bob_pk = bob_sk.public_key();

// === Encryption (by anyone with Alice's public key) ===
let plaintext = b"secret data";
let (capsule, ciphertext) = encrypt(&alice_pk, plaintext).unwrap();

// === Alice can decrypt directly ===
let decrypted = decrypt_original(&alice_sk, &capsule, &ciphertext).unwrap();

// === Alice generates kfrags for Bob (threshold scheme) ===
let shares = 3;    // total fragments
let threshold = 2;  // minimum needed to decrypt
let verified_kfrags = generate_kfrags(
    &alice_sk, &bob_pk, &signer,
    threshold, shares,
    true,   // sign kfrags
    true    // delegating pk is verified
);

// === Proxies re-encrypt (each proxy gets one kfrag) ===
let kfrag0 = verified_kfrags[0].clone().unverify(); // simulate network transfer
let kfrag1 = verified_kfrags[1].clone().unverify();

// Proxy 0 verifies kfrag and re-encrypts
let vk0 = kfrag0.verify(&verifying_pk, Some(&alice_pk), Some(&bob_pk)).unwrap();
let cfrag0 = reencrypt(&capsule, vk0);

// Proxy 1
let vk1 = kfrag1.verify(&verifying_pk, Some(&alice_pk), Some(&bob_pk)).unwrap();
let cfrag1 = reencrypt(&capsule, vk1);

// === Bob decrypts using threshold cfrags ===
let vc0 = cfrag0.unverify().verify(&capsule, &verifying_pk, &alice_pk, &bob_pk).unwrap();
let vc1 = cfrag1.unverify().verify(&capsule, &verifying_pk, &alice_pk, &bob_pk).unwrap();

let plaintext_bob = decrypt_reencrypted(
    &bob_sk, &alice_pk, &capsule,
    [vc0, vc1],
    &ciphertext
).unwrap();
```

## API Quirks & Notes

### 1. Separate key types from our Ed25519 identity
`umbral-pre` uses **secp256k1** (via `k256` crate), NOT Ed25519. Our identity system uses Ed25519.

**Options:**
- **A) Dual keypairs**: Ed25519 for identity/signing, secp256k1 for PRE operations
- **B) Switch identity to secp256k1**: Simplifies but limits future flexibility
- **C) Derive secp256k1 from Ed25519 seed**: Use same 32-byte secret, generate both key types

**Recommendation: Option A (dual keypairs)**. Different crypto operations benefit from different curves. Store both in the vault.

### 2. Signer required for kfrag generation
Alice needs a separate signing keypair (`Signer`) to authenticate kfrags. This prevents a proxy from forging kfrags. The `verifying_pk` must be shared with Bob and proxies.

### 3. Threshold scheme is mandatory
Even for "give Bob full access" you must set `shares` and `threshold`. For direct sharing (no proxy), use `shares=1, threshold=1`.

### 4. umbral-pre encrypts plaintext directly
The `encrypt()` function handles both key encapsulation AND data encryption internally (using ChaCha20Poly1305 DEM). We have two integration patterns:

- **Pattern A — Let umbral handle everything**: Encrypt the DEK as the "plaintext" passed to umbral. File body encrypted separately with AES-256-GCM using that DEK.
- **Pattern B — Use umbral for the DEK only**: Same as A, but clearer separation.

Both are equivalent. Pattern A is simpler.

### 5. Capsule must travel with the ciphertext
The `Capsule` is NOT secret — it's the encapsulated key that proxies re-encrypt. Store it alongside the encrypted file (in the shard manifest).

### 6. Serialization
With `serde` feature enabled, all types implement Serialize/Deserialize. Use this for storage and network transfer.

## Compatibility

- **Rust edition 2021** ✅ (our workspace is 2021)
- **no_std** ✅ (future-proofs for embedded/WASM if ever needed)
- **Uses k256 0.13** — may conflict if we have other secp256k1 crates; shouldn't be an issue
- **GPL-3.0** ✅ matches our project license
