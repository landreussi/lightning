use crate::{
    domain::{Node, PublicKey},
    error::{OptionExt, Result},
    repository::SharedNodeRepository,
};

/// Serving the stored nodes.
#[derive(Clone)]
pub struct NodeHandler {
    pub db: SharedNodeRepository,
}

impl NodeHandler {
    #[tracing::instrument(skip(self))]
    pub async fn list_nodes(&self) -> Result<Vec<Node>> {
        self.db.list().await
    }

    #[tracing::instrument(skip(self))]
    pub async fn get_node(&self, public_key: PublicKey) -> Result<Node> {
        self.db.get(public_key).await?.ok_or_not_found()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use time::macros::datetime;

    use super::*;
    use crate::{domain::Node, error::Error, repository::MockNodeRepository, testing::node};

    fn handler(db: MockNodeRepository) -> NodeHandler {
        NodeHandler { db: Arc::new(db) }
    }

    #[tokio::test]
    async fn list_nodes_returns_what_the_repository_holds() {
        let mut db = MockNodeRepository::new();
        db.expect_list()
            .times(1)
            .returning(|| Ok(vec![node(Node::ACINQ, "ACINQ", 36_010_516_297)]));

        let listed = handler(db).list_nodes().await.unwrap();

        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].alias, "ACINQ");
        assert_eq!(listed[0].capacity.to_string(), "360.10516297");
        assert_eq!(listed[0].first_seen, datetime!(2018-04-05 15:13:42 UTC));
    }

    #[tokio::test]
    async fn get_node_looks_the_key_up() {
        let key: PublicKey = Node::ACINQ.parse().unwrap();

        let mut db = MockNodeRepository::new();
        db.expect_get()
            .withf(move |asked| *asked == key)
            .times(1)
            .returning(|key| Ok(Some(node(&key.to_string(), "ACINQ", 1))));

        let found = handler(db).get_node(key).await.unwrap();

        assert_eq!(found.public_key, key);
    }

    #[tokio::test]
    async fn get_node_is_not_found_when_the_key_is_unknown() {
        let mut db = MockNodeRepository::new();
        db.expect_get().returning(|_| Ok(None));

        let result = handler(db).get_node(Node::ACINQ.parse().unwrap()).await;

        assert!(matches!(result, Err(Error::NotFound)));
    }

    #[tokio::test]
    async fn repository_errors_are_propagated() {
        let mut db = MockNodeRepository::new();
        db.expect_list()
            .returning(|| Err(Error::Postgres(diesel::result::Error::NotFound)));

        assert!(matches!(
            handler(db).list_nodes().await,
            Err(Error::Postgres(_))
        ));
    }
}
