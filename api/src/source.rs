//! BTC Lightning node data source.

use std::sync::Arc;

use mempool::MempoolClient;
use serde::Deserialize;
use url::Url;

use crate::{domain::Node, error::Result};

pub type SharedNodeSource = Arc<dyn NodeSource + Send + Sync + 'static>;

/// An upstream that knows about lightning nodes.
#[cfg_attr(test, mockall::automock)]
#[async_trait::async_trait]
pub trait NodeSource {
    /// The nodes worth storing, as the upstream currently sees them.
    async fn fetch_nodes(&self) -> Result<Vec<Node>>;
}

/// Reads the mempool.space settings out of the environment.
#[derive(Debug, Deserialize)]
pub struct MempoolClientBuilder {
    /// The full ranking URL, e.g.
    /// `https://mempool.space/api/v1/lightning/nodes/rankings/connectivity`.
    mempool_endpoint: Url,
}

impl MempoolClientBuilder {
    pub fn from_env() -> envy::Result<Self> {
        envy::from_env()
    }

    pub fn build(self) -> MempoolClient {
        MempoolClient::new(self.mempool_endpoint)
    }
}

/// The client is the adapter itself: the only thing this side owns beyond
/// fetching is turning mempool's payload into domain nodes.
#[async_trait::async_trait]
impl NodeSource for MempoolClient {
    #[tracing::instrument(skip(self))]
    async fn fetch_nodes(&self) -> Result<Vec<Node>> {
        let ranking = self.fetch_nodes().await?;
        let fetched = ranking.len();

        let nodes: Vec<_> = ranking
            .into_iter()
            .filter_map(|entry| {
                Node::try_from(entry)
                    .inspect_err(|err| {
                        // One unusable entry: a key that isn't 33 hex-encoded
                        // bytes, it shouldn't cost us the whole ranking, so
                        // it's dropped and logged.
                        tracing::warn!(%err, "skipping an unusable node");
                    })
                    .ok()
            })
            .collect();

        tracing::info!(fetched, usable = nodes.len(), "fetched nodes from mempool");

        Ok(nodes)
    }
}
