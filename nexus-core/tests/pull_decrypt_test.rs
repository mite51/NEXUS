//! Unit tests for the pull-then-decrypt flow — the logic shared by CLI and Tauri.
//!
//! These test the building blocks used in `pull_shared_file` (Tauri) and `pull` (CLI)
//! without any networking. Covers:
//! - Share link parsing (valid, invalid, edge cases)
//! - Shard CID verification
//! - PRE decryption with both private grants and public grants
//! - Shard reassembly + symmetric decryption
//! - Re-encryption (add-to-my-files flow)

use nexus_core::crypto::{encrypt_data, decrypt_data, generate_dek};
use nexus_core::crypto::pre::{PreKeypair, PreSigner, reencrypt, public_pre_keypair, PUBLIC_DID};
use nexus_core::manifest::{NexusManifest, ShareGrant};
use nexus_core::storage::shard::{self, Shard, compute_cid};
use nexus_core::storage::{AssetStore, ShardStore, DEFAULT_SHARD_SIZE};
use tempfile::TempDir;

/// Helper: create a full encrypted asset and return all pieces
fn create_test_asset(plaintext: &[u8]) -> (PreKeypair, NexusManifest, Vec<Shard>, Vec<u8>) {
    let pre_kp = PreKeypair::generate();
    let dek = generate_dek();
    let encrypted_body = encrypt_data(plaintext, &dek).unwrap();
    let (shard_manifest, shards) = shard::shard_data(&encrypted_body, DEFAULT_SHARD_SIZE);
    let encrypted_dek = pre_kp.encrypt_dek(&dek).unwrap();

    let manifest = NexusManifest {
        owner: "did:nexus:TestOwner".into(),
        owner_pre_pk: pre_kp.public_key(),
        encrypted_dek,
        shards: shard_manifest,
    };

    (pre_kp, manifest, shards, dek.to_vec())
}

// ============================================================
// Share link parsing
// ============================================================

#[test]
fn test_parse_share_link_valid() {
    let link = "nexus://12D3KooWABC/asset/deadbeef1234";
    let (peer, asset) = AssetStore::parse_share_link(link).unwrap();
    assert_eq!(peer, "12D3KooWABC");
    assert_eq!(asset, "deadbeef1234");
}

#[test]
fn test_parse_share_link_long_ids() {
    let peer = "12D3KooWPKLuqYV9FKz3MxTCReWHZnbUr9HGhLGYZcPQ1Ne5rWZq";
    let asset = "eaa4580326f5c9361e1d7a8caaf93f8c3030870613ab3d612ea5709674cceafe";
    let link = format!("nexus://{}/asset/{}", peer, asset);
    let (p, a) = AssetStore::parse_share_link(&link).unwrap();
    assert_eq!(p, peer);
    assert_eq!(a, asset);
}

#[test]
fn test_parse_share_link_invalid() {
    assert!(AssetStore::parse_share_link("").is_none());
    assert!(AssetStore::parse_share_link("http://google.com").is_none());
    assert!(AssetStore::parse_share_link("nexus://peer/wrong/path").is_none());
    assert!(AssetStore::parse_share_link("nexus://peer/notasset/id").is_none());
    assert!(AssetStore::parse_share_link("nexus://peer").is_none());
    assert!(AssetStore::parse_share_link("nexus://").is_none());
}

#[test]
fn test_share_link_roundtrip() {
    let peer = "12D3KooWTestPeer";
    let asset = "abc123";
    let link = AssetStore::share_link(peer, asset);
    let (p, a) = AssetStore::parse_share_link(&link).unwrap();
    assert_eq!(p, peer);
    assert_eq!(a, asset);
}

// ============================================================
// Shard CID verification
// ============================================================

#[test]
fn test_shard_cid_verification() {
    let data = b"Some test shard data for CID computation";
    let cid = compute_cid(data);
    let cid_hex: String = cid.iter().map(|b| format!("{:02x}", b)).collect();

    // Verify same data produces same CID
    let cid2 = compute_cid(data);
    let cid2_hex: String = cid2.iter().map(|b| format!("{:02x}", b)).collect();
    assert_eq!(cid_hex, cid2_hex);

    // Different data produces different CID
    let cid3 = compute_cid(b"Different data");
    let cid3_hex: String = cid3.iter().map(|b| format!("{:02x}", b)).collect();
    assert_ne!(cid_hex, cid3_hex);
}

