use std::sync::Arc;

use crate::{
    domain::{Node, PublicKey},
    error::Result,
};

pub mod postgres;
pub mod schema;

pub type SharedNodeRepository = Arc<dyn NodeRepository + Send + Sync + 'static>;

/// Where lightning nodes are kept.
#[cfg_attr(test, mockall::automock)]
#[async_trait::async_trait]
pub trait NodeRepository {
    /// Stores every node, overwriting the fields of the ones already known —
    /// the public key identifies a node for good, everything else drifts.
    /// Answers with how many rows were written.
    async fn upsert_many(&self, nodes: Vec<Node>) -> Result<usize>;

    /// Every stored node, largest capacity first.
    async fn list(&self) -> Result<Vec<Node>>;

    async fn get(&self, public_key: PublicKey) -> Result<Option<Node>>;
}
