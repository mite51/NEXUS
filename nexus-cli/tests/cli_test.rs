//! CLI integration tests — exercises the `nexus` binary as a subprocess.
//!
//! Tests the full encrypt/decrypt/share/make-public flows without network.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::TempDir;

fn nexus_bin() -> PathBuf {
    // Try release first, then debug
    let release = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent().unwrap()
        .join("target/release/nexus");
    if release.exists() {
        return release;
    }
    let debug = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent().unwrap()
        .join("target/debug/nexus");
    if debug.exists() {
        return debug;
    }
    panic!("nexus binary not found. Run `cargo build -p nexus-cli` first.");
}

fn run_nexus(dir: &Path, args: &[&str], stdin_input: &str) -> (String, String, bool) {
    let output = Command::new(nexus_bin())
        .args(args)
        .current_dir(dir)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .and_then(|mut child| {
            use std::io::Write;
            if !stdin_input.is_empty() {
                if let Some(ref mut stdin) = child.stdin {
                    stdin.write_all(stdin_input.as_bytes()).ok();
                }
            }
            drop(child.stdin.take());
            child.wait_with_output()
        })
        .expect("Failed to run nexus binary");

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    (stdout, stderr, output.status.success())
}

// ============================================================
// Init
// ============================================================

#[test]
fn test_init_creates_vault() {
    let dir = TempDir::new().unwrap();
    let (stdout, _stderr, success) = run_nexus(
        dir.path(),
        &["init", "--vault", "test-vault.json"],
        "hunter2\nhunter2\n",
    );
    assert!(success, "init should succeed");
    assert!(stdout.contains("Identity created"), "should show success: {}", stdout);
    assert!(stdout.contains("DID: did:nexus:"), "should show DID: {}", stdout);
    assert!(dir.path().join("test-vault.json").exists(), "vault file should exist");
}

#[test]
fn test_init_passphrase_mismatch() {
    let dir = TempDir::new().unwrap();
    let (_stdout, _stderr, success) = run_nexus(
        dir.path(),
        &["init", "--vault", "vault.json"],
        "abc\nxyz\n",
    );
    assert!(!success, "should fail with mismatched passphrases");
}

// ============================================================
// Encrypt + Decrypt round-trip
// ============================================================

#[test]
fn test_encrypt_decrypt_roundtrip() {
    let dir = TempDir::new().unwrap();

    // Init vault
    let (_, _, ok) = run_nexus(dir.path(), &["init", "--vault", "vault.json"], "pass\npass\n");
    assert!(ok, "init failed");

    // Create a test file
    let test_file = dir.path().join("hello.txt");
    fs::write(&test_file, "Hello, NEXUS encryption!").unwrap();

    // Encrypt
    let (stdout, _stderr, ok) = run_nexus(
        dir.path(),
        &["encrypt", "--vault", "vault.json", "hello.txt"],
        "pass\n",
    );
    assert!(ok, "encrypt failed: {}", stdout);
    assert!(stdout.contains("Encrypted: hello.txt"), "stdout: {}", stdout);
    assert!(dir.path().join("hello.txt.nexus").exists(), "manifest should exist");
    assert!(dir.path().join("shards").exists(), "shards dir should exist");

    // Decrypt
    let (stdout, _stderr, ok) = run_nexus(
        dir.path(),
        &["decrypt", "--vault", "vault.json", "-o", "decrypted.txt", "hello.txt.nexus"],
        "pass\n",
    );
    assert!(ok, "decrypt failed: {}", stdout);

    let decrypted = fs::read_to_string(dir.path().join("decrypted.txt")).unwrap();
    assert_eq!(decrypted, "Hello, NEXUS encryption!");
}

#[test]
fn test_encrypt_large_file_multiple_shards() {
    let dir = TempDir::new().unwrap();

    // Init
    run_nexus(dir.path(), &["init", "--vault", "vault.json"], "pass\npass\n");

    // Create a file larger than one shard (262144 bytes = 256KB)
    let test_file = dir.path().join("large.bin");
    let data: Vec<u8> = (0..300_000).map(|i| (i % 256) as u8).collect();
    fs::write(&test_file, &data).unwrap();

    // Encrypt
    let (stdout, _, ok) = run_nexus(
        dir.path(),
        &["encrypt", "--vault", "vault.json", "large.bin"],
        "pass\n",
    );
    assert!(ok, "encrypt failed");
    assert!(stdout.contains("Shards: 2"), "Should produce 2 shards: {}", stdout);

    // Decrypt
    let (_, _, ok) = run_nexus(
        dir.path(),
        &["decrypt", "--vault", "vault.json", "-o", "large_out.bin", "large.bin.nexus"],
        "pass\n",
    );
    assert!(ok, "decrypt failed");

    let output = fs::read(dir.path().join("large_out.bin")).unwrap();
    assert_eq!(output, data);
}

