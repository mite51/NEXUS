use nexus_core::identity::{IdentityKeypair, Vault};
use nexus_core::manifest::{NexusManifest, ShareGrant};
use nexus_core::crypto::pre::{self, PreSigner, public_pre_keypair, PUBLIC_DID};
use nexus_core::storage::AssetStore;

fn main() {
    let vault_path = "neo-vault.json";
    let passphrase = "neo-test-2026";
    let asset_id = "f831013bcb3d4258d2fa246be3cf544d96098458d9f867c9eb68b8ff303feabd";

    // Load keys
    let vault_json = std::fs::read_to_string(vault_path).unwrap();
    let vault: Vault = serde_json::from_str(&vault_json).unwrap();
    let keypair = vault.decrypt_identity(passphrase).unwrap();
    let pre_kp = vault.decrypt_pre_keypair(passphrase).unwrap();

    // Load manifest
    let store = AssetStore::open(".nexus-store").unwrap();
    let manifest_bytes = store.get_manifest(asset_id).unwrap().unwrap();
    let manifest: NexusManifest = serde_json::from_slice(&manifest_bytes).unwrap();

    // Generate public rfrag
    let public_kp = public_pre_keypair();
    let signer = PreSigner::new();
    let vk = signer.verifying_key();
    let kfrags = signer.generate_kfrags(&pre_kp, &public_kp.public_key(), 1, 1).unwrap();

    let cfrag = pre::reencrypt(
        &manifest.encrypted_dek,
        &kfrags[0],
        &pre_kp.public_key(),
        &public_kp.public_key(),
        &vk,
    ).unwrap();

    let grant = ShareGrant {
        recipient: PUBLIC_DID.to_string(),
        recipient_pre_pk: public_kp.public_key(),
        cfrags: vec![cfrag],
        verifying_key: vk,
        manifest_ref: String::new(),
    };

    let rfrag_bytes = serde_json::to_vec(&grant).unwrap();
    store.put_rfrag(asset_id, PUBLIC_DID, &rfrag_bytes).unwrap();

    // Mark as public
    store.set_public(asset_id, true).unwrap();

    println!("✓ Asset {} marked public with rfrag for {}", asset_id, PUBLIC_DID);
}
