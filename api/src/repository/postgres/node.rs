use diesel::{
    deserialize::{self, FromSql, FromSqlRow},
    expression::AsExpression,
    pg::{Pg, PgValue},
    prelude::*,
    serialize::{self, IsNull, Output, ToSql},
    sql_types::Bytea,
    upsert::excluded,
};
use diesel_async::RunQueryDsl;

use crate::{
    domain::{Node, PublicKey},
    error::Result,
    repository::{NodeRepository, postgres::PostgresRepository, schema::nodes},
};

/// The `BYTEA` side of a [`PublicKey`].
///
/// Diesel can't map the key type to a column on its own — the trait and the
/// type are both foreign, so neither `ToSql` nor `FromSql` can be implemented
/// for it here. The column goes through this instead, which is what
/// [`Node`]'s `serialize_as`/`deserialize_as` name.
#[derive(Debug, AsExpression, FromSqlRow)]
#[diesel(sql_type = Bytea)]
pub struct DbPublicKey(PublicKey);

impl From<PublicKey> for DbPublicKey {
    fn from(public_key: PublicKey) -> Self {
        Self(public_key)
    }
}

impl From<DbPublicKey> for PublicKey {
    fn from(stored: DbPublicKey) -> Self {
        stored.0
    }
}

impl ToSql<Bytea, Pg> for DbPublicKey {
    fn to_sql(&self, out: &mut Output<'_, '_, Pg>) -> serialize::Result {
        use std::io::Write as _;

        out.write_all(&self.0.serialize())?;

        Ok(IsNull::No)
    }
}

impl FromSql<Bytea, Pg> for DbPublicKey {
    fn from_sql(bytes: PgValue<'_>) -> deserialize::Result<Self> {
        // The column is `CHECK (octet_length(public_key) = 33)`, so this only
        // trips if something wrote around the constraint.
        PublicKey::from_slice(bytes.as_bytes())
            .map(Self)
            .map_err(Into::into)
    }
}

#[async_trait::async_trait]
impl NodeRepository for PostgresRepository {
    #[tracing::instrument(skip(self, nodes), fields(count = nodes.len()))]
    async fn upsert_many(&self, nodes: Vec<Node>) -> Result<usize> {
        if nodes.is_empty() {
            return Ok(0);
        }

        let mut conn = self.pool.get().await?;

        let rows = diesel::insert_into(nodes::table)
            .values(nodes)
            .on_conflict(nodes::public_key)
            .do_update()
            .set((
                nodes::alias.eq(excluded(nodes::alias)),
                nodes::capacity.eq(excluded(nodes::capacity)),
                nodes::first_seen.eq(excluded(nodes::first_seen)),
                nodes::synced_at.eq(diesel::dsl::now),
            ))
            .execute(&mut conn)
            .await?;

        Ok(rows)
    }

    #[tracing::instrument(skip(self))]
    async fn list(&self) -> Result<Vec<Node>> {
        let mut conn = self.pool.get().await?;

        let nodes = nodes::table
            .select(Node::as_select())
            .order(nodes::capacity.desc())
            .load(&mut conn)
            .await?;

        Ok(nodes)
    }

    #[tracing::instrument(skip(self))]
    async fn get(&self, public_key: PublicKey) -> Result<Option<Node>> {
        let mut conn = self.pool.get().await?;

        let node = nodes::table
            .find(public_key.serialize().to_vec())
            .select(Node::as_select())
            .first(&mut conn)
            .await
            .optional()?;

        Ok(node)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{error::Error, repository::postgres::TestPostgresRepository};

    const FIRST_SEEN: time::OffsetDateTime = time::macros::datetime!(2018-04-05 15:13:42 UTC);

    #[tokio::test]
    async fn upsert_many_writes_then_updates_in_place() {
        let repo = TestPostgresRepository::new().await;

        assert_eq!(
            repo.upsert_many(vec![Node::acinq(36_010_516_297, FIRST_SEEN)])
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
        repo.upsert_many(vec![
            Node::acinq(1, FIRST_SEEN).with_alias("ACINQ renamed".to_string()),
        ])
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

    /// `ON CONFLICT DO UPDATE` refuses to touch the same row twice in one
    /// statement, so a batch is the caller's to deduplicate: a repeated key
    /// fails the whole write rather than half-applying it.
    #[tokio::test]
    async fn upsert_many_rejects_a_repeated_key() {
        let result = TestPostgresRepository::new()
            .await
            .upsert_many(vec![
                Node::wos(1, FIRST_SEEN).with_alias("first".to_string()),
                Node::wos(2, FIRST_SEEN).with_alias("second".to_string()),
            ])
            .await;

        assert!(matches!(result, Err(Error::Postgres(_))));
    }

    #[tokio::test]
    async fn list_orders_by_capacity() {
        let repo = TestPostgresRepository::new().await;
        repo.upsert_many(vec![
            Node::acinq(36_010_516_297, FIRST_SEEN),
            Node::wos(1, FIRST_SEEN),
        ])
        .await
        .unwrap();

        let listed = repo.list().await.unwrap();
        let capacities: Vec<_> = listed.iter().map(|node| node.capacity).collect();

        assert!(capacities.windows(2).all(|pair| pair[0] >= pair[1]));
    }

    #[tokio::test]
    async fn get_answers_none_for_an_unknown_key() {
        let unknown: PublicKey = Node::UNKNOWN.parse().unwrap();

        assert!(
            TestPostgresRepository::new()
                .await
                .get(unknown)
                .await
                .unwrap()
                .is_none()
        );
    }

    /// The isolation itself: a repository sees its own writes, and dropping it
    /// takes them with it. Without this, a test would leave rows behind for
    /// the next one to trip over.
    #[tokio::test]
    async fn writes_are_rolled_back_once_the_repository_is_dropped() {
        let key: PublicKey = Node::UNKNOWN.parse().unwrap();

        let repo = TestPostgresRepository::new().await;
        repo.upsert_many(vec![Node::unknown(1, FIRST_SEEN)])
            .await
            .unwrap();
        assert!(repo.get(key).await.unwrap().is_some());
        drop(repo);

        let other_repo = TestPostgresRepository::new().await;
        assert!(other_repo.get(key).await.unwrap().is_none());
    }
}
