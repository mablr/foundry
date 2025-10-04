//! Beacon API error types

use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};
use std::{borrow::Cow, fmt};

/// Represents a Beacon API error response
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BeaconError {
    /// HTTP status code
    #[serde(skip)]
    pub status_code: u16,
    /// Error code
    pub code: BeaconErrorCode,
    /// Error message
    pub message: Cow<'static, str>,
}

impl BeaconError {
    /// Creates a new beacon error with the given code
    pub fn new(code: BeaconErrorCode, message: impl Into<Cow<'static, str>>) -> Self {
        let status_code = code.status_code();
        Self { status_code, code, message: message.into() }
    }

    /// Creates a 400 Bad Request error
    pub fn bad_request(message: impl Into<Cow<'static, str>>) -> Self {
        Self::new(BeaconErrorCode::BadRequest, message)
    }

    /// Creates a 404 Not Found error
    pub fn not_found(message: impl Into<Cow<'static, str>>) -> Self {
        Self::new(BeaconErrorCode::NotFound, message)
    }

    /// Creates a 500 Internal Server Error
    pub fn internal_error(message: impl Into<Cow<'static, str>>) -> Self {
        Self::new(BeaconErrorCode::InternalError, message)
    }

    /// Creates a 503 Service Unavailable error
    pub fn unavailable(message: impl Into<Cow<'static, str>>) -> Self {
        Self::new(BeaconErrorCode::Unavailable, message)
    }

    /// Converts to an Axum response
    pub fn into_response(self) -> Response {
        let status = StatusCode::from_u16(self.status_code)
            .unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
        
        (status, Json(serde_json::json!({
            "code": self.code as u16,
            "message": self.message,
        }))).into_response()
    }
}

impl fmt::Display for BeaconError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.code.as_str(), self.message)
    }
}

impl std::error::Error for BeaconError {}

impl IntoResponse for BeaconError {
    fn into_response(self) -> Response {
        BeaconError::into_response(self)
    }
}

/// Beacon API error codes following the beacon chain specification
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u16)]
pub enum BeaconErrorCode {
    /// 400 - Invalid block ID or other bad request parameter
    BadRequest = 400,
    /// 404 - Resource not found (block, blob, etc.)
    NotFound = 404,
    /// 500 - Internal server error
    InternalError = 500,
    /// 503 - Service unavailable
    Unavailable = 503,
}

impl BeaconErrorCode {
    /// Returns the HTTP status code for this error
    pub const fn status_code(&self) -> u16 {
        *self as u16
    }

    /// Returns a string representation of the error code
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::BadRequest => "Bad Request",
            Self::NotFound => "Not Found",
            Self::InternalError => "Internal Server Error",
            Self::Unavailable => "Service Unavailable",
        }
    }
}

impl fmt::Display for BeaconErrorCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_beacon_error_codes() {
        assert_eq!(BeaconErrorCode::BadRequest.status_code(), 400);
        assert_eq!(BeaconErrorCode::NotFound.status_code(), 404);
        assert_eq!(BeaconErrorCode::InternalError.status_code(), 500);
    }

    #[test]
    fn test_beacon_error_display() {
        let err = BeaconError::bad_request("Invalid block ID");
        assert_eq!(err.to_string(), "Bad Request: Invalid block ID");
    }
}

