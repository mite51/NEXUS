use clap::{Parser, Subcommand};

mod commands;
mod access_commands;

/// NEXUS — Decentralized encrypted file ownership
#[derive(Parser)]
#[command(name = "nexus", version, about)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Initialize a new identity (generate keypair + encrypted vault)
    Init {
        /// Path to store the vault file
        #[arg(long, default_value = "vault.json")]
        vault: String,
    },
    /// Show your identity (DID and public key)
    Identity {
        #[arg(long, default_value = "vault.json")]
        vault: String,
    },
    /// Export your public key (share with others so they can grant you access)
    ExportKey {
        #[arg(long, default_value = "vault.json")]
        vault: String,
    },
    /// Encrypt a file (produces encrypted shards + manifest)
    Encrypt {
        /// File to encrypt
        file: String,
        /// Output directory for shards and manifest
        #[arg(long, short, default_value = ".")]
        output: String,
        #[arg(long, default_value = "vault.json")]
        vault: String,
    },
    /// Decrypt a file from a manifest (owner decryption)
    Decrypt {
        /// Path to the .nexus manifest file
        manifest: String,
        /// Output filename (defaults to original name)
        #[arg(long, short)]
        output: Option<String>,
        #[arg(long, default_value = "vault.json")]
        vault: String,
    },
    /// Decrypt a shared file using a .share grant
    DecryptShared {
        /// Path to the .nexus manifest file
        manifest: String,
        /// Path to the .share grant file
        #[arg(long)]
        share: String,
        /// Output filename
        #[arg(long, short)]
        output: Option<String>,
        #[arg(long, default_value = "vault.json")]
        vault: String,
    },
    /// Share access to an encrypted file with another DID
    Share {
        /// Path to the .nexus manifest file
        manifest: String,
        /// Path to recipient's exported public key file
        #[arg(long)]
        to: String,
        #[arg(long, default_value = "vault.json")]
        vault: String,
    },
    /// Start a NEXUS network node (peer-to-peer daemon)
    Node {
        #[arg(long, default_value = "vault.json")]
        vault: String,
        /// Listen address (can be specified multiple times)
        #[arg(long, default_values_t = vec![
            "/ip4/0.0.0.0/udp/4001/quic-v1".to_string(),
            "/ip4/0.0.0.0/tcp/4001".to_string(),
        ])]
        listen: Vec<String>,
        /// Bootstrap peer (multiaddr, can be specified multiple times)
        #[arg(long)]
        bootstrap: Vec<String>,
        /// Relay server (multiaddr, can be specified multiple times)
        #[arg(long)]
        relay: Vec<String>,
    },
    /// Ping a peer node to check connectivity
    Ping {
        /// Multiaddr of the peer to ping
        addr: String,
        #[arg(long, default_value = "vault.json")]
        vault: String,
    },
    /// Request a single shard from a peer (connectivity test)
    GetShard {
        /// CID of the shard to request
        cid: String,
        /// Multiaddr of the peer holding the shard
        #[arg(long)]
        from: String,
        #[arg(long, default_value = "vault.json")]
        vault: String,
    },
    /// Fetch shards from a peer and decrypt (full receive flow)
    Fetch {
        /// Path to the .nexus manifest file
        manifest: String,
        /// Multiaddr of the peer holding the shards
        #[arg(long)]
        from: String,
        /// Path to the .share grant file (required unless you're the owner)
        #[arg(long)]
        share: Option<String>,
        /// Output filename (defaults to original name from manifest)
        #[arg(long, short)]
        output: Option<String>,
        #[arg(long, default_value = "vault.json")]
        vault: String,
    },
    /// Send an encrypted file to a peer (push shards + manifest)
    Push {
        /// File to push
        file: String,
        /// Target peer ID
        #[arg(long)]
        peer: String,
        /// Target folder on receiver's vault
        #[arg(long, default_value = "/")]
        folder: String,
        /// Direct multiaddr to dial the peer (overrides relay)
        #[arg(long)]
        addr: Option<String>,
        /// Relay multiaddr (e.g. /ip4/1.2.3.4/tcp/4002/p2p/<relay-peer-id>)
        #[arg(long)]
        relay: Option<String>,
        #[arg(long, default_value = "vault.json")]
        vault: String,
    },
    /// Pull an encrypted file using a nexus:// share link
    Pull {
        /// nexus:// share link (e.g. nexus://<peer-id>/asset/<asset-id>)
        link: String,
        /// Output filename (defaults to original name from manifest)
        #[arg(long, short)]
        output: Option<String>,
        /// Multiaddr to dial the peer (if not discoverable via mDNS)
        #[arg(long)]
        addr: Option<String>,
        #[arg(long, default_value = "vault.json")]
        vault: String,
    },
    Relay {
        /// Listen port (TCP and QUIC)
        #[arg(long, default_value_t = 4001)]
        port: u16,
        /// Max concurrent relay circuits
        #[arg(long, default_value_t = 128)]
        max_circuits: u32,
        /// Max reservations per peer
        #[arg(long, default_value_t = 4)]
        max_reservations_per_peer: u32,
    },
    /// Manage the local shard store
    Store {
        #[command(subcommand)]
        action: StoreAction,
    },
    /// Manage access-control contacts
    Contact {
        #[command(subcommand)]
        action: ContactAction,
    },
    /// Manage access-control groups
    Group {
        #[command(subcommand)]
        action: GroupAction,
    },
    /// Manage vault folders and grants
    Folder {
        #[command(subcommand)]
        action: FolderAction,
    },
    /// Mark an asset as public (generates public PRE rfrag)
    MakePublic {
        /// Asset ID (hex hash)
        asset_id: String,
        #[arg(long, default_value = "vault.json")]
        vault: String,
    },
    /// Sync re-encryption fragments for all contacts with access grants
    ///
    /// Generates missing rfrags (so authorized contacts can decrypt) and removes
    /// stale rfrags (where access was revoked). Requires vault passphrase.
    SyncRfrags {
        #[arg(long, default_value = "vault.json")]
        vault: String,
        /// Store directory
        #[arg(long, default_value = ".nexus-store")]
        dir: String,
        /// Only generate rfrags (don't remove stale ones)
        #[arg(long, default_value_t = false)]
        no_revoke: bool,
        /// Only process a specific asset ID
        #[arg(long)]
        asset: Option<String>,
        /// Dry run — show what would be done without making changes
        #[arg(long, default_value_t = false)]
        dry_run: bool,
    },
    /// Create a join request to exchange contacts
    CreateJoinRequest {
        #[arg(long, default_value = "vault.json")]
        vault: String,
        /// Your display name
        #[arg(long)]
        name: String,
        /// Include your PRE public key in the request
        #[arg(long, default_value_t = true)]
        include_pre: bool,
    },
    /// Accept an incoming join request and output a response
    AcceptJoinRequest {
        #[arg(long, default_value = "vault.json")]
        vault: String,
        /// Your display name
        #[arg(long)]
        name: String,
        /// The join request JSON string
        #[arg(long)]
        request: String,
    },
    /// Apply a join response to complete the handshake
    ApplyJoinResponse {
        /// The join response JSON string
        #[arg(long)]
        response: String,
    },
}

