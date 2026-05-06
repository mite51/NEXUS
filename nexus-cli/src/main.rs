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
    Send {
        /// Path to the .nexus manifest file
        manifest: String,
        /// Multiaddr of the recipient peer
        #[arg(long)]
        to: String,
        /// Path to the .share grant file (if sharing with the recipient)
        #[arg(long)]
        share: Option<String>,
        #[arg(long, default_value = "vault.json")]
        vault: String,
    },
    /// Start a relay server (helps NATted peers connect)
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
        Commands::Fetch { manifest, from, share, output, vault } => {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(commands::fetch(&manifest, &from, share.as_deref(), output.as_deref(), &vault))
        }
        Commands::Send { manifest, to, share, vault } => {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(commands::send(&manifest, &to, share.as_deref(), &vault))
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
    };

    if let Err(e) = result {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
}
