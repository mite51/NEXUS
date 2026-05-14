use clap::{Parser, Subcommand};

mod commands;

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
    /// Start a relay server (helps NATted peers connect)
    ///
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
        #[arg(long, default_value = "vault.json")]
        vault: String,
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
    /// Mark an asset as public (generates public PRE rfrag)
    MakePublic {
        /// Asset ID (hex hash)
        asset_id: String,
        #[arg(long, default_value = "vault.json")]
        vault: String,
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
        Commands::Pull { link, output, addr, vault } => {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(commands::pull(&link, output.as_deref(), addr.as_deref(), &vault))
        }
        Commands::Relay { vault, port, max_circuits, max_reservations_per_peer } => {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(commands::run_relay(&vault, port, max_circuits, max_reservations_per_peer))
        }
        Commands::Store { action } => match action {
            StoreAction::Stats { dir } => commands::store_stats(&dir),
            StoreAction::List { dir } => commands::store_list(&dir),
            StoreAction::Import { manifest, from, dir } => commands::store_import(&manifest, &from, &dir),
            StoreAction::Verify { dir } => commands::store_verify(&dir),
        },
        Commands::MakePublic { asset_id, vault } => {
            commands::make_public(&asset_id, &vault)
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
