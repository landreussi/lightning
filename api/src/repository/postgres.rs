use diesel::QueryResult;
use diesel_async::{
    AsyncPgConnection,
    pooled_connection::{
        AsyncDieselConnectionManager,
        deadpool::{BuildError, Object, Pool},
    },
};
use serde::Deserialize;
use url::Url;

use crate::error::Result;

pub mod node;

/// Reads the Postgres connection settings out of the environment.
#[derive(Debug, Deserialize)]
pub struct PostgresRepositoryBuilder {
    database_url: Url,
}

impl PostgresRepositoryBuilder {
    pub fn from_env() -> envy::Result<Self> {
        envy::from_env()
    }

    #[tracing::instrument]
    pub fn build(self) -> std::result::Result<PostgresRepository, BuildError> {
        let manager =
            AsyncDieselConnectionManager::<AsyncPgConnection>::new(self.database_url.as_str());
        let pool = Pool::builder(manager).build()?;

        Ok(PostgresRepository::new(pool))
    }
}

/// The Postgres side of every repository port.
#[derive(Clone)]
pub struct PostgresRepository {
    pool: Pool<AsyncPgConnection>,
}

impl PostgresRepository {
    pub const fn new(pool: Pool<AsyncPgConnection>) -> Self {
        Self { pool }
    }

    /// Runs one query on a pooled connection, so every repository method below
    /// is the query itself and nothing else: checking a connection out and
    /// turning both failures into [`crate::error::Error`] happens here.
    async fn with_conn<T, Q>(&self, query: Q) -> Result<T>
    where
        Q: AsyncFnOnce(&mut AsyncPgConnection) -> QueryResult<T>,
    {
        let mut conn: Object<AsyncPgConnection> = self.pool.get().await?;

        Ok(query(&mut conn).await?)
    }
}
