//! A thin client over the [mempool.space] REST API.
//!
//! It only deserialises what mempool.space answers; mapping those payloads onto
//! a domain lives with whoever consumes this crate.
//!
//! [mempool.space]: https://mempool.space/docs/api/rest

pub mod error;
pub use error::*;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use url::Url;

/// One entry of a node ranking.
///
/// Mempool sends more than this (channel counts, geolocation, ...); the fields
/// nothing downstream reads are left out so a change to them can't break
/// deserialisation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Node {
    /// The node's compressed public key, hex encoded (66 chars).
    pub public_key: String,

    pub alias: String,

    /// Total channel capacity, in satoshis.
    pub capacity: i64,

    /// When the node was first seen on the network, as a unix timestamp.
    #[serde(with = "time::serde::timestamp")]
    pub first_seen: OffsetDateTime,
}

/// The whole endpoint URL is a constructor argument, not a base URL this crate
/// appends a path to: which ranking to read, and from which deployment, is the
/// caller's configuration — and it lets the tests point it at a stub.
#[derive(Debug, Clone)]
pub struct MempoolClient {
    http: Client,
    endpoint: Url,
}

impl MempoolClient {
    pub fn new(endpoint: Url) -> Self {
        Self::with_client(Client::new(), endpoint)
    }

    /// Uses an already-configured HTTP client (timeouts, proxies, ...).
    pub const fn with_client(http: Client, endpoint: Url) -> Self {
        Self { http, endpoint }
    }

    /// The nodes the configured endpoint ranks, in its own order.
    #[tracing::instrument(skip(self))]
    pub async fn fetch_nodes(&self) -> Result<Vec<Node>> {
        let nodes = self
            .http
            .get(self.endpoint.as_str())
            .send()
            .await?
            .error_for_status()?
            .json::<Vec<_>>()
            .await?;

        tracing::debug!(count = nodes.len(), "fetched mempool nodes");

        Ok(nodes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ACINQ_KEY: &str = "03864ef025fde8fb587d989186ce6a4a186895ee44a926bfc370e2c366597a3f8f";

    fn client(server: &mockito::Server) -> MempoolClient {
        MempoolClient::new(server.url().parse().expect("a valid endpoint"))
    }

    #[tokio::test]
    async fn connectivity_ranking_parses_the_payload() {
        let mut server = mockito::Server::new_async().await;
        let acinq_only = serde_json::json!([{
            "publicKey": ACINQ_KEY,
            "alias": "ACINQ",
            "channels": 2908,
            "capacity": 36_010_516_297i64,
            "firstSeen": 1_522_941_222,
            "updatedAt": 1_661_274_935,
            "city": null,
            "country": { "en": "United States", "pt-BR": "EUA" }
        }]);
        let mock = server
            .mock("GET", "/")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(acinq_only.to_string())
            .create_async()
            .await;

        let nodes = client(&server).fetch_nodes().await.unwrap();

        mock.assert_async().await;
        assert_eq!(
            nodes,
            vec![Node {
                public_key: ACINQ_KEY.to_string(),
                alias: "ACINQ".to_string(),
                capacity: 36_010_516_297,
                first_seen: time::macros::datetime!(2018-04-05 15:13:42 UTC),
            }]
        );
    }

    /// The endpoint is taken as given: a deployment behind a path prefix is
    /// read there, not at the host's root.
    #[tokio::test]
    async fn the_endpoint_is_requested_verbatim() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("GET", "/")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body("[]")
            .create_async()
            .await;

        let endpoint = server.url().parse().unwrap();
        let nodes = MempoolClient::new(endpoint).fetch_nodes().await.unwrap();

        mock.assert_async().await;
        assert!(nodes.is_empty());
    }

    #[tokio::test]
    async fn an_error_status_is_an_error() {
        let mut server = mockito::Server::new_async().await;
        server
            .mock("GET", "/")
            .with_status(503)
            .create_async()
            .await;

        let result = client(&server).fetch_nodes().await;

        assert!(matches!(result, Err(Error::Http(_))));
    }

    #[tokio::test]
    async fn a_malformed_payload_is_an_error() {
        let mut server = mockito::Server::new_async().await;
        server
            .mock("GET", "/")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"[{"alias": "ACINQ"}]"#)
            .create_async()
            .await;

        let result = client(&server).fetch_nodes().await;

        assert!(matches!(result, Err(Error::Http(_))));
    }
}