#[test]
fn test_shard_cid_mismatch_detection() {
    let data = b"Original shard data";
    let cid = compute_cid(data);
    let expected_hex: String = cid.iter().map(|b| format!("{:02x}", b)).collect();

    // Tampered data should NOT match
    let tampered = b"Tampered shard data";
    let tampered_cid = compute_cid(tampered);
    let tampered_hex: String = tampered_cid.iter().map(|b| format!("{:02x}", b)).collect();
    assert_ne!(expected_hex, tampered_hex);
}

// ============================================================
// Full decrypt flow: encrypt → shard → PRE share → reassemble → decrypt
// ============================================================

#[test]
fn test_private_share_decrypt_flow() {
    let plaintext = b"Private document for authorized recipient only.";
    let (owner_pre, manifest, shards, _dek) = create_test_asset(plaintext);

    // Recipient
    let recipient_pre = PreKeypair::generate();
    let recipient_did = "did:nexus:Recipient123";

    // Owner creates share grant
    let signer = PreSigner::new();
    let vk = signer.verifying_key();
    let kfrags = signer.generate_kfrags(&owner_pre, &recipient_pre.public_key(), 1, 1).unwrap();
    let cfrag = reencrypt(
        &manifest.encrypted_dek,
        &kfrags[0],
        &owner_pre.public_key(),
        &recipient_pre.public_key(),
        &vk,
    ).unwrap();

    let grant = ShareGrant {
        recipient: recipient_did.into(),
        recipient_pre_pk: recipient_pre.public_key(),
        cfrags: vec![cfrag],
        verifying_key: vk,
        manifest_ref: "test".into(),
    };

    // Simulate pull response: verify CIDs, decrypt, reassemble
    let shard_data: Vec<Vec<u8>> = shards.iter().map(|s| s.data.clone()).collect();
    for (expected_cid, data) in manifest.shards.shards.iter().zip(&shard_data) {
        let computed = compute_cid(data);
        let computed_hex: String = computed.iter().map(|b| format!("{:02x}", b)).collect();
        assert_eq!(&computed_hex, expected_cid);
    }

    // PRE decrypt
    let dek = recipient_pre.decrypt_dek_reencrypted(
        &manifest.encrypted_dek,
        &grant.cfrags,
        &manifest.owner_pre_pk,
        &grant.verifying_key,
    ).unwrap();

    // Reassemble
    let shard_objs: Vec<Shard> = manifest.shards.shards.iter()
        .zip(shard_data)
        .map(|(cid_hex, data)| {
            let cid_bytes: Vec<u8> = (0..cid_hex.len())
                .step_by(2)
                .map(|i| u8::from_str_radix(&cid_hex[i..i+2], 16).unwrap())
                .collect();
            Shard { cid: cid_bytes, data }
        })
        .collect();

    let encrypted_body = shard::reassemble(&manifest.shards, &shard_objs).unwrap();
    let decrypted = decrypt_data(&encrypted_body, &dek).unwrap();
    assert_eq!(decrypted, plaintext);
}

#[test]
fn test_public_share_decrypt_flow() {
    let plaintext = b"Public document anyone can read.";
    let (owner_pre, manifest, shards, _dek) = create_test_asset(plaintext);

    // Create public grant
    let public_kp = public_pre_keypair();
    let signer = PreSigner::new();
    let vk = signer.verifying_key();
    let kfrags = signer.generate_kfrags(&owner_pre, &public_kp.public_key(), 1, 1).unwrap();
    let cfrag = reencrypt(
        &manifest.encrypted_dek,
        &kfrags[0],
        &owner_pre.public_key(),
        &public_kp.public_key(),
        &vk,
    ).unwrap();

    let grant = ShareGrant {
        recipient: PUBLIC_DID.into(),
        recipient_pre_pk: public_kp.public_key(),
        cfrags: vec![cfrag],
        verifying_key: vk,
        manifest_ref: "".into(),
    };

    // Simulate what the client does: detect public grant and use public keypair
    let decrypt_kp = if grant.recipient == PUBLIC_DID {
        public_pre_keypair()
    } else {
        panic!("Should be public");
    };

    let shard_data: Vec<Vec<u8>> = shards.iter().map(|s| s.data.clone()).collect();
    let dek = decrypt_kp.decrypt_dek_reencrypted(
        &manifest.encrypted_dek,
        &grant.cfrags,
        &manifest.owner_pre_pk,
        &grant.verifying_key,
    ).unwrap();

    let shard_objs: Vec<Shard> = manifest.shards.shards.iter()
        .zip(shard_data)
        .map(|(cid_hex, data)| {
            let cid_bytes: Vec<u8> = (0..cid_hex.len())
                .step_by(2)
                .map(|i| u8::from_str_radix(&cid_hex[i..i+2], 16).unwrap())
                .collect();
            Shard { cid: cid_bytes, data }
        })
        .collect();

    let encrypted_body = shard::reassemble(&manifest.shards, &shard_objs).unwrap();
    let decrypted = decrypt_data(&encrypted_body, &dek).unwrap();
    assert_eq!(decrypted, plaintext);
}

