use crate::{error::Result, repository::SharedNodeRepository, source::SharedNodeSource};

/// Bringing the stored nodes up to date with the upstream.
///
/// Its own use case rather than something the sync binary wires by hand, so the
/// job stays a `main` that calls one method.
#[derive(Clone)]
pub struct SyncHandler {
    pub db: SharedNodeRepository,
    pub source: SharedNodeSource,
}

impl SyncHandler {
    /// Fetches the upstream ranking and writes it, answering with how many
    /// nodes were stored.
    #[tracing::instrument(skip(self))]
    pub async fn sync(&self) -> Result<usize> {
        let nodes = self.source.fetch_nodes().await?;
        let stored = self.db.upsert_many(nodes).await?;

        tracing::info!(stored, "synced the node ranking");

        Ok(stored)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use time::macros::datetime;

    use super::*;
    use crate::{
        domain::Node, error::Error, repository::MockNodeRepository, source::MockNodeSource,
    };

    #[tokio::test]
    async fn sync_stores_every_fetched_node() {
        let mut source = MockNodeSource::new();
        source.expect_fetch_nodes().times(1).returning(|| {
            Ok(vec![
                Node::acinq(36_010_516_297, datetime!(2018-04-05 15:13:42 UTC)),
                Node::wos(1, datetime!(2018-04-05 15:13:42 UTC)),
            ])
        });

        let mut db = MockNodeRepository::new();
        db.expect_upsert_many()
            .withf(|nodes| nodes.len() == 2 && nodes[0].alias == "ACINQ")
            .times(1)
            .returning(|nodes| Ok(nodes.len()));

        let handler = SyncHandler {
            db: Arc::new(db),
            source: Arc::new(source),
        };

        assert_eq!(handler.sync().await.unwrap(), 2);
    }

    /// The upstream being down must not look like a successful empty sync.
    #[tokio::test]
    async fn sync_fails_when_the_source_fails_and_writes_nothing() {
        let mut source = MockNodeSource::new();
        source
            .expect_fetch_nodes()
            .returning(|| Err(Error::NotFound));

        let mut db = MockNodeRepository::new();
        db.expect_upsert_many().never();

        let handler = SyncHandler {
            db: Arc::new(db),
            source: Arc::new(source),
        };

        assert!(handler.sync().await.is_err());
    }
}
