//! # Post-Urbit Core Node
//!
//! This crate implements the core Post-Urbit node with Transport, Identity, and DHT layers.
//!
//! ## Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────┐
//! │             PostUrbitNode               │
//! │                                         │
//! │  ┌─────────────┐ ┌─────────────┐        │
//! │  │ Identity    │ │ Transport   │        │
//! │  │ Manager     │ │ (QUIC)      │        │
//! │  └─────────────┘ └─────────────┘        │
//! │         │               │               │
//! │         └───────┬───────┘               │
//! │                 │                       │
//! │          ┌─────────────┐                │
//! │          │    DHT      │                │
//! │          │ (libp2p)    │                │
//! │          └─────────────┘                │
//! └─────────────────────────────────────────┘
//! ```
//!
//! ## Key Components
//!
//! - **Identity Layer**: Manages cryptographic identity, key rotation, and document signing
//! - **Transport Layer**: QUIC-based networking with identity handshake
//! - **DHT Layer**: Distributed hash table for peer discovery and identity publication
//!
//! ## Usage
//!
//! ```rust,no_run
//! use post_urbit_core::{PostUrbitNode, NodeConfig};
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let config = NodeConfig {
//!         port: 4433,
//!         data_dir: "./data".to_string(),
//!         bootstrap_peers: vec![],
//!     };
//!
//!     let node = PostUrbitNode::new(config).await?;
//!     node.run().await?;
//!
//!     Ok(())
//! }
//! ```

pub mod transport;
pub mod identity;
pub mod dht;
pub mod encoding;
pub mod canonical_json;
pub mod error;
pub mod node;
pub mod messaging;
pub mod sync;
pub mod runtime;
pub mod node_config;
pub mod node_backup;
pub mod node_http;
pub mod ratchet;
pub mod mailbox;
pub mod messaging_service;

pub use crate::node::{PostUrbitNode, NodeConfig};
