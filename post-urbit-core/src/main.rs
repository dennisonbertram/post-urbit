use clap::Parser;
use tracing::info;

use post_urbit_core::node::{NodeConfig, PostUrbitNode};

#[derive(Parser)]
#[command(name = "post-urbit-node")]
#[command(about = "Post-Urbit personal node daemon")]
struct Args {
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

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

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

    Ok(())
}
