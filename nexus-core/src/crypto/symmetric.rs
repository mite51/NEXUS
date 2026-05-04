//! Symmetric encryption — AES-256-GCM for file body encryption
//!
//! Every file is encrypted with a random Data Encryption Key (DEK).
//! The DEK is then wrapped via PRE for access control.

use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use rand::rngs::OsRng;
use rand::RngCore;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum CryptoError {
    #[error("encryption failed")]
    EncryptionFailed,
    #[error("decryption failed")]
    DecryptionFailed,
    #[error("invalid ciphertext format")]
    InvalidFormat,
}

/// A 256-bit Data Encryption Key
pub type Dek = [u8; 32];

/// Generate a random DEK
pub fn generate_dek() -> Dek {
    let mut dek = [0u8; 32];
    OsRng.fill_bytes(&mut dek);
    dek
}

/// Encrypt data with a DEK (AES-256-GCM)
///
/// Returns: nonce (12 bytes) || ciphertext (with auth tag)
pub fn encrypt_data(plaintext: &[u8], dek: &Dek) -> Result<Vec<u8>, CryptoError> {
    let cipher = Aes256Gcm::new_from_slice(dek).map_err(|_| CryptoError::EncryptionFailed)?;

    let mut nonce_bytes = [0u8; 12];
    OsRng.fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);

    let ciphertext = cipher
        .encrypt(nonce, plaintext)
        .map_err(|_| CryptoError::EncryptionFailed)?;

    // Prepend nonce to ciphertext
    let mut output = Vec::with_capacity(12 + ciphertext.len());
    output.extend_from_slice(&nonce_bytes);
    output.extend_from_slice(&ciphertext);
    Ok(output)
}

/// Decrypt data with a DEK (AES-256-GCM)
///
/// Input: nonce (12 bytes) || ciphertext (with auth tag)
pub fn decrypt_data(encrypted: &[u8], dek: &Dek) -> Result<Vec<u8>, CryptoError> {
    if encrypted.len() < 12 {
        return Err(CryptoError::InvalidFormat);
    }

    let (nonce_bytes, ciphertext) = encrypted.split_at(12);
    let cipher = Aes256Gcm::new_from_slice(dek).map_err(|_| CryptoError::DecryptionFailed)?;
    let nonce = Nonce::from_slice(nonce_bytes);

    cipher
        .decrypt(nonce, ciphertext)
        .map_err(|_| CryptoError::DecryptionFailed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encrypt_decrypt_roundtrip() {
        let dek = generate_dek();
        let plaintext = b"Hello, NEXUS! This is a secret message.";

        let encrypted = encrypt_data(plaintext, &dek).unwrap();
        let decrypted = decrypt_data(&encrypted, &dek).unwrap();

        assert_eq!(plaintext.as_slice(), decrypted.as_slice());
    }

    #[test]
    fn test_wrong_key_fails() {
        let dek1 = generate_dek();
        let dek2 = generate_dek();
        let plaintext = b"secret";

        let encrypted = encrypt_data(plaintext, &dek1).unwrap();
        let result = decrypt_data(&encrypted, &dek2);

        assert!(result.is_err());
    }

    #[test]
    fn test_tampered_ciphertext_fails() {
        let dek = generate_dek();
        let plaintext = b"important data";

        let mut encrypted = encrypt_data(plaintext, &dek).unwrap();
        // Tamper with a byte
        let last = encrypted.len() - 1;
        encrypted[last] ^= 0xFF;

        let result = decrypt_data(&encrypted, &dek);
        assert!(result.is_err());
    }

    #[test]
    fn test_empty_plaintext() {
        let dek = generate_dek();
        let plaintext = b"";

        let encrypted = encrypt_data(plaintext, &dek).unwrap();
        let decrypted = decrypt_data(&encrypted, &dek).unwrap();

        assert_eq!(plaintext.as_slice(), decrypted.as_slice());
    }

    #[test]
    fn test_large_plaintext() {
        let dek = generate_dek();
        let plaintext = vec![0xAB; 1024 * 1024]; // 1MB

        let encrypted = encrypt_data(&plaintext, &dek).unwrap();
        let decrypted = decrypt_data(&encrypted, &dek).unwrap();

        assert_eq!(plaintext, decrypted);
    }
}
