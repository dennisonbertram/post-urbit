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
//!         http_addr: "127.0.0.1:8080".parse().unwrap(),
//!         metrics_enabled: true,
//!         admin_password_hash: None,
//!         admin_token_hash: None,
//!         session_secret: None,
//!         session_timeout_hours: 24,
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
pub mod health;
pub mod logging;
pub mod metrics;
pub mod diagnostics;
pub mod ratchet;
pub mod mailbox;
pub mod messaging_service;
pub mod mailbox_client;
pub mod harness;
pub mod runtime_wasm;
pub mod mailbox_store;
pub mod mailbox_http;
pub mod nat;
pub mod relay;
pub mod relay_client;
pub mod group;
pub mod admin_types;
pub mod admin_state;
pub mod admin_auth;
pub mod app_store;
pub mod event_bus;
pub mod scheduler;

pub use crate::node::{PostUrbitNode, NodeConfig};
