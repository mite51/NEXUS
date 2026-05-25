//! Proxy Re-Encryption (PRE) module — Umbral scheme
//!
//! Wraps `umbral-pre` to provide:
//! - Key encapsulation (encrypt a DEK for the owner)
//! - kfrag generation (delegate access to another identity)
//! - Re-encryption (proxy transforms capsule for recipient)
//! - Decryption (owner or delegated recipient decrypts DEK)

use serde::{Deserialize, Serialize};
use thiserror::Error;
use hkdf::Hkdf;
use sha2::Sha256;
use umbral_pre::{
    self as umbral, Capsule, CapsuleFrag, KeyFrag, SecretKey as UmbralSecretKey,
    Signer as UmbralSigner, VerifiedCapsuleFrag,
    DefaultSerialize, DefaultDeserialize,
};

#[derive(Error, Debug)]
pub enum PreError {
    #[error("encryption failed")]
    EncryptionFailed,
    #[error("decryption failed")]
    DecryptionFailed,
    #[error("kfrag generation failed")]
    KfragGenerationFailed,
    #[error("re-encryption failed")]
    ReencryptionFailed,
    #[error("kfrag verification failed")]
    KfragVerificationFailed,
    #[error("cfrag verification failed")]
    CfragVerificationFailed,
    #[error("serialization failed")]
    SerializationFailed,
    #[error("deserialization failed")]
    DeserializationFailed,
}

/// Well-known seed for the "public" PRE identity.
/// Every client embeds this — it's the shared key for public assets.
/// Security note: this is intentionally public. "Public" means anyone can decrypt.
const PUBLIC_PRE_SEED: [u8; 32] = [
    0x6e, 0x65, 0x78, 0x75, 0x73, 0x2d, 0x70, 0x75,
    0x62, 0x6c, 0x69, 0x63, 0x2d, 0x70, 0x72, 0x65,
    0x2d, 0x73, 0x65, 0x65, 0x64, 0x2d, 0x76, 0x31,
    0x2d, 0x32, 0x30, 0x32, 0x36, 0x00, 0x00, 0x00,
]; // "nexus-public-pre-seed-v1-2026\0\0\0"

/// The DID used for public share grants
pub const PUBLIC_DID: &str = "did:nexus:public";

/// Get the well-known public PRE keypair (embedded in every client)
pub fn public_pre_keypair() -> PreKeypair {
    PreKeypair::from_seed_internal(&PUBLIC_PRE_SEED)
}

/// A PRE keypair (secp256k1 via umbral)
/// Internally backed by a 32-byte seed for deterministic reconstruction.
#[derive(Clone)]
pub struct PreKeypair {
    secret_key: UmbralSecretKey,
    /// The seed that deterministically generates this key
    seed: [u8; 32],
}

/// Serializable PRE public key
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PrePublicKey {
    /// Compressed public key bytes (33 bytes secp256k1)
    pub bytes: Vec<u8>,
}

/// Result of encrypting a DEK — capsule + ciphertext
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncryptedDek {
    /// Serialized Capsule (MessagePack via DefaultSerialize)
    pub capsule: Vec<u8>,
    /// Encrypted DEK bytes (ChaCha20Poly1305 DEM output)
    pub ciphertext: Vec<u8>,
}

/// A serializable kfrag for delegation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerializedKfrag {
    pub bytes: Vec<u8>,
}

/// A serializable cfrag (re-encrypted capsule fragment)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerializedCfrag {
    pub bytes: Vec<u8>,
}

/// Signing keypair for kfrag authenticity
#[derive(Clone)]
pub struct PreSigner {
    signer: UmbralSigner,
    verifying_key_bytes: Vec<u8>,
}

/// Serializable verifying key
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerifyingKey {
    pub bytes: Vec<u8>,
}

impl PreKeypair {
    /// Generate a new random PRE keypair
    pub fn generate() -> Self {
        let mut seed = [0u8; 32];
        use rand::RngCore;
        rand::rngs::OsRng.fill_bytes(&mut seed);
        Self::from_seed_internal(&seed)
    }

    fn from_seed_internal(seed: &[u8; 32]) -> Self {
        let factory = umbral::SecretKeyFactory::from_secure_randomness(seed)
            .expect("32-byte seed is valid");
        let sk = factory.make_key(b"nexus-pre-v1");
        Self {
            secret_key: sk,
            seed: *seed,
        }
    }

