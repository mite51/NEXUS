//! Ed25519 keypair generation and management

use ed25519_dalek::{SigningKey, VerifyingKey, Signer};
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};

/// A NEXUS identity keypair (Ed25519)
#[derive(Clone)]
pub struct IdentityKeypair {
    signing_key: SigningKey,
}

/// Serializable public key representation
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PublicIdentity {
    pub public_key: [u8; 32],
}

impl IdentityKeypair {
    /// Generate a new random keypair
    pub fn generate() -> Self {
        let signing_key = SigningKey::generate(&mut OsRng);
        Self { signing_key }
    }

    /// Get the public verifying key
    pub fn public_key(&self) -> VerifyingKey {
        self.signing_key.verifying_key()
    }

    /// Get the public identity (serializable)
    pub fn public_identity(&self) -> PublicIdentity {
        PublicIdentity {
            public_key: self.public_key().to_bytes(),
        }
    }

    /// Export private key bytes (for vault storage)
    pub fn to_secret_bytes(&self) -> [u8; 32] {
        self.signing_key.to_bytes()
    }

    /// Import from private key bytes
    pub fn from_secret_bytes(bytes: &[u8; 32]) -> Self {
        let signing_key = SigningKey::from_bytes(bytes);
        Self { signing_key }
    }

    /// Convert to a libp2p Keypair (same Ed25519 key → same PeerId as DID)
    pub fn to_libp2p_keypair(&self) -> libp2p::identity::Keypair {
        // ed25519-dalek uses 32-byte seed; libp2p wants the same
        let bytes = self.signing_key.to_bytes();
        let libp2p_kp = libp2p::identity::Keypair::ed25519_from_bytes(bytes)
            .expect("valid ed25519 seed");
        libp2p_kp
    }

    /// Get the libp2p PeerId derived from this keypair
    pub fn peer_id(&self) -> libp2p::PeerId {
        self.to_libp2p_keypair().public().to_peer_id()
    }

    /// Sign arbitrary bytes with this identity key
    pub fn sign(&self, message: &[u8]) -> Vec<u8> {
        let sig = self.signing_key.sign(message);
        sig.to_bytes().to_vec()
    }

    /// Get the DID string for this keypair
    pub fn did(&self) -> String {
        crate::identity::did::Did::from_public_identity(&self.public_identity()).0
    }
}

/// Verify an Ed25519 signature given a 32-byte public key, message, and 64-byte signature.
pub fn verify_signature(public_key: &[u8; 32], message: &[u8], signature: &[u8]) -> bool {
    use ed25519_dalek::Verifier;

    let Ok(verifying_key) = VerifyingKey::from_bytes(public_key) else {
        return false;
    };

    let sig_bytes: [u8; 64] = match signature.try_into() {
        Ok(b) => b,
        Err(_) => return false,
    };

    let sig = ed25519_dalek::Signature::from_bytes(&sig_bytes);
    verifying_key.verify(message, &sig).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_keypair() {
        let kp = IdentityKeypair::generate();
        let pub_id = kp.public_identity();
        assert_eq!(pub_id.public_key.len(), 32);
    }

    #[test]
    fn test_roundtrip_secret_bytes() {
        let kp = IdentityKeypair::generate();
        let bytes = kp.to_secret_bytes();
        let restored = IdentityKeypair::from_secret_bytes(&bytes);
        assert_eq!(kp.public_key(), restored.public_key());
    }

    #[test]
    fn test_unique_keypairs() {
        let kp1 = IdentityKeypair::generate();
        let kp2 = IdentityKeypair::generate();
        assert_ne!(kp1.public_identity(), kp2.public_identity());
    }
}
