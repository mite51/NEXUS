//! Crypto module — symmetric encryption, content encryption key management
//!
//! PRE (rust-umbral) will be integrated once the base layer is proven.

pub mod symmetric;

pub use symmetric::{encrypt_data, decrypt_data, generate_dek};
