use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use utoipa::ToSchema;

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ErrorResponse {
    pub error: String,
    pub status: u16,
}

#[derive(Error, Debug)]
pub enum AppError {
    #[error("Satellite not found")]
    NotFound,

    #[error("Invalid request input: {0}")]
    BadRequest(String),

    #[error("SGP4 calculation error: {0}")]
    Sgp4Error(String),

    #[error("Internal server error: {0}")]
    InternalServerError(String),

    #[error("Unauthorized: {0}")]
    Unauthorized(String),

    #[error("Forbidden: {0}")]
    Forbidden(String),

    #[error("Too Many Requests: {0}")]
    TooManyRequests(String),

    #[error("Service unavailable: {0}")]
    ServiceUnavailable(String),

    #[error("Gateway timeout: {0}")]
    GatewayTimeout(String),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, error_message) = match &self {
            AppError::NotFound => (StatusCode::NOT_FOUND, self.to_string()),
            AppError::BadRequest(ref msg) => (StatusCode::BAD_REQUEST, msg.clone()),
            AppError::Sgp4Error(ref msg) => (StatusCode::UNPROCESSABLE_ENTITY, msg.clone()),
            AppError::InternalServerError(ref msg) => {
                (StatusCode::INTERNAL_SERVER_ERROR, msg.clone())
            }
            AppError::Unauthorized(ref msg) => (StatusCode::UNAUTHORIZED, msg.clone()),
            AppError::Forbidden(ref msg) => (StatusCode::FORBIDDEN, msg.clone()),
            AppError::TooManyRequests(ref msg) => (StatusCode::TOO_MANY_REQUESTS, msg.clone()),
            AppError::ServiceUnavailable(ref msg) => (StatusCode::SERVICE_UNAVAILABLE, msg.clone()),
            AppError::GatewayTimeout(ref msg) => (StatusCode::GATEWAY_TIMEOUT, msg.clone()),
        };

        let body = Json(ErrorResponse {
            error: error_message,
            status: status.as_u16(),
        });

        (status, body).into_response()
    }
}
