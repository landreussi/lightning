use std::pin::Pin;

use async_stream::try_stream;
use axum::{
    Router,
    body::{Body, Bytes},
    http::{StatusCode, header},
    response::{IntoResponse, Response},
    routing::get,
};
use futures_util::{StreamExt as _, stream::BoxStream};
use serde::Serialize;

use crate::{Service, error::Result};

pub mod node;

pub fn initialize() -> Router<Service> {
    Router::new()
        .route("/health", get(health))
        .nest("/nodes", node::configure_router())
}

/// Answers with a JSON array written as the rows arrive, so a large answer
/// costs neither this service nor the caller the whole collection.
///
/// The first row is awaited before anything is written: until then a failure
/// is still a status code, and past it the response is on the wire and can
/// only end short.
#[tracing::instrument(skip_all)]
async fn json_array<T>(rows: BoxStream<'static, Result<T>>) -> Result<Response>
where
    T: Serialize + Send + 'static,
{
    let mut rows = rows.peekable();

    // This looks duplicated, but the peek method gets a pointer, if there is a
    // error in it, we should get the error as a value, this is why the next
    // verification is needed.
    if let Some(Err(_)) = Pin::new(&mut rows).peek().await
        && let Some(Err(err)) = rows.next().await
    {
        return Err(err);
    }

    // Annotated so the macro knows what a `?` in it converts into.
    let body: BoxStream<'_, Result<_>> = Box::pin(try_stream! {
        yield Bytes::from_static(b"[");

        let mut separator = "";
        while let Some(row) = rows.next().await {
            let mut item = Vec::from(separator);
            serde_json::to_writer(&mut item, &row?)?;
            separator = ",";

            yield Bytes::from(item);
        }

        yield Bytes::from_static(b"]");
    });

    Ok((
        [(header::CONTENT_TYPE, "application/json")],
        Body::from_stream(body),
    )
        .into_response())
}

/// Liveness for whatever runs this: answers as soon as the server is up.
async fn health() -> StatusCode {
    StatusCode::NO_CONTENT
}
