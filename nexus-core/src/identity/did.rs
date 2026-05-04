//! Decentralized Identifier (DID) generation
//!
//! Format: did:nexus:<base58-encoded-public-key>

use crate::identity::keypair::PublicIdentity;
use serde::{Deserialize, Serialize};

/// A NEXUS Decentralized Identifier
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct Did(pub String);

impl Did {
    /// Create a DID from a public identity
    pub fn from_public_identity(identity: &PublicIdentity) -> Self {
        let encoded = bs58_encode(&identity.public_key);
        Did(format!("did:nexus:{}", encoded))
    }

    /// Extract the public key bytes from a DID string
    pub fn to_public_key_bytes(&self) -> Option<[u8; 32]> {
        let prefix = "did:nexus:";
        if !self.0.starts_with(prefix) {
            return None;
        }
        let encoded = &self.0[prefix.len()..];
        bs58_decode(encoded)
    }
}

impl std::fmt::Display for Did {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Simple base58 encoding (Bitcoin alphabet)
fn bs58_encode(bytes: &[u8]) -> String {
    const ALPHABET: &[u8] = b"123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";

    if bytes.is_empty() {
        return String::new();
    }

    // Count leading zeros
    let leading_zeros = bytes.iter().take_while(|&&b| b == 0).count();

    // Convert to base58
    let mut digits: Vec<u8> = Vec::new();
    for &byte in bytes {
        let mut carry = byte as u32;
        for digit in digits.iter_mut() {
            carry += (*digit as u32) * 256;
            *digit = (carry % 58) as u8;
            carry /= 58;
        }
        while carry > 0 {
            digits.push((carry % 58) as u8);
            carry /= 58;
        }
    }

    let mut result = String::with_capacity(leading_zeros + digits.len());
    for _ in 0..leading_zeros {
        result.push('1');
    }
    for &digit in digits.iter().rev() {
        result.push(ALPHABET[digit as usize] as char);
    }

    result
}

/// Simple base58 decoding
fn bs58_decode(s: &str) -> Option<[u8; 32]> {
    const ALPHABET: &[u8] = b"123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";

    let mut bytes: Vec<u8> = Vec::new();
    let leading_ones = s.chars().take_while(|&c| c == '1').count();

    for ch in s.chars() {
        let idx = ALPHABET.iter().position(|&a| a == ch as u8)? as u32;
        let mut carry = idx;
        for byte in bytes.iter_mut() {
            carry += (*byte as u32) * 58;
            *byte = (carry & 0xFF) as u8;
            carry >>= 8;
        }
        while carry > 0 {
            bytes.push((carry & 0xFF) as u8);
            carry >>= 8;
        }
    }

    for _ in 0..leading_ones {
        bytes.push(0);
    }
    bytes.reverse();

    if bytes.len() != 32 {
        return None;
    }

    let mut result = [0u8; 32];
    result.copy_from_slice(&bytes);
    Some(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::keypair::IdentityKeypair;

    #[test]
    fn test_did_generation() {
        let kp = IdentityKeypair::generate();
        let did = Did::from_public_identity(&kp.public_identity());
        assert!(did.0.starts_with("did:nexus:"));
    }

    #[test]
    fn test_did_roundtrip() {
        let kp = IdentityKeypair::generate();
        let pub_id = kp.public_identity();
        let did = Did::from_public_identity(&pub_id);
        let recovered = did.to_public_key_bytes().unwrap();
        assert_eq!(recovered, pub_id.public_key);
    }

    #[test]
    fn test_did_display() {
        let kp = IdentityKeypair::generate();
        let did = Did::from_public_identity(&kp.public_identity());
        let s = format!("{}", did);
        assert!(s.starts_with("did:nexus:"));
    }
}
