use std::sync::Arc;

use tracing::info;

use crate::dht::MemoryDht;
use crate::identity::{publish_genesis, publish_identity, IdentityManager};
use crate::transport::QuicTransport;
use crate::error::Result;

#[derive(Clone)]
pub struct NodeConfig {
    pub port: u16,
    pub data_dir: String,
    pub bootstrap_peers: Vec<String>,
}

pub struct PostUrbitNode {
    config: NodeConfig,
    identity: Arc<IdentityManager>,
    transport: Arc<QuicTransport>,
    dht: Arc<MemoryDht>,
}

impl PostUrbitNode {
    pub async fn new(config: NodeConfig) -> Result<Self> {
        info!("Initializing Post-Urbit node...");

        let identity = Arc::new(IdentityManager::new(&config.data_dir).await?);
        info!("Identity loaded: {}", identity.iid());

        let dht = Arc::new(MemoryDht::new());
        info!("DHT initialized (in-memory)");

        let transport = Arc::new(QuicTransport::new(config.port, identity.clone()).await?);
        info!("QUIC transport initialized on port {}", config.port);

        if identity.identity_document().sequence == "0" {
            publish_genesis(dht.as_ref(), identity.identity_document()).await?;
        } else {
            publish_identity(dht.as_ref(), identity.identity_document()).await?;
        }
        info!("Identity published to DHT");

        Ok(Self {
            config,
            identity,
            transport,
            dht,
        })
    }

    pub async fn run(self) -> Result<()> {
        info!("Post-Urbit node running...");
        info!("Node IID: {}", self.identity.iid());
        info!("Listening on port {}", self.config.port);

        let transport = self.transport.clone();
        let transport_handle = tokio::spawn(async move { transport.run().await });

        tokio::signal::ctrl_c().await?;
        info!("Shutdown signal received");

        transport_handle.abort();
        Ok(())
    }
}
