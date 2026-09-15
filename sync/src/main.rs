//! Brings the stored nodes up to date with mempool.space.
//!
//! Runs once and exits, so a crontab entry can drive the schedule. It reads
//! `DATABASE_URL` and `MEMPOOL_ENDPOINT` from the `.env` next to it, so the
//! entry carries no configuration of its own:
//!
//! ```crontab
//! */15 * * * * cd /srv/api && ./sync
//! ```
//!
//! A non-zero exit means nothing was written, which is what cron reports on.

use std::{process::ExitCode, sync::Arc};

use api::{
    handler::sync::SyncHandler, repository::postgres::PostgresRepositoryBuilder,
    source::MempoolClientBuilder,
};

#[tokio::main]
async fn main() -> ExitCode {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt::init();

    let handler = SyncHandler {
        db: Arc::new(
            PostgresRepositoryBuilder::from_env()
                .expect("DATABASE_URL to be set")
                .build()
                .expect("the postgres pool to build"),
        ),
        source: Arc::new(
            MempoolClientBuilder::from_env()
                .expect("MEMPOOL_ENDPOINT to be set")
                .build(),
        ),
    };

    match handler.sync().await {
        Ok(stored) => {
            tracing::info!(stored, "sync finished");
            ExitCode::SUCCESS
        }
        Err(err) => {
            tracing::error!(%err, "sync failed");
            ExitCode::FAILURE
        }
    }
}
