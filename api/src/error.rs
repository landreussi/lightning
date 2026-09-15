use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use diesel_async::pooled_connection::deadpool::PoolError;
use serde::Serialize;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("postgres: {0}")]
    Postgres(#[from] diesel::result::Error),

    #[error("postgres pool: {0}")]
    Pool(#[from] PoolError),

    #[error("mempool: {0}")]
    Mempool(#[from] mempool::Error),

    #[error(transparent)]
    InvalidPublicKey(#[from] secp256k1::Error),

    #[error("serialising a node: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("resource not found")]
    NotFound,
}

impl Error {
    const fn status(&self) -> StatusCode {
        match self {
            Self::Postgres(_) | Self::Pool(_) | Self::Mempool(_) | Self::Serialization(_) => {
                StatusCode::INTERNAL_SERVER_ERROR
            }
            Self::InvalidPublicKey(_) => StatusCode::BAD_REQUEST,
            Self::NotFound => StatusCode::NOT_FOUND,
        }
    }
}

/// What a failed request answers with.
#[derive(Debug, Serialize)]
struct ErrorBody {
    error: String,
}

impl IntoResponse for Error {
    fn into_response(self) -> Response {
        let status = self.status();

        // Server-side faults are logged in full; the caller only gets the
        // message, never a connection string or a driver backtrace.
        let error = if status == StatusCode::INTERNAL_SERVER_ERROR {
            tracing::error!(error = %self, "request failed");
            "internal server error".to_string()
        } else {
            self.to_string()
        };

        (status, Json(ErrorBody { error })).into_response()
    }
}

pub type Result<T> = std::result::Result<T, Error>;

pub trait OptionExt<T> {
    fn ok_or_not_found(self) -> Result<T>;
}

impl<T> OptionExt<T> for Option<T> {
    fn ok_or_not_found(self) -> Result<T> {
        self.ok_or(Error::NotFound)
    }
}