#[derive(Subcommand)]
enum StoreAction {
    /// Show store statistics
    Stats {
        /// Store directory
        #[arg(long, default_value = ".nexus-store")]
        dir: String,
    },
    /// List all shards in the store
    List {
        #[arg(long, default_value = ".nexus-store")]
        dir: String,
    },
    /// Import shards from an encrypted file's manifest
    Import {
        /// Path to the .nexus manifest file
        manifest: String,
        /// Shard source directory (where .shard files live)
        #[arg(long, default_value = ".")]
        from: String,
        /// Store directory
        #[arg(long, default_value = ".nexus-store")]
        dir: String,
    },
    /// Verify integrity of stored shards
    Verify {
        #[arg(long, default_value = ".nexus-store")]
        dir: String,
    },
}

#[derive(Subcommand)]
enum ContactAction {
    /// Add a new access-control contact
    Add {
        /// Contact's DID
        did: String,
        /// Display label
        #[arg(long)]
        label: String,
        /// Access level (none, read, rw, full, or 0-7)
        #[arg(long, default_value = "read")]
        access: String,
        /// Store directory
        #[arg(long, default_value = ".nexus-store")]
        dir: String,
        /// Peer ID (optional)
        #[arg(long)]
        peer_id: Option<String>,
    },
    /// Remove a contact
    Remove {
        /// Contact's DID
        did: String,
        #[arg(long, default_value = ".nexus-store")]
        dir: String,
    },
    /// List all contacts
    List {
        #[arg(long, default_value = ".nexus-store")]
        dir: String,
    },
    /// Show details of a contact
    Show {
        /// Contact's DID
        did: String,
        #[arg(long, default_value = ".nexus-store")]
        dir: String,
    },
    /// Set a contact's access level
    SetAccess {
        /// Contact's DID
        did: String,
        /// New access level
        access: String,
        #[arg(long, default_value = ".nexus-store")]
        dir: String,
    },
}

