use std::collections::HashMap;

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
    repository::{NodeRepository, postgres::PostgresRepository},
    schema::nodes,
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
        // `ON CONFLICT DO UPDATE` refuses to touch the same row twice in one
        // statement, so a batch that repeats a public key would fail as a
        // whole. Keep one entry per key instead.
        let rows: Vec<Node> = nodes
            .into_iter()
            .map(|node| (node.public_key, node))
            .collect::<HashMap<_, _>>()
            .into_values()
            .collect();

        if rows.is_empty() {
            return Ok(0);
        }

        self.with_conn(async move |conn| {
            diesel::insert_into(nodes::table)
                .values(rows)
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
        self.with_conn(async move |conn| {
            nodes::table
                .select(Node::as_select())
                .order(nodes::capacity.desc())
                .load(conn)
                .await
        })
        .await
    }

    #[tracing::instrument(skip(self))]
    async fn get(&self, public_key: PublicKey) -> Result<Option<Node>> {
        self.with_conn(async move |conn| {
            nodes::table
                .find(public_key.serialize().to_vec())
                .select(Node::as_select())
                .first(conn)
                .await
                .optional()
        })
        .await
    }
}

#[cfg(test)]
mod tests {
    use time::{OffsetDateTime, macros::datetime};

    use super::*;
    use crate::{domain::Node, repository::postgres::PostgresRepositoryBuilder};

    /// Every fixture shares one first-seen date; these tests are about the
    /// rows, not about the timestamp.
    const FIRST_SEEN: OffsetDateTime = datetime!(2018-04-05 15:13:42 UTC);

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

    /// A batch that repeats a key is accepted rather than failing outright.
    #[tokio::test]
    async fn upsert_many_tolerates_a_repeated_key() {
        let repo = repo();

        let written = repo
            .upsert_many(vec![
                Node::wos(1, FIRST_SEEN).with_alias("first".to_string()),
                Node::wos(2, FIRST_SEEN).with_alias("second".to_string()),
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
        // The generator point: a valid key, and not one a node would use.
        let unknown: PublicKey =
            "0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798"
                .parse()
                .unwrap();

        assert!(repo().get(unknown).await.unwrap().is_none());
    }
}
