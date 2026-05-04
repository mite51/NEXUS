//! Identity Vault — Argon2id-encrypted storage for keypairs
//!
//! The vault encrypts the master signing key with a user-provided passphrase.

use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use argon2::{Argon2, password_hash::SaltString};
use rand::rngs::OsRng;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::identity::keypair::IdentityKeypair;

#[derive(Error, Debug)]
pub enum VaultError {
    #[error("encryption failed")]
    EncryptionFailed,
    #[error("decryption failed — wrong passphrase?")]
    DecryptionFailed,
    #[error("invalid vault format")]
    InvalidFormat,
}

/// Encrypted identity vault (serializable to disk)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdentityVault {
    /// Argon2id salt (base64)
    pub salt: String,
    /// AES-256-GCM nonce (12 bytes, hex)
    pub nonce: String,
    /// Encrypted private key (hex)
    pub ciphertext: String,
}

impl IdentityVault {
    /// Encrypt a keypair into a vault using a passphrase
    pub fn seal(keypair: &IdentityKeypair, passphrase: &str) -> Result<Self, VaultError> {
        let salt = SaltString::generate(&mut OsRng);

        // Derive 32-byte key from passphrase
        let mut key = [0u8; 32];
        Argon2::default()
            .hash_password_into(passphrase.as_bytes(), salt.as_str().as_bytes(), &mut key)
            .map_err(|_| VaultError::EncryptionFailed)?;

        // Encrypt the private key
        let cipher = Aes256Gcm::new_from_slice(&key).map_err(|_| VaultError::EncryptionFailed)?;
        let mut nonce_bytes = [0u8; 12];
        OsRng.fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);

        let plaintext = keypair.to_secret_bytes();
        let ciphertext = cipher
            .encrypt(nonce, plaintext.as_ref())
            .map_err(|_| VaultError::EncryptionFailed)?;

        Ok(Self {
            salt: salt.as_str().to_string(),
            nonce: hex::encode(nonce_bytes),
            ciphertext: hex::encode(ciphertext),
        })
    }

    /// Decrypt a vault to recover the keypair
    pub fn unseal(&self, passphrase: &str) -> Result<IdentityKeypair, VaultError> {
        // Derive key from passphrase + stored salt
        let mut key = [0u8; 32];
        Argon2::default()
            .hash_password_into(passphrase.as_bytes(), self.salt.as_bytes(), &mut key)
            .map_err(|_| VaultError::DecryptionFailed)?;

        // Decrypt
        let cipher = Aes256Gcm::new_from_slice(&key).map_err(|_| VaultError::DecryptionFailed)?;
        let nonce_bytes = hex::decode(&self.nonce).map_err(|_| VaultError::InvalidFormat)?;
        let nonce = Nonce::from_slice(&nonce_bytes);
        let ciphertext = hex::decode(&self.ciphertext).map_err(|_| VaultError::InvalidFormat)?;

        let plaintext = cipher
            .decrypt(nonce, ciphertext.as_ref())
            .map_err(|_| VaultError::DecryptionFailed)?;

        if plaintext.len() != 32 {
            return Err(VaultError::InvalidFormat);
        }

        let mut bytes = [0u8; 32];
        bytes.copy_from_slice(&plaintext);
        Ok(IdentityKeypair::from_secret_bytes(&bytes))
    }
}

/// Simple hex encoding (no external dep)
mod hex {
    pub fn encode(bytes: impl AsRef<[u8]>) -> String {
        bytes.as_ref().iter().map(|b| format!("{:02x}", b)).collect()
    }

    pub fn decode(s: &str) -> Result<Vec<u8>, ()> {
        if s.len() % 2 != 0 {
            return Err(());
        }
        (0..s.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&s[i..i + 2], 16).map_err(|_| ()))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_seal_unseal() {
        let kp = IdentityKeypair::generate();
        let passphrase = "correct horse battery staple";

        let vault = IdentityVault::seal(&kp, passphrase).unwrap();
        let recovered = vault.unseal(passphrase).unwrap();

        assert_eq!(kp.public_key(), recovered.public_key());
    }

    #[test]
    fn test_wrong_passphrase() {
        let kp = IdentityKeypair::generate();
        let vault = IdentityVault::seal(&kp, "right password").unwrap();
        let result = vault.unseal("wrong password");
        assert!(result.is_err());
    }

    #[test]
    fn test_vault_serialization() {
        let kp = IdentityKeypair::generate();
        let vault = IdentityVault::seal(&kp, "test").unwrap();

        let json = serde_json::to_string(&vault).unwrap();
        let deserialized: IdentityVault = serde_json::from_str(&json).unwrap();

        let recovered = deserialized.unseal("test").unwrap();
        assert_eq!(kp.public_key(), recovered.public_key());
    }
}