#[test]
fn test_decrypt_wrong_passphrase() {
    let dir = TempDir::new().unwrap();

    // Init + encrypt
    run_nexus(dir.path(), &["init", "--vault", "vault.json"], "pass\npass\n");
    fs::write(dir.path().join("test.txt"), "secret").unwrap();
    run_nexus(dir.path(), &["encrypt", "--vault", "vault.json", "test.txt"], "pass\n");

    // Decrypt with wrong passphrase
    let (_, _, ok) = run_nexus(
        dir.path(),
        &["decrypt", "--vault", "vault.json", "-o", "out.txt", "test.txt.nexus"],
        "WRONG\n",
    );
    assert!(!ok, "should fail with wrong passphrase");
}

// ============================================================
// Identity
// ============================================================

#[test]
fn test_identity_shows_did() {
    let dir = TempDir::new().unwrap();
    run_nexus(dir.path(), &["init", "--vault", "vault.json"], "pass\npass\n");

    let (stdout, _, ok) = run_nexus(dir.path(), &["identity", "--vault", "vault.json"], "pass\n");
    assert!(ok);
    assert!(stdout.contains("DID: did:nexus:"), "stdout: {}", stdout);
    assert!(stdout.contains("public key"), "should show public key: {}", stdout);
}

// ============================================================
// Export key
// ============================================================

#[test]
fn test_export_key() {
    let dir = TempDir::new().unwrap();
    run_nexus(dir.path(), &["init", "--vault", "vault.json"], "pass\npass\n");

    let (stdout, _, ok) = run_nexus(dir.path(), &["export-key", "--vault", "vault.json"], "pass\n");
    assert!(ok);
    assert!(stdout.contains(".pubkey.json"), "should mention pub key file: {}", stdout);

    // Find the exported key file
    let pub_files: Vec<_> = fs::read_dir(dir.path()).unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().contains(".pubkey.json"))
        .collect();
    assert!(!pub_files.is_empty(), "exported key file should exist");
}

// ============================================================
// Share + decrypt-shared
// ============================================================

#[test]
fn test_share_and_decrypt_shared() {
    // Two identities: alice (owner) and bob (recipient)
    let alice_dir = TempDir::new().unwrap();
    let bob_dir = TempDir::new().unwrap();

    // Init both
    run_nexus(alice_dir.path(), &["init", "--vault", "vault.json"], "alice\nalice\n");
    run_nexus(bob_dir.path(), &["init", "--vault", "vault.json"], "bob\nbob\n");

    // Bob exports key
    run_nexus(bob_dir.path(), &["export-key", "--vault", "vault.json"], "bob\n");
    let bob_pub_files: Vec<_> = fs::read_dir(bob_dir.path()).unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().contains(".pubkey.json"))
        .collect();
    assert!(!bob_pub_files.is_empty(), "Bob's pub key should exist");
    let bob_pub_path = bob_pub_files[0].path();

    // Copy Bob's public key to Alice's dir
    let bob_pub_in_alice = alice_dir.path().join("bob.pub.json");
    fs::copy(&bob_pub_path, &bob_pub_in_alice).unwrap();

    // Alice encrypts a file
    fs::write(alice_dir.path().join("secret.txt"), "Shared secret message").unwrap();
    let (_, _, ok) = run_nexus(
        alice_dir.path(),
        &["encrypt", "--vault", "vault.json", "secret.txt"],
        "alice\n",
    );
    assert!(ok, "Alice encrypt failed");

    // Alice shares with Bob
    let (stdout, _, ok) = run_nexus(
        alice_dir.path(),
        &["share", "--vault", "vault.json", "--to", "bob.pub.json", "secret.txt.nexus"],
        "alice\n",
    );
    assert!(ok, "share failed: {}", stdout);
    assert!(stdout.contains(".share"), "should mention .share file: {}", stdout);

    // Find the .share file (written to cwd = alice_dir)
    let share_files: Vec<_> = fs::read_dir(alice_dir.path()).unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            name.contains(".share") && name.ends_with(".json")
        })
        .collect();
    assert!(!share_files.is_empty(), "share grant should exist");
    let share_path = share_files[0].path();

    // Copy manifest and share grant to Bob's dir, along with shards
    fs::copy(
        alice_dir.path().join("secret.txt.nexus"),
        bob_dir.path().join("secret.txt.nexus"),
    ).unwrap();
    fs::copy(&share_path, bob_dir.path().join("grant.share")).unwrap();

    // Copy shards directory
    let alice_shards = alice_dir.path().join("shards");
    let bob_shards = bob_dir.path().join("shards");
    fs::create_dir_all(&bob_shards).unwrap();
    for entry in fs::read_dir(&alice_shards).unwrap() {
        let entry = entry.unwrap();
        fs::copy(entry.path(), bob_shards.join(entry.file_name())).unwrap();
    }

    // Bob decrypts the shared file
    let (stdout, _, ok) = run_nexus(
        bob_dir.path(),
        &["decrypt-shared", "--vault", "vault.json", "--share", "grant.share", "-o", "received.txt", "secret.txt.nexus"],
        "bob\n",
    );
    assert!(ok, "decrypt-shared failed: {}", stdout);

    let decrypted = fs::read_to_string(bob_dir.path().join("received.txt")).unwrap();
    assert_eq!(decrypted, "Shared secret message");
}