    /// Get the public key
    pub fn public_key(&self) -> PrePublicKey {
        let pk = self.secret_key.public_key();
        // to_compressed_bytes takes self, so we use the public_key() which returns a new value
        PrePublicKey {
            bytes: pk.to_compressed_bytes().to_vec(),
        }
    }

    /// Export seed for vault storage (32 bytes)
    pub fn to_secret_bytes(&self) -> Vec<u8> {
        self.seed.to_vec()
    }

    /// Import from seed bytes (32 bytes) — deterministic reconstruction
    pub fn from_secret_bytes(bytes: &[u8]) -> Result<Self, PreError> {
        if bytes.len() != 32 {
            return Err(PreError::DeserializationFailed);
        }
        let mut seed = [0u8; 32];
        seed.copy_from_slice(bytes);
        Ok(Self::from_seed_internal(&seed))
    }

    /// Deterministically derive a PRE keypair for a specific peer.
    ///
    /// Given the same `vault_seed` (your vault's PRE seed) and `peer_id`,
    /// this always produces the same keypair. Secure as long as `vault_seed`
    /// remains secret.
    ///
    /// Derivation: HKDF-SHA256(ikm=vault_seed, salt="nexus-pre-peer-v1", info=peer_id)
    pub fn derive_for_peer(vault_seed: &[u8], peer_id: &str) -> Self {
        let hk = Hkdf::<Sha256>::new(Some(b"nexus-pre-peer-v1"), vault_seed);
        let mut derived_seed = [0u8; 32];
        hk.expand(peer_id.as_bytes(), &mut derived_seed)
            .expect("32 bytes is valid HKDF output length");
        Self::from_seed_internal(&derived_seed)
    }

    /// Get a reference to the inner umbral secret key
    fn umbral_sk(&self) -> &UmbralSecretKey {
        &self.secret_key
    }

    /// Encrypt a DEK (or any small plaintext) for this keypair's public key
    pub fn encrypt_dek(&self, dek: &[u8; 32]) -> Result<EncryptedDek, PreError> {
        let pk = self.secret_key.public_key();
        let (capsule, ciphertext) =
            umbral::encrypt(&pk, dek).map_err(|_| PreError::EncryptionFailed)?;

        Ok(EncryptedDek {
            capsule: capsule.to_bytes().map_err(|_| PreError::SerializationFailed)?.to_vec(),
            ciphertext: ciphertext.to_vec(),
        })
    }

    /// Encrypt a DEK for a specific recipient's public key
    pub fn encrypt_dek_for(recipient_pk: &PrePublicKey, dek: &[u8; 32]) -> Result<EncryptedDek, PreError> {
        let pk = pk_from_bytes(&recipient_pk.bytes)?;
        let (capsule, ciphertext) =
            umbral::encrypt(&pk, dek).map_err(|_| PreError::EncryptionFailed)?;

        Ok(EncryptedDek {
            capsule: capsule.to_bytes().map_err(|_| PreError::SerializationFailed)?.to_vec(),
            ciphertext: ciphertext.to_vec(),
        })
    }

    /// Decrypt a DEK that was encrypted for this keypair (owner decryption)
    pub fn decrypt_dek(&self, encrypted: &EncryptedDek) -> Result<[u8; 32], PreError> {
        let capsule = capsule_from_bytes(&encrypted.capsule)?;

        let plaintext = umbral::decrypt_original(&self.secret_key, &capsule, &encrypted.ciphertext)
            .map_err(|_| PreError::DecryptionFailed)?;

        if plaintext.len() != 32 {
            return Err(PreError::DecryptionFailed);
        }
        let mut dek = [0u8; 32];
        dek.copy_from_slice(&plaintext);
        Ok(dek)
    }

    /// Decrypt a DEK that was re-encrypted for this keypair (delegated decryption)
    pub fn decrypt_dek_reencrypted(
        &self,
        encrypted: &EncryptedDek,
        cfrags: &[SerializedCfrag],
        alice_pk: &PrePublicKey,
        verifying_key: &VerifyingKey,
    ) -> Result<[u8; 32], PreError> {
        let capsule = capsule_from_bytes(&encrypted.capsule)?;
        let alice_umbral_pk = pk_from_bytes(&alice_pk.bytes)?;
        let bob_umbral_pk = self.secret_key.public_key();
        let vk = pk_from_bytes(&verifying_key.bytes)?;

        // Verify and collect cfrags
        let verified_cfrags: Result<Vec<VerifiedCapsuleFrag>, PreError> = cfrags
            .iter()
            .map(|sc| {
                let cfrag = cfrag_from_bytes(&sc.bytes)?;
                cfrag
                    .verify(&capsule, &vk, &alice_umbral_pk, &bob_umbral_pk)
                    .map_err(|_| PreError::CfragVerificationFailed)
            })
            .collect();

        let verified = verified_cfrags?;

        let plaintext = umbral::decrypt_reencrypted(
            &self.secret_key,
            &alice_umbral_pk,
            &capsule,
            verified,
            &encrypted.ciphertext,
        )
        .map_err(|_| PreError::DecryptionFailed)?;

        if plaintext.len() != 32 {
            return Err(PreError::DecryptionFailed);
        }
        let mut dek = [0u8; 32];
        dek.copy_from_slice(&plaintext);
        Ok(dek)
    }
}

