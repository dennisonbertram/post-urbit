use clap::{Args, Parser, Subcommand};
use tracing::info;

use post_urbit_core::diagnostics;
use post_urbit_core::node::{NodeConfig, PostUrbitNode};
use post_urbit_core::admin_state::AdminState;
use post_urbit_core::identity::IdentityManager;
use post_urbit_core::node_config::default_node_settings;

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
    /// Port to listen on for QUIC connections
    #[arg(long, default_value = "4433")]
    port: u16,

    /// Data directory for node state
    #[arg(long, default_value = "./data")]
    data_dir: String,

    /// HTTP listen address for admin API
    #[arg(long, default_value = "127.0.0.1:8080")]
    http_addr: String,

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
    #[arg(long, default_value = "24")]
    session_timeout_hours: u32,

    /// Enable verbose logging
    #[arg(short, long)]
    verbose: bool,
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

    match cli.command.unwrap_or(Command::Run(RunArgs {
        port: 4433,
        data_dir: "./data".to_string(),
        http_addr: "127.0.0.1:8080".to_string(),
        admin_password_hash: None,
        admin_token_hash: None,
        session_secret: None,
        session_timeout_hours: 24,
        verbose: false,
    })) {
        Command::Run(args) => {
            let log_level = if args.verbose { "debug" } else { "info" };
            tracing_subscriber::fmt().with_env_filter(log_level).init();

            let config = NodeConfig {
                port: args.port,
                data_dir: args.data_dir,
                bootstrap_peers: vec![
                    "/ip4/127.0.0.1/tcp/4001/p2p/12D3KooWAbc123...".to_string(),
                ],
                http_addr: args.http_addr.parse()?,
                metrics_enabled: true,
                admin_password_hash: args.admin_password_hash,
                admin_token_hash: args.admin_token_hash,
                session_secret: args.session_secret,
                session_timeout_hours: args.session_timeout_hours,
            };

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
