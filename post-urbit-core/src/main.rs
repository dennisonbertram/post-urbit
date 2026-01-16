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
    };

    let node = PostUrbitNode::new(config).await?;
    info!("Node initialized, entering run loop");
    node.run().await?;

    Ok(())
}