// ============================================================
// Re-encryption flow (add-to-my-files)
// ============================================================

#[test]
fn test_reencrypt_to_own_key() {
    let plaintext = b"Downloaded file that user wants to keep in their vault.";
    let (owner_pre, manifest, shards, _) = create_test_asset(plaintext);

    // Create public grant for download
    let public_kp = public_pre_keypair();
    let signer = PreSigner::new();
    let vk = signer.verifying_key();
    let kfrags = signer.generate_kfrags(&owner_pre, &public_kp.public_key(), 1, 1).unwrap();
    let cfrag = reencrypt(
        &manifest.encrypted_dek,
        &kfrags[0],
        &owner_pre.public_key(),
        &public_kp.public_key(),
        &vk,
    ).unwrap();

    let grant = ShareGrant {
        recipient: PUBLIC_DID.into(),
        recipient_pre_pk: public_kp.public_key(),
        cfrags: vec![cfrag],
        verifying_key: vk,
        manifest_ref: "".into(),
    };

    // Step 1: Decrypt the downloaded asset
    let shard_data: Vec<Vec<u8>> = shards.iter().map(|s| s.data.clone()).collect();
    let dek = public_kp.decrypt_dek_reencrypted(
        &manifest.encrypted_dek,
        &grant.cfrags,
        &manifest.owner_pre_pk,
        &grant.verifying_key,
    ).unwrap();

    let shard_objs: Vec<Shard> = manifest.shards.shards.iter()
        .zip(shard_data)
        .map(|(cid_hex, data)| {
            let cid_bytes: Vec<u8> = (0..cid_hex.len())
                .step_by(2)
                .map(|i| u8::from_str_radix(&cid_hex[i..i+2], 16).unwrap())
                .collect();
            Shard { cid: cid_bytes, data }
        })
        .collect();

    let encrypted_body = shard::reassemble(&manifest.shards, &shard_objs).unwrap();
    let decrypted_plaintext = decrypt_data(&encrypted_body, &dek).unwrap();
    assert_eq!(decrypted_plaintext, plaintext);

    // Step 2: Re-encrypt with user's own key (add-to-my-files)
    let my_pre = PreKeypair::generate();
    let new_dek = generate_dek();
    let new_encrypted = encrypt_data(&decrypted_plaintext, &new_dek).unwrap();
    let new_encrypted_dek = my_pre.encrypt_dek(&new_dek).unwrap();

    let (new_shard_manifest, new_shards) = shard::shard_data(&new_encrypted, DEFAULT_SHARD_SIZE);

    let new_manifest = NexusManifest {
        owner: "did:nexus:MyOwnDID".into(),
        owner_pre_pk: my_pre.public_key(),
        encrypted_dek: new_encrypted_dek.clone(),
        shards: new_shard_manifest.clone(),
    };

    // Step 3: Verify I can now decrypt with my own key (owner decryption)
    let owner_dek = my_pre.decrypt_dek(&new_encrypted_dek).unwrap();
    let new_shard_objs: Vec<Shard> = new_shard_manifest.shards.iter()
        .zip(&new_shards)
        .map(|(_cid_hex, s)| s.clone())
        .collect();
    let reassembled = shard::reassemble(&new_manifest.shards, &new_shard_objs).unwrap();
    let final_plaintext = decrypt_data(&reassembled, &owner_dek).unwrap();
    assert_eq!(final_plaintext, plaintext);
}

// ============================================================
// Asset store + shard store integration
// ============================================================

#[test]
fn test_asset_store_manifest_and_shards() {
    let dir = TempDir::new().unwrap();
    let asset_store = AssetStore::open(dir.path()).unwrap();
    let shard_store = ShardStore::open(dir.path()).unwrap();

    let plaintext = b"Test file for asset store integration";
    let (_, manifest, shards, _) = create_test_asset(plaintext);

    // Store shards
    for s in &shards {
        shard_store.put(s).unwrap();
    }

    // Store manifest
    let manifest_bytes = serde_json::to_vec(&manifest).unwrap();
    let asset_id = asset_store.put_manifest(&manifest_bytes).unwrap();

    // Verify retrieval
    let retrieved = asset_store.get_manifest(&asset_id).unwrap().unwrap();
    let parsed: NexusManifest = serde_json::from_slice(&retrieved).unwrap();
    assert_eq!(parsed.owner, manifest.owner);
    assert_eq!(parsed.shards.shards.len(), manifest.shards.shards.len());

    // Verify all shards exist in store
    for cid in &parsed.shards.shards {
        let shard = shard_store.get(cid).unwrap();
        assert!(shard.is_some(), "Shard {} should exist in store", cid);
    }
}

