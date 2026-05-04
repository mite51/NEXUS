//! Ed25519 keypair generation and management

use ed25519_dalek::{SigningKey, VerifyingKey};
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