#[derive(Subcommand)]
enum GroupAction {
    /// Create a new group
    Create {
        /// Group name
        name: String,
        /// Description
        #[arg(long)]
        description: Option<String>,
        #[arg(long, default_value = ".nexus-store")]
        dir: String,
    },
    /// Delete a group
    Delete {
        /// Group name
        name: String,
        #[arg(long, default_value = ".nexus-store")]
        dir: String,
    },
    /// Add a member (DID) to a group
    AddMember {
        /// Group name
        group: String,
        /// Member DID
        did: String,
        #[arg(long, default_value = ".nexus-store")]
        dir: String,
    },
    /// Remove a member from a group
    RemoveMember {
        /// Group name
        group: String,
        /// Member DID
        did: String,
        #[arg(long, default_value = ".nexus-store")]
        dir: String,
    },
    /// List all groups
    List {
        #[arg(long, default_value = ".nexus-store")]
        dir: String,
    },
    /// Show a group's members
    Show {
        /// Group name
        name: String,
        #[arg(long, default_value = ".nexus-store")]
        dir: String,
    },
}

#[derive(Subcommand)]
enum FolderAction {
    /// Create a vault folder
    Create {
        /// Folder path (must start with /)
        path: String,
        /// Display label
        #[arg(long)]
        label: Option<String>,
        /// Default access for unlisted contacts (none, read, rw, full)
        #[arg(long, default_value = "none")]
        default_access: String,
        /// Inherit permissions from parent folder
        #[arg(long, default_value_t = true)]
        inherit: bool,
        #[arg(long, default_value = ".nexus-store")]
        dir: String,
    },
    /// Remove a vault folder
    Remove {
        /// Folder path
        path: String,
        #[arg(long, default_value = ".nexus-store")]
        dir: String,
    },
    /// List all folders
    List {
        #[arg(long, default_value = ".nexus-store")]
        dir: String,
    },
    /// Show a folder's details and grants
    Show {
        /// Folder path
        path: String,
        #[arg(long, default_value = ".nexus-store")]
        dir: String,
    },
    /// Grant access on a folder (or asset within it)
    Grant {
        /// Folder path
        path: String,
        /// Grantee DID (or group name with --group)
        grantee: String,
        /// Access level (none, read, rw, full)
        #[arg(long, default_value = "read")]
        access: String,
        /// Grant to a group instead of a contact
        #[arg(long, default_value_t = false)]
        group: bool,
        /// Asset ID (grants at asset level within the folder)
        #[arg(long)]
        asset: Option<String>,
        #[arg(long, default_value = ".nexus-store")]
        dir: String,
    },
    /// Revoke a grant on a folder
    Revoke {
        /// Folder path
        path: String,
        /// Grantee DID (or group name with --group)
        grantee: String,
        /// Grantee is a group
        #[arg(long, default_value_t = false)]
        group: bool,
        #[arg(long, default_value = ".nexus-store")]
        dir: String,
    },
    /// Check effective permission for a DID
    Check {
        /// DID to check
        did: String,
        /// Folder path
        path: String,
        /// Asset ID (optional, check at asset level)
        #[arg(long)]
        asset: Option<String>,
        #[arg(long, default_value = ".nexus-store")]
        dir: String,
    },
}

