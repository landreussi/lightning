//! A service that serves the BTC lightning nodes it has been told about.
//!
//! Ports and adapters: [`handler`] holds the use cases and talks only to the
//! ports ([`repository::NodeRepository`], [`source::NodeSource`]); the adapters
//! behind them (Postgres through [`repository::postgres`], mempool.space
//! through [`mempool::MempoolClient`]) are wired in at startup, and [`routes`]
//! is the HTTP adapter driving it all.

use std::{
    io,
    net::{Ipv4Addr, SocketAddr},
};

use tokio::net::TcpListener;

use crate::handler::Handler;

pub mod domain;
pub mod error;
pub mod handler;
pub mod repository;
pub mod routes;
pub mod schema;
pub mod source;

#[cfg(test)]
pub mod testing;

/// Everything a request needs, handed to the routes as axum state.
#[derive(Clone)]
pub struct Service {
    pub handler: Handler,
}

impl Service {
    #[tracing::instrument(skip_all)]
    pub async fn serve(self) -> io::Result<()> {
        let routes = routes::initialize().with_state(self);

        let listener = TcpListener::bind((Ipv4Addr::UNSPECIFIED, Self::port())).await?;
        let addr: SocketAddr = listener.local_addr()?;
        tracing::info!("{} is listening on {addr}", env!("CARGO_CRATE_NAME"));

        axum::serve(listener, routes).await
    }

    /// The port the API listens on, from the `PORT` env var.
    pub fn port() -> u16 {
        const DEFAULT_PORT: u16 = 3000;

        std::env::var("PORT").map_or(DEFAULT_PORT, |port| {
            port.parse()
                .inspect_err(|err: &std::num::ParseIntError| {
                    tracing::warn!(%err, "PORT is set but not a u16, assuming {DEFAULT_PORT}");
                })
                .unwrap_or(DEFAULT_PORT)
        })
    }
}