// ============================================================
// Make public
// ============================================================

#[test]
fn test_make_public() {
    let dir = TempDir::new().unwrap();
    run_nexus(dir.path(), &["init", "--vault", "vault.json"], "pass\npass\n");

    // Create and encrypt
    fs::write(dir.path().join("public.txt"), "Public content").unwrap();
    let (stdout, _, ok) = run_nexus(
        dir.path(),
        &["encrypt", "--vault", "vault.json", "public.txt"],
        "pass\n",
    );
    assert!(ok);

    // Extract asset ID from output
    let asset_id = stdout.lines()
        .find(|l| l.contains("Asset ID:"))
        .map(|l| l.split("Asset ID:").nth(1).unwrap().trim())
        .expect("Should have asset ID");

    // Make public
    let (stdout, _, ok) = run_nexus(
        dir.path(),
        &["make-public", "--vault", "vault.json", asset_id],
        "pass\n",
    );
    assert!(ok, "make-public failed: {}", stdout);
    assert!(stdout.contains("marked public"), "stdout: {}", stdout);
}

// ============================================================
// Store commands
// ============================================================

#[test]
fn test_store_stats() {
    let dir = TempDir::new().unwrap();
    run_nexus(dir.path(), &["init", "--vault", "vault.json"], "pass\npass\n");

    // Create a shard store
    fs::write(dir.path().join("file.txt"), "test data").unwrap();
    run_nexus(dir.path(), &["encrypt", "--vault", "vault.json", "file.txt"], "pass\n");

    let (stdout, _, ok) = run_nexus(
        dir.path(),
        &["store", "stats", "--dir", dir.path().join(".nexus-store").to_str().unwrap()],
        "",
    );
    assert!(ok, "store stats failed");
    assert!(stdout.contains("shard") || stdout.contains("Shard"), "stdout: {}", stdout);
}

// ============================================================
// Edge cases
// ============================================================

#[test]
fn test_encrypt_nonexistent_file() {
    let dir = TempDir::new().unwrap();
    run_nexus(dir.path(), &["init", "--vault", "vault.json"], "pass\npass\n");

    let (_, _, ok) = run_nexus(
        dir.path(),
        &["encrypt", "--vault", "vault.json", "no_such_file.bin"],
        "pass\n",
    );
    assert!(!ok, "should fail for nonexistent file");
}

#[test]
fn test_decrypt_corrupted_manifest() {
    let dir = TempDir::new().unwrap();
    run_nexus(dir.path(), &["init", "--vault", "vault.json"], "pass\npass\n");

    // Write a garbage manifest
    fs::write(dir.path().join("bad.nexus"), "not a valid manifest").unwrap();
    let (_, _, ok) = run_nexus(
        dir.path(),
        &["decrypt", "--vault", "vault.json", "bad.nexus"],
        "pass\n",
    );
    assert!(!ok, "should fail with corrupt manifest");
}

#[test]
fn test_encrypt_empty_file() {
    let dir = TempDir::new().unwrap();
    run_nexus(dir.path(), &["init", "--vault", "vault.json"], "pass\npass\n");

    fs::write(dir.path().join("empty.txt"), "").unwrap();
    let (stdout, _, ok) = run_nexus(
        dir.path(),
        &["encrypt", "--vault", "vault.json", "empty.txt"],
        "pass\n",
    );
    assert!(ok, "encrypt empty file should succeed: {}", stdout);

    // Decrypt it back
    let (_, _, ok) = run_nexus(
        dir.path(),
        &["decrypt", "--vault", "vault.json", "-o", "empty_out.txt", "empty.txt.nexus"],
        "pass\n",
    );
    assert!(ok, "decrypt empty file should succeed");
    let content = fs::read(dir.path().join("empty_out.txt")).unwrap();
    assert!(content.is_empty(), "should be empty");
}