fn main() {
    let cli = Cli::parse();

    let result = match cli.command {
        Commands::Init { vault } => commands::init(&vault),
        Commands::Identity { vault } => commands::identity(&vault),
        Commands::ExportKey { vault } => commands::export_key(&vault),
        Commands::Encrypt { file, output, vault } => commands::encrypt(&file, &output, &vault),
        Commands::Decrypt { manifest, output, vault } => {
            commands::decrypt(&manifest, output.as_deref(), &vault)
        }
        Commands::DecryptShared { manifest, share, output, vault } => {
            commands::decrypt_shared(&manifest, &share, output.as_deref(), &vault)
        }
        Commands::Share { manifest, to, vault } => commands::share(&manifest, &to, &vault),
        Commands::Node { vault, listen, bootstrap, relay } => {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(commands::run_node(&vault, &listen, &bootstrap, &relay))
        }
        Commands::Ping { addr, vault } => {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(commands::ping_peer(&vault, &addr))
        }
        Commands::GetShard { cid, from, vault } => {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(commands::get_shard(&vault, &cid, &from))
        }
        Commands::Fetch { manifest, from, share, output, vault } => {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(commands::fetch(&manifest, &from, share.as_deref(), output.as_deref(), &vault))
        }
        Commands::Push { file, peer, folder, addr, relay, vault } => {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(commands::push_file(&file, &peer, &folder, addr.as_deref(), relay.as_deref(), &vault))
        }
        Commands::Pull { link, output, addr, vault } => {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(commands::pull(&link, output.as_deref(), addr.as_deref(), &vault))
        }
        Commands::Relay { port, max_circuits, max_reservations_per_peer } => {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(commands::run_relay("", port, max_circuits, max_reservations_per_peer))
        }
        Commands::Store { action } => match action {
            StoreAction::Stats { dir } => commands::store_stats(&dir),
            StoreAction::List { dir } => commands::store_list(&dir),
            StoreAction::Import { manifest, from, dir } => commands::store_import(&manifest, &from, &dir),
            StoreAction::Verify { dir } => commands::store_verify(&dir),
        },
        Commands::Contact { action } => match action {
            ContactAction::Add { did, label, access, dir, peer_id } => {
                access_commands::contact_add(&dir, &did, &label, &access, peer_id.as_deref())
            }
            ContactAction::Remove { did, dir } => access_commands::contact_remove(&dir, &did),
            ContactAction::List { dir } => access_commands::contact_list(&dir),
            ContactAction::Show { did, dir } => access_commands::contact_show(&dir, &did),
            ContactAction::SetAccess { did, access, dir } => {
                access_commands::contact_set_access(&dir, &did, &access)
            }
        },
        Commands::Group { action } => match action {
            GroupAction::Create { name, description, dir } => {
                access_commands::group_create(&dir, &name, description.as_deref())
            }
            GroupAction::Delete { name, dir } => access_commands::group_delete(&dir, &name),
            GroupAction::AddMember { group, did, dir } => {
                access_commands::group_add_member(&dir, &group, &did)
            }
            GroupAction::RemoveMember { group, did, dir } => {
                access_commands::group_remove_member(&dir, &group, &did)
            }
            GroupAction::List { dir } => access_commands::group_list(&dir),
            GroupAction::Show { name, dir } => access_commands::group_show(&dir, &name),
        },
        Commands::Folder { action } => match action {
            FolderAction::Create { path, label, default_access, inherit, dir } => {
                access_commands::folder_create(&dir, &path, label.as_deref(), &default_access, inherit)
            }
            FolderAction::Remove { path, dir } => access_commands::folder_remove(&dir, &path),
            FolderAction::List { dir } => access_commands::folder_list(&dir),
            FolderAction::Show { path, dir } => access_commands::folder_show(&dir, &path),
            FolderAction::Grant { path, grantee, access, group, asset, dir } => {
                access_commands::folder_grant(&dir, &path, &grantee, &access, asset.as_deref(), group)
            }
            FolderAction::Revoke { path, grantee, group, dir } => {
                access_commands::folder_revoke(&dir, &path, &grantee, group)
            }
            FolderAction::Check { did, path, asset, dir } => {
                access_commands::folder_check(&dir, &did, &path, asset.as_deref())
            }
        },
        Commands::MakePublic { asset_id, vault } => {
            commands::make_public(&asset_id, &vault)
        }
        Commands::SyncRfrags { vault, dir, no_revoke, asset, dry_run } => {
            commands::sync_rfrags(&vault, &dir, !no_revoke, asset.as_deref(), dry_run)
        }
        Commands::CreateJoinRequest { vault, name, include_pre } => {
            commands::create_join_request(&vault, &name, include_pre)
        }
        Commands::AcceptJoinRequest { vault, name, request } => {
            commands::accept_join_request(&vault, &name, &request)
        }
        Commands::ApplyJoinResponse { response } => {
            commands::apply_join_response(&response)
        }
    };

    if let Err(e) = result {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
}