#[test]
fn test_public_flag_persistence() {
    let dir = TempDir::new().unwrap();
    let store = AssetStore::open(dir.path()).unwrap();

    let manifest_bytes = b"test manifest";
    let asset_id = store.put_manifest(manifest_bytes).unwrap();

    // Not public by default
    assert!(!store.is_public(&asset_id).unwrap());

    // Mark public
    store.set_public(&asset_id, true).unwrap();
    assert!(store.is_public(&asset_id).unwrap());

    // Un-mark
    store.set_public(&asset_id, false).unwrap();
    assert!(!store.is_public(&asset_id).unwrap());
}

// ============================================================
// Edge cases
// ============================================================

#[test]
fn test_encrypt_decrypt_empty_data() {
    let plaintext = b"";
    let dek = generate_dek();
    let encrypted = encrypt_data(plaintext, &dek).unwrap();
    let decrypted = decrypt_data(&encrypted, &dek).unwrap();
    assert_eq!(decrypted, plaintext);
}

#[test]
fn test_encrypt_decrypt_1_byte() {
    let plaintext = b"X";
    let dek = generate_dek();
    let encrypted = encrypt_data(plaintext, &dek).unwrap();
    let decrypted = decrypt_data(&encrypted, &dek).unwrap();
    assert_eq!(decrypted, plaintext);
}

#[test]
fn test_encrypt_decrypt_exact_shard_boundary() {
    // Data exactly equal to shard size
    let plaintext: Vec<u8> = vec![0xAB; DEFAULT_SHARD_SIZE];
    let dek = generate_dek();
    let encrypted = encrypt_data(&plaintext, &dek).unwrap();
    let (shard_manifest, shards) = shard::shard_data(&encrypted, DEFAULT_SHARD_SIZE);

    let reassembled = shard::reassemble(&shard_manifest, &shards).unwrap();
    let decrypted = decrypt_data(&reassembled, &dek).unwrap();
    assert_eq!(decrypted, plaintext);
}

#[test]
fn test_shard_reassemble_wrong_order() {
    let data = vec![0xFF; 500_000]; // Multiple shards
    let (manifest, shards) = shard::shard_data(&data, DEFAULT_SHARD_SIZE);
    assert!(shards.len() >= 2, "Need multiple shards for this test");

    // Correct order works
    let reassembled = shard::reassemble(&manifest, &shards).unwrap();
    assert_eq!(reassembled, data);

    // The reassemble function uses CID matching, so order shouldn't matter
    // (it reassembles by matching shard CIDs to manifest order)
    let mut reversed = shards.clone();
    reversed.reverse();
    let reassembled2 = shard::reassemble(&manifest, &reversed).unwrap();
    assert_eq!(reassembled2, data);
}

#[test]
fn test_manifest_serialization_roundtrip() {
    let (_, manifest, _, _) = create_test_asset(b"test data");
    let json = serde_json::to_vec(&manifest).unwrap();
    let parsed: NexusManifest = serde_json::from_slice(&json).unwrap();
    assert_eq!(parsed.owner, manifest.owner);
    assert_eq!(parsed.shards.shards, manifest.shards.shards);
}

#[test]
fn test_share_grant_serialization_roundtrip() {
    let owner_pre = PreKeypair::generate();
    let recipient_pre = PreKeypair::generate();
    let dek = generate_dek();
    let encrypted_dek = owner_pre.encrypt_dek(&dek).unwrap();

    let signer = PreSigner::new();
    let vk = signer.verifying_key();
    let kfrags = signer.generate_kfrags(&owner_pre, &recipient_pre.public_key(), 1, 1).unwrap();
    let cfrag = reencrypt(
        &encrypted_dek,
        &kfrags[0],
        &owner_pre.public_key(),
        &recipient_pre.public_key(),
        &vk,
    ).unwrap();

    let grant = ShareGrant {
        recipient: "did:nexus:Test".into(),
        recipient_pre_pk: recipient_pre.public_key(),
        cfrags: vec![cfrag],
        verifying_key: vk,
        manifest_ref: "manifest.nexus".into(),
    };

    let json = serde_json::to_vec(&grant).unwrap();
    let parsed: ShareGrant = serde_json::from_slice(&json).unwrap();
    assert_eq!(parsed.recipient, grant.recipient);
    assert_eq!(parsed.cfrags.len(), 1);
}
