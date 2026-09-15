use std::sync::Arc;

use futures_util::stream::BoxStream;

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
    ///
    /// Answers with a stream rather than a `Vec` so a caller can forward rows
    /// as the database produces them. Checking the connection out is what the
    /// outer `Result` reports; a row that fails to come back arrives as an
    /// `Err` item on the stream, by which point a response may already be on
    /// the wire.
    async fn list(&self) -> Result<BoxStream<'static, Result<Node>>>;

    async fn get(&self, public_key: PublicKey) -> Result<Option<Node>>;
}
