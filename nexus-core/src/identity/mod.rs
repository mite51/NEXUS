//! Identity module — DID generation, keypairs, encrypted vault
//!
//! Provides Ed25519 keypair management and Argon2id-encrypted identity vaults.

pub mod keypair;
pub mod vault;
pub mod did;

pub use keypair::{IdentityKeypair, verify_signature};
pub use vault::IdentityVault;
pub use did::Did;