impl PreSigner {
    /// Create a new signer (for kfrag authenticity)
    pub fn new() -> Self {
        let sk = UmbralSecretKey::random();
        let signer = UmbralSigner::new(sk);
        let vk_bytes = signer.verifying_key().to_compressed_bytes().to_vec();
        Self {
            signer,
            verifying_key_bytes: vk_bytes,
        }
    }

    /// Get the verifying key (share with Bob and proxies)
    pub fn verifying_key(&self) -> VerifyingKey {
        VerifyingKey {
            bytes: self.verifying_key_bytes.clone(),
        }
    }

    /// Generate kfrags to delegate decryption from Alice to Bob
    pub fn generate_kfrags(
        &self,
        alice: &PreKeypair,
        bob_pk: &PrePublicKey,
        threshold: usize,
        shares: usize,
    ) -> Result<Vec<SerializedKfrag>, PreError> {
        let bob_umbral_pk = pk_from_bytes(&bob_pk.bytes)?;

        let verified_kfrags = umbral::generate_kfrags(
            alice.umbral_sk(),
            &bob_umbral_pk,
            &self.signer,
            threshold,
            shares,
            true,  // sign kfrags
            true,  // verify delegating pk
        );

        let result: Result<Vec<SerializedKfrag>, PreError> = verified_kfrags
            .iter()
            .map(|vkf| {
                let kf: KeyFrag = vkf.clone().unverify();
                let bytes = kf.to_bytes().map_err(|_| PreError::SerializationFailed)?.to_vec();
                Ok(SerializedKfrag { bytes })
            })
            .collect();

        result
    }
}

/// Re-encrypt a capsule using a kfrag (performed by a proxy or Bob directly)
pub fn reencrypt(
    encrypted: &EncryptedDek,
    kfrag: &SerializedKfrag,
    alice_pk: &PrePublicKey,
    bob_pk: &PrePublicKey,
    verifying_key: &VerifyingKey,
) -> Result<SerializedCfrag, PreError> {
    let capsule = capsule_from_bytes(&encrypted.capsule)?;
    let kf = kfrag_from_bytes(&kfrag.bytes)?;
    let alice_umbral_pk = pk_from_bytes(&alice_pk.bytes)?;
    let bob_umbral_pk = pk_from_bytes(&bob_pk.bytes)?;
    let vk = pk_from_bytes(&verifying_key.bytes)?;

    // Verify kfrag before re-encrypting
    let verified_kfrag = kf
        .verify(&vk, Some(&alice_umbral_pk), Some(&bob_umbral_pk))
        .map_err(|_| PreError::KfragVerificationFailed)?;

    let verified_cfrag = umbral::reencrypt(&capsule, verified_kfrag);
    let cfrag: CapsuleFrag = verified_cfrag.clone().unverify();
    let bytes = cfrag.to_bytes().map_err(|_| PreError::SerializationFailed)?.to_vec();

    Ok(SerializedCfrag { bytes })
}

// --- Serialization helpers ---

fn capsule_from_bytes(bytes: &[u8]) -> Result<Capsule, PreError> {
    Capsule::from_bytes(bytes).map_err(|_| PreError::DeserializationFailed)
}

fn kfrag_from_bytes(bytes: &[u8]) -> Result<KeyFrag, PreError> {
    KeyFrag::from_bytes(bytes).map_err(|_| PreError::DeserializationFailed)
}

fn cfrag_from_bytes(bytes: &[u8]) -> Result<CapsuleFrag, PreError> {
    CapsuleFrag::from_bytes(bytes).map_err(|_| PreError::DeserializationFailed)
}

