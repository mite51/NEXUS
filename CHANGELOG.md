# Changelog

All notable changes to NEXUS will be documented in this file.

Format follows [Keep a Changelog](https://keepachangelog.com/).

## [Unreleased]

### Added
- Initial project setup
- PRD v1.0 and SRD v1.0 planning documents
- Project rules and development disciplines (RULES.md)
- Core identity module: Ed25519 keypairs, DID generation (did:nexus:*), Argon2id encrypted vault
- Symmetric encryption: AES-256-GCM with random DEK generation
- Content-addressed sharding with IPFS-compatible CIDs (SHA2-256 multihash)
- **Proxy Re-Encryption (PRE)**: Full Umbral scheme via rust-umbral
  - Owner encryption/decryption of DEKs
  - kfrag generation for access delegation (threshold scheme)
  - Capsule re-encryption by proxies
  - Delegated decryption by recipients
  - Serialization of all PRE types (capsules, kfrags, cfrags)
- CLI client (`nexus` binary): init, identity, encrypt, decrypt, share
- Research docs: rust-umbral, libp2p, integration options
