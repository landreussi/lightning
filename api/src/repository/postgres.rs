use diesel_async::{
    AsyncPgConnection,
    pooled_connection::{
        AsyncDieselConnectionManager,
        deadpool::{BuildError, Pool},
    },
};
use serde::Deserialize;
use url::Url;

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
}

/// A repository that leaves nothing behind.
///
/// Every query of a test runs on one connection inside a transaction that is
/// never committed, so the test reads its own writes and dropping it rolls
/// them back. A pool of one is what makes that work: a transaction is only
/// open on the connection that began it.
#[cfg(test)]
pub struct TestPostgresRepository(PostgresRepository);

#[cfg(test)]
impl TestPostgresRepository {
    pub async fn new() -> Self {
        use diesel_async::AsyncConnection as _;

        // The tests read the same `.env` the binaries do.
        dotenvy::dotenv().ok();

        let manager = AsyncDieselConnectionManager::<AsyncPgConnection>::new(
            std::env::var("DATABASE_URL").expect("DATABASE_URL to be set"),
        );
        let pool = Pool::builder(manager)
            .max_size(1)
            .build()
            .expect("the pool to build");

        pool.get()
            .await
            .expect("a connection")
            .begin_test_transaction()
            .await
            .expect("a test transaction");

        Self(PostgresRepository::new(pool))
    }
}

#[cfg(test)]
impl std::ops::Deref for TestPostgresRepository {
    type Target = PostgresRepository;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