fn pk_from_bytes(bytes: &[u8]) -> Result<umbral::PublicKey, PreError> {
    umbral::PublicKey::try_from_compressed_bytes(bytes)
        .map_err(|_| PreError::DeserializationFailed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encrypt_decrypt_owner() {
        let alice = PreKeypair::generate();
        let dek: [u8; 32] = [0xAB; 32];

        let encrypted = alice.encrypt_dek(&dek).unwrap();
        let recovered = alice.decrypt_dek(&encrypted).unwrap();

        assert_eq!(dek, recovered);
    }

    #[test]
    fn test_encrypt_dek_for_recipient() {
        // Alice encrypts a DEK for Bob's public key
        let bob = PreKeypair::generate();
        let bob_pk = bob.public_key();

        let dek: [u8; 32] = [0xCD; 32];

        // Encrypt for Bob (using static method)
        let encrypted = PreKeypair::encrypt_dek_for(&bob_pk, &dek).unwrap();

        // Bob decrypts with his private key
        let recovered = bob.decrypt_dek(&encrypted).unwrap();
        assert_eq!(dek, recovered);
    }

    #[test]
    fn test_encrypt_dek_for_wrong_recipient_fails() {
        let bob = PreKeypair::generate();
        let eve = PreKeypair::generate();
        let bob_pk = bob.public_key();

        let dek: [u8; 32] = [0xEF; 32];

        // Encrypt for Bob
        let encrypted = PreKeypair::encrypt_dek_for(&bob_pk, &dek).unwrap();

        // Eve tries to decrypt — should fail
        let result = eve.decrypt_dek(&encrypted);
        assert!(result.is_err());
    }

    #[test]
    fn test_full_pre_flow() {
        // Alice and Bob generate keypairs
        let alice = PreKeypair::generate();
        let bob = PreKeypair::generate();

        // Alice creates a signer
        let signer = PreSigner::new();
        let vk = signer.verifying_key();

        // Alice encrypts a DEK
        let dek: [u8; 32] = [0x42; 32];
        let encrypted = alice.encrypt_dek(&dek).unwrap();

        // Alice generates kfrags for Bob (threshold=1, shares=1 for direct sharing)
        let kfrags = signer
            .generate_kfrags(&alice, &bob.public_key(), 1, 1)
            .unwrap();
        assert_eq!(kfrags.len(), 1);

        // Proxy (or Bob) re-encrypts the capsule
        let cfrag = reencrypt(
            &encrypted,
            &kfrags[0],
            &alice.public_key(),
            &bob.public_key(),
            &vk,
        )
        .unwrap();

        // Bob decrypts the DEK
        let recovered = bob
            .decrypt_dek_reencrypted(&encrypted, &[cfrag], &alice.public_key(), &vk)
            .unwrap();

        assert_eq!(dek, recovered);
    }

    #[test]
    fn test_pre_threshold() {
        let alice = PreKeypair::generate();
        let bob = PreKeypair::generate();
        let signer = PreSigner::new();
        let vk = signer.verifying_key();

        let dek: [u8; 32] = [0xFF; 32];
        let encrypted = alice.encrypt_dek(&dek).unwrap();

        // 3 shares, threshold 2
        let kfrags = signer
            .generate_kfrags(&alice, &bob.public_key(), 2, 3)
            .unwrap();
        assert_eq!(kfrags.len(), 3);

        // Re-encrypt with 2 out of 3
        let cfrag0 = reencrypt(
            &encrypted,
            &kfrags[0],
            &alice.public_key(),
            &bob.public_key(),
            &vk,
        )
        .unwrap();
        let cfrag1 = reencrypt(
            &encrypted,
            &kfrags[1],
            &alice.public_key(),
            &bob.public_key(),
            &vk,
        )
        .unwrap();

        // Bob decrypts with threshold cfrags
        let recovered = bob
            .decrypt_dek_reencrypted(
                &encrypted,
                &[cfrag0, cfrag1],
                &alice.public_key(),
                &vk,
            )
            .unwrap();

        assert_eq!(dek, recovered);
    }

    #[test]
    fn test_wrong_recipient_fails() {
        let alice = PreKeypair::generate();
        let bob = PreKeypair::generate();
        let eve = PreKeypair::generate();
        let signer = PreSigner::new();
        let vk = signer.verifying_key();

        let dek: [u8; 32] = [0x99; 32];
        let encrypted = alice.encrypt_dek(&dek).unwrap();

        // kfrags for Bob, not Eve
        let kfrags = signer
            .generate_kfrags(&alice, &bob.public_key(), 1, 1)
            .unwrap();

        let cfrag = reencrypt(
            &encrypted,
            &kfrags[0],
            &alice.public_key(),
            &bob.public_key(),
            &vk,
        )
        .unwrap();

        // Eve tries to decrypt — should fail
        let result =
            eve.decrypt_dek_reencrypted(&encrypted, &[cfrag], &alice.public_key(), &vk);
        assert!(result.is_err());
    }

    #[test]
    fn test_keypair_secret_roundtrip() {
        let kp = PreKeypair::generate();
        let bytes = kp.to_secret_bytes();
        let restored = PreKeypair::from_secret_bytes(&bytes).unwrap();
        assert_eq!(kp.public_key(), restored.public_key());
    }

    #[test]
    fn test_kfrag_serialization_roundtrip() {
        let alice = PreKeypair::generate();
        let bob = PreKeypair::generate();
        let signer = PreSigner::new();

        let kfrags = signer
            .generate_kfrags(&alice, &bob.public_key(), 1, 1)
            .unwrap();

        // Serialize to JSON and back
        let json = serde_json::to_string(&kfrags[0]).unwrap();
        let deserialized: SerializedKfrag = serde_json::from_str(&json).unwrap();

        assert_eq!(kfrags[0].bytes, deserialized.bytes);
    }

    #[test]
    fn test_encrypted_dek_serialization() {
        let alice = PreKeypair::generate();
        let dek: [u8; 32] = [0x55; 32];
        let encrypted = alice.encrypt_dek(&dek).unwrap();

        let json = serde_json::to_string(&encrypted).unwrap();
        let deserialized: EncryptedDek = serde_json::from_str(&json).unwrap();

        let recovered = alice.decrypt_dek(&deserialized).unwrap();
        assert_eq!(dek, recovered);
    }

    #[test]
    fn test_derive_for_peer_deterministic() {
        let vault_seed = b"this-is-a-32-byte-vault-seed!!!!"; // 32 bytes
        let peer_id = "12D3KooWKLeXas9R5uXZqjrMmHTEs29WRaNyFmXgBWohZsCGfR1J";

        let kp1 = PreKeypair::derive_for_peer(vault_seed, peer_id);
        let kp2 = PreKeypair::derive_for_peer(vault_seed, peer_id);

        // Same inputs → same keypair
        assert_eq!(kp1.public_key().bytes, kp2.public_key().bytes);
        assert_eq!(kp1.to_secret_bytes(), kp2.to_secret_bytes());
    }

    #[test]
    fn test_derive_for_peer_different_peers() {
        let vault_seed = b"this-is-a-32-byte-vault-seed!!!!";
        let peer_a = "12D3KooWKLeXas9R5uXZqjrMmHTEs29WRaNyFmXgBWohZsCGfR1J";
        let peer_b = "12D3KooWQnAi3e5PXZqjrMmHTEs29WRaNyFmXgBWohZsCGfABCDE";

        let kp_a = PreKeypair::derive_for_peer(vault_seed, peer_a);
        let kp_b = PreKeypair::derive_for_peer(vault_seed, peer_b);

        // Different peers → different keypairs
        assert_ne!(kp_a.public_key().bytes, kp_b.public_key().bytes);
        assert_ne!(kp_a.to_secret_bytes(), kp_b.to_secret_bytes());
    }

    #[test]
    fn test_derive_for_peer_different_vaults() {
        let vault_a = b"vault-seed-aaaaaaaaaaaaaaaaaaaaaa";
        let vault_b = b"vault-seed-bbbbbbbbbbbbbbbbbbbbbb";
        let peer_id = "12D3KooWKLeXas9R5uXZqjrMmHTEs29WRaNyFmXgBWohZsCGfR1J";

        let kp_a = PreKeypair::derive_for_peer(vault_a, peer_id);
        let kp_b = PreKeypair::derive_for_peer(vault_b, peer_id);

        // Different vault seeds → different keypairs even for same peer
        assert_ne!(kp_a.public_key().bytes, kp_b.public_key().bytes);
    }

    #[test]
    fn test_derive_for_peer_encryption_roundtrip() {
        let vault_seed = b"this-is-a-32-byte-vault-seed!!!!";
        let peer_id = "12D3KooWKLeXas9R5uXZqjrMmHTEs29WRaNyFmXgBWohZsCGfR1J";

        let kp = PreKeypair::derive_for_peer(vault_seed, peer_id);

        // Can encrypt/decrypt with a derived keypair
        let dek: [u8; 32] = [0xAB; 32];
        let encrypted = kp.encrypt_dek(&dek).unwrap();
        let recovered = kp.decrypt_dek(&encrypted).unwrap();
        assert_eq!(dek, recovered);
    }
}
