use clap::{Args, Parser, Subcommand};
use tracing::info;

use post_urbit_core::diagnostics;
use post_urbit_core::node::PostUrbitNode;
use post_urbit_core::admin_state::AdminState;
use post_urbit_core::identity::IdentityManager;
use post_urbit_core::node_config::{build_node_config, default_node_settings, load_config};

#[derive(Parser)]
#[command(name = "post-urbit-node")]
#[command(about = "Post-Urbit personal node daemon")]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    Run(RunArgs),
    Diagnostics(DiagnosticsArgs),
}

#[derive(Args)]
struct RunArgs {
    /// Path to config file (TOML/JSON)
    #[arg(long)]
    config: Option<String>,

    /// Port to listen on for QUIC connections
    #[arg(long)]
    port: Option<u16>,

    /// Data directory for node state
    #[arg(long)]
    data_dir: Option<String>,

    /// HTTP listen address for admin API
    #[arg(long)]
    http_addr: Option<String>,

    /// Admin password hash (argon2id)
    #[arg(long)]
    admin_password_hash: Option<String>,

    /// Admin token hash (sha256 hex)
    #[arg(long)]
    admin_token_hash: Option<String>,

    /// Session secret (hex-encoded)
    #[arg(long)]
    session_secret: Option<String>,

    /// Session timeout in hours
    #[arg(long)]
    session_timeout_hours: Option<u32>,

    /// Enable verbose logging
    #[arg(short, long)]
    verbose: bool,
}

impl Default for RunArgs {
    fn default() -> Self {
        Self {
            config: None,
            port: None,
            data_dir: None,
            http_addr: None,
            admin_password_hash: None,
            admin_token_hash: None,
            session_secret: None,
            session_timeout_hours: None,
            verbose: false,
        }
    }
}

#[derive(Args)]
struct DiagnosticsArgs {
    #[command(subcommand)]
    command: DiagnosticsCommand,

    /// Data directory for node state
    #[arg(long, default_value = "./data")]
    data_dir: String,
}

#[derive(Subcommand)]
enum DiagnosticsCommand {
    /// Create a diagnostic bundle archive
    Dump {
        /// Output path for archive
        #[arg(long)]
        output: String,
    },
    /// Print a diagnostic snapshot to stdout
    Status,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    match cli.command.unwrap_or(Command::Run(RunArgs::default())) {
        Command::Run(args) => {
            let log_level = if args.verbose { "debug" } else { "info" };
            tracing_subscriber::fmt().with_env_filter(log_level).init();

            let mut overrides = std::collections::HashMap::new();
            if let Some(port) = args.port {
                overrides.insert("port".to_string(), port.to_string());
            }
            if let Some(data_dir) = args.data_dir.clone() {
                overrides.insert("data_dir".to_string(), data_dir);
            }
            if let Some(http_addr) = args.http_addr.clone() {
                overrides.insert("http_addr".to_string(), http_addr);
            }
            if let Some(hash) = args.admin_password_hash.clone() {
                overrides.insert("admin_password_hash".to_string(), hash);
            }
            if let Some(hash) = args.admin_token_hash.clone() {
                overrides.insert("admin_token_hash".to_string(), hash);
            }
            if let Some(secret) = args.session_secret.clone() {
                overrides.insert("session_secret".to_string(), secret);
            }
            if let Some(timeout) = args.session_timeout_hours {
                overrides.insert("session_timeout_hours".to_string(), timeout.to_string());
            }

            let daemon = load_config(args.config.as_deref(), overrides)?;
            let config = build_node_config(
                daemon,
                vec!["/ip4/127.0.0.1/tcp/4001/p2p/12D3KooWAbc123...".to_string()],
            )?;

            let node = PostUrbitNode::new(config).await?;
            info!("Node initialized, entering run loop");
            node.run().await?;
        }
        Command::Diagnostics(args) => {
            let log_dir = std::path::Path::new(&args.data_dir).join("logs");
            let settings = default_node_settings(&args.data_dir, log_dir.to_string_lossy().as_ref());
            let admin = AdminState::load(&args.data_dir, settings).await?;
            let identity_dir = std::path::Path::new(&args.data_dir).join("identity");
            let identity = IdentityManager::new(identity_dir.to_string_lossy().as_ref()).await?;
            match args.command {
                DiagnosticsCommand::Dump { output } => {
                    diagnostics::write_bundle(
                        &admin,
                        &identity,
                        None,
                        std::time::Instant::now(),
                        std::path::Path::new(&output),
                    )
                    .await?;
                }
                DiagnosticsCommand::Status => {
                    let snapshot = diagnostics::collect_snapshot(
                        &admin,
                        &identity,
                        None,
                        std::time::Instant::now(),
                    )
                    .await?;
                    let payload = serde_json::to_string_pretty(&snapshot)?;
                    println!("{payload}");
                }
            }
        }
    }

    Ok(())
}
