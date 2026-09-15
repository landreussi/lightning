use axum::{Router, http::StatusCode, routing::get};

use crate::Service;

pub mod node;

pub fn initialize() -> Router<Service> {
    Router::new()
        .route("/health", get(health))
        .nest("/nodes", node::configure_router())
}

/// Liveness for whatever runs this: answers as soon as the server is up.
async fn health() -> StatusCode {
    StatusCode::NO_CONTENT
}
