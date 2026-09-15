use axum::{
    Json, Router,
    extract::{Path, State},
    response::IntoResponse,
    routing::get,
};

use crate::{Service, domain::PublicKey, error::Result, routes::json_array};

pub fn configure_router() -> Router<Service> {
    Router::new()
        .route("/", get(list_nodes))
        .route("/{public_key}", get(get_node))
}

/// Every stored node, largest capacity first.
///
/// The rows are written to the response as the database produces them, so
/// neither this service nor the caller waits on the whole list.
#[tracing::instrument(skip(state))]
async fn list_nodes(State(state): State<Service>) -> Result<impl IntoResponse> {
    json_array(state.handler.node.list_nodes().await?).await
}

/// One node by its hex-encoded public key.
#[tracing::instrument(skip(state))]
async fn get_node(
    State(state): State<Service>,
    Path(public_key): Path<String>,
) -> Result<impl IntoResponse> {
    let public_key: PublicKey = public_key.parse()?;

    state.handler.node.get_node(public_key).await.map(Json)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use axum::{
        body::Body,
        http::{Request, StatusCode},
    };
    use futures_util::{StreamExt as _, stream};
    use time::macros::datetime;
    use tower::ServiceExt;

    use crate::{
        Service,
        domain::Node,
        error::Error,
        handler::{Handler, node::NodeHandler},
        repository::MockNodeRepository,
    };

    /// The whole router, backed by a mocked repository.
    fn app(db: MockNodeRepository) -> axum::Router {
        let service = Service {
            handler: Handler {
                node: NodeHandler { db: Arc::new(db) },
            },
        };

        crate::routes::initialize().with_state(service)
    }

    async fn get(db: MockNodeRepository, uri: &str) -> (StatusCode, serde_json::Value) {
        let response = app(db)
            .oneshot(Request::get(uri).body(Body::empty()).unwrap())
            .await
            .unwrap();

        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let body = serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null);

        (status, body)
    }

    #[tokio::test]
    async fn list_nodes_answers_the_documented_shape() {
        let mut db = MockNodeRepository::new();
        db.expect_list().returning(|| {
            Ok(stream::once(async {
                Ok(Node::acinq(
                    36_010_516_297,
                    datetime!(2018-04-05 15:13:42 UTC),
                ))
            })
            .boxed())
        });

        let (status, body) = get(db, "/nodes").await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(
            body,
            serde_json::json!([{
                "publicKey": Node::ACINQ,
                "alias": "ACINQ",
                "capacity": "360.10516297",
                "firstSeen": "2018-04-05T15:13:42Z",
            }])
        );
    }

    #[tokio::test]
    async fn get_node_answers_one_node() {
        let mut db = MockNodeRepository::new();
        db.expect_get()
            .returning(|_| Ok(Some(Node::acinq(1, datetime!(2018-04-05 15:13:42 UTC)))));

        let (status, body) = get(db, &format!("/nodes/{}", Node::ACINQ)).await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["publicKey"], Node::ACINQ);
    }

    #[tokio::test]
    async fn an_unknown_node_is_not_found() {
        let mut db = MockNodeRepository::new();
        db.expect_get().returning(|_| Ok(None));

        let (status, _) = get(db, &format!("/nodes/{}", Node::ACINQ)).await;

        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    /// A key that isn't 33 hex-encoded bytes is the caller's mistake, and never
    /// reaches the repository.
    #[tokio::test]
    async fn a_malformed_key_is_rejected() {
        let mut db = MockNodeRepository::new();
        db.expect_get().never();

        let (status, body) = get(db, "/nodes/not-a-key").await;

        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert!(body["error"].is_string());
    }

    /// A repository fault is a 500 that doesn't leak the driver's message.
    #[tokio::test]
    async fn a_repository_failure_is_an_internal_error() {
        let mut db = MockNodeRepository::new();
        db.expect_list()
            .returning(|| Err(Error::Postgres(diesel::result::Error::NotFound)));

        let (status, body) = get(db, "/nodes").await;

        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(body["error"], "internal server error");
    }
}
