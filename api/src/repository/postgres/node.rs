use std::collections::HashMap;

use diesel::{Selectable, prelude::*, upsert::excluded};
use diesel_async::RunQueryDsl;
use rust_decimal::Decimal;
use time::OffsetDateTime;

use crate::{
    domain::{Node, PublicKey},
    error::Result,
    repository::{NodeRepository, postgres::PostgresRepository},
    schema::nodes,
};

/// A row of `nodes`, minus the columns Postgres maintains itself.
#[derive(Debug, Insertable, Queryable, Selectable)]
#[diesel(table_name = nodes)]
#[diesel(check_for_backend(diesel::pg::Pg))]
struct NodeRow {
    public_key: Vec<u8>,
    alias: String,
    capacity: Decimal,
    first_seen: OffsetDateTime,
}

impl From<Node> for NodeRow {
    fn from(node: Node) -> Self {
        Self {
            public_key: node.public_key.serialize().to_vec(),
            alias: node.alias,
            capacity: node.capacity,
            first_seen: node.first_seen,
        }
    }
}

impl TryFrom<NodeRow> for Node {
    type Error = crate::error::Error;

    fn try_from(row: NodeRow) -> Result<Self> {
        Ok(Self {
            // The column is `CHECK (octet_length(public_key) = 33)`, so this
            // only trips if something wrote around the constraint.
            public_key: PublicKey::from_slice(&row.public_key)?,
            alias: row.alias,
            capacity: row.capacity,
            first_seen: row.first_seen,
        })
    }
}

#[async_trait::async_trait]
impl NodeRepository for PostgresRepository {
    #[tracing::instrument(skip(self, nodes), fields(count = nodes.len()))]
    async fn upsert_many(&self, nodes: Vec<Node>) -> Result<usize> {
        // `ON CONFLICT DO UPDATE` refuses to touch the same row twice in one
        // statement, so a batch that repeats a public key would fail as a
        // whole. Keep one entry per key instead.
        let rows: Vec<NodeRow> = nodes
            .into_iter()
            .map(|node| (node.public_key, node))
            .collect::<HashMap<_, _>>()
            .into_values()
            .map(NodeRow::from)
            .collect();

        if rows.is_empty() {
            return Ok(0);
        }

        self.with_conn(async move |conn| {
            diesel::insert_into(nodes::table)
                .values(&rows)
                .on_conflict(nodes::public_key)
                .do_update()
                .set((
                    nodes::alias.eq(excluded(nodes::alias)),
                    nodes::capacity.eq(excluded(nodes::capacity)),
                    nodes::first_seen.eq(excluded(nodes::first_seen)),
                    nodes::synced_at.eq(diesel::dsl::now),
                ))
                .execute(conn)
                .await
        })
        .await
    }

    #[tracing::instrument(skip(self))]
    async fn list(&self) -> Result<Vec<Node>> {
        let rows = self
            .with_conn(async move |conn| {
                nodes::table
                    .select(NodeRow::as_select())
                    .order(nodes::capacity.desc())
                    .load(conn)
                    .await
            })
            .await?;

        rows.into_iter().map(Node::try_from).collect()
    }

    #[tracing::instrument(skip(self))]
    async fn get(&self, public_key: PublicKey) -> Result<Option<Node>> {
        let row = self
            .with_conn(async move |conn| {
                nodes::table
                    .find(public_key.serialize().to_vec())
                    .select(NodeRow::as_select())
                    .first(conn)
                    .await
                    .optional()
            })
            .await?;

        row.map(Node::try_from).transpose()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{domain::Node, repository::postgres::PostgresRepositoryBuilder, testing::node};

    fn repo() -> PostgresRepository {
        // The tests read the same `.env` the binaries do.
        dotenvy::dotenv().ok();

        PostgresRepositoryBuilder::from_env()
            .expect("DATABASE_URL to be set")
            .build()
            .expect("the pool to build")
    }

    #[tokio::test]
    async fn upsert_many_writes_then_updates_in_place() {
        let repo = repo();

        assert_eq!(
            repo.upsert_many(vec![node(Node::ACINQ, "ACINQ", 36_010_516_297)])
                .await
                .unwrap(),
            1
        );

        let stored = repo
            .get(Node::ACINQ.parse().unwrap())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(stored.alias, "ACINQ");
        assert_eq!(stored.capacity.to_string(), "360.10516297");

        // Same key, new figures: the row is updated, not duplicated.
        repo.upsert_many(vec![node(Node::ACINQ, "ACINQ renamed", 1)])
            .await
            .unwrap();

        let stored = repo
            .get(Node::ACINQ.parse().unwrap())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(stored.alias, "ACINQ renamed");
        assert_eq!(stored.capacity.to_string(), "0.00000001");
    }

    /// A batch that repeats a key is accepted rather than failing outright.
    #[tokio::test]
    async fn upsert_many_tolerates_a_repeated_key() {
        let repo = repo();

        let written = repo
            .upsert_many(vec![
                node(Node::WOS, "first", 1),
                node(Node::WOS, "second", 2),
            ])
            .await
            .unwrap();

        assert_eq!(written, 1);
        assert!(
            repo.get(Node::WOS.parse().unwrap())
                .await
                .unwrap()
                .is_some()
        );
    }

    #[tokio::test]
    async fn list_orders_by_capacity() {
        let repo = repo();
        repo.upsert_many(vec![
            node(Node::ACINQ, "ACINQ", 36_010_516_297),
            node(Node::WOS, "WalletOfSatoshi", 1),
        ])
        .await
        .unwrap();

        let listed = repo.list().await.unwrap();
        let capacities: Vec<_> = listed.iter().map(|node| node.capacity).collect();

        assert!(capacities.windows(2).all(|pair| pair[0] >= pair[1]));
    }

    #[tokio::test]
    async fn get_answers_none_for_an_unknown_key() {
        // The generator point: a valid key, and not one a node would use.
        let unknown: PublicKey =
            "0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798"
                .parse()
                .unwrap();

        assert!(repo().get(unknown).await.unwrap().is_none());
    }
}
