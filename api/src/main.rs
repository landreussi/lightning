use std::{io, sync::Arc};

use api::{
    Service,
    handler::{Handler, node::NodeHandler},
    repository::postgres::PostgresRepositoryBuilder,
};

#[tokio::main]
async fn main() -> io::Result<()> {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt::init();

    let repository = Arc::new(
        PostgresRepositoryBuilder::from_env()
            .expect("DATABASE_URL to be set")
            .build()
            .expect("the postgres pool to build"),
    );

    Service {
        handler: Handler {
            node: NodeHandler { db: repository },
        },
    }
    .serve()
    .await
}
