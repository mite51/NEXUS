mod commands;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "nexus", version, about = "NEXUS — Decentralized encrypted file ownership")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Initialize a new identity (generate keypair + encrypted vault)
    Init {
        /// Path to store the vault file (default: ./nexus-vault.json)
        #[arg(short, long, default_value = "nexus-vault.json")]
        vault: String,
    },

    /// Show your identity (DID and public key)
    Identity {
        /// Path to vault file
        #[arg(short, long, default_value = "nexus-vault.json")]
        vault: String,
    },

    /// Encrypt a file (produces encrypted shards + manifest)
    Encrypt {
        /// File to encrypt
        file: String,

        /// Output directory for shards and manifest
        #[arg(short, long, default_value = ".")]
        output: String,

        /// Path to vault file (for key wrapping)
        #[arg(short, long, default_value = "nexus-vault.json")]
        vault: String,
    },

    /// Decrypt a file from a manifest
    Decrypt {
        /// Path to the manifest file
        manifest: String,

        /// Output file path
        #[arg(short, long)]
        output: Option<String>,

        /// Path to vault file
        #[arg(short, long, default_value = "nexus-vault.json")]
        vault: String,
    },

    /// Share access to an encrypted file with another DID
    Share {
        /// Path to the manifest file
        manifest: String,

        /// Recipient's DID (did:nexus:...)
        #[arg(short, long)]
        to: String,

        /// Path to your vault file
        #[arg(short, long, default_value = "nexus-vault.json")]
        vault: String,
    },
}

fn main() {
    let cli = Cli::parse();

    let result = match cli.command {
        Commands::Init { vault } => commands::init(&vault),
        Commands::Identity { vault } => commands::identity(&vault),
        Commands::Encrypt { file, output, vault } => commands::encrypt(&file, &output, &vault),
        Commands::Decrypt { manifest, output, vault } => commands::decrypt(&manifest, output.as_deref(), &vault),
        Commands::Share { manifest, to, vault } => commands::share(&manifest, &to, &vault),
    };

    if let Err(e) = result {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
}
