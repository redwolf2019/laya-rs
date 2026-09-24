//! HTTP transport for System One; see docs/compatibility.md §7.
//!
//! Validate readiness, JSON media type, streaming byte limit, then the complete
//! protocol before submitting to the shared scheduler. No request text is logged.
//! Dropping a route future cancels its wait only; CPU work remains scheduler-owned.
//! GET routes also support HEAD. Axum returns empty 404/405 with Allow for a known
//! path's unsupported method. Only POST System One contributes HTTP observations.

use crate::{
    scheduler,
    system_one::{self, ErrorBody, ErrorEnvelope, Limits, RequestError},
};
use axum::{
    Json, Router,
    body::to_bytes,
    extract::{Request, State},
    http::{StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use std::error::Error as _;

#[derive(Clone)]
struct AppState {
    client: scheduler::Client,
    limits: Limits,
}

/// Build routes from fully initialized resources; keep driving the scheduler owner.
pub fn router(client: scheduler::Client, limits: Limits) -> Router {
    Router::new()
        .route("/v1/system-one", post(system_one))
        .route(
            "/healthz",
            get(|| async { Json(serde_json::json!({"status": "ok"})) }),
        )
        .route("/readyz", get(ready))
        .route("/metrics", get(metrics))
        .with_state(AppState { client, limits })
}

async fn metrics(State(state): State<AppState>) -> Response {
    let text = match state.client.metrics().encode(state.client.snapshot()) {
        Ok(text) => text,
        Err(_) => return scheduler::Error::Unavailable.into_response(),
    };
    (
        [(
            header::CONTENT_TYPE,
            "application/openmetrics-text; version=1.0.0; charset=utf-8",
        )],
        text,
    )
        .into_response()
}

async fn system_one(State(state): State<AppState>, request: Request) -> Response {
    let mut observation = crate::metrics::RequestObservation::new(state.client.clone());
    let response = respond(state, request).await;
    observation.outcome = response
        .extensions()
        .get::<ErrorReason>()
        .map_or("success", |reason| reason.0);
    response
}

async fn respond(state: AppState, request: Request) -> Response {
    if !state.client.snapshot().accepting {
        return scheduler::Error::Unavailable.into_response();
    }
    if !json_media_type(&request) {
        return static_error(
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "unsupported_media_type",
            "Unsupported media type",
        );
    }
    let body = match to_bytes(request.into_body(), state.limits.max_body_bytes).await {
        Ok(body) => body,
        Err(error) => {
            return if error
                .source()
                .is_some_and(|source| source.is::<http_body_util::LengthLimitError>())
            {
                RequestError::PayloadTooLarge.into_response()
            } else {
                RequestError::JsonSyntax.into_response()
            };
        }
    };
    let request = match system_one::Request::from_slice(&body, &state.limits) {
        Ok(request) => request,
        Err(error) => return error.into_response(),
    };
    match state.client.system_one(request).await {
        Ok(response) => Json(response).into_response(),
        Err(error) => error.into_response(),
    }
}

fn json_media_type(request: &Request) -> bool {
    request
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| mediatype::MediaType::parse(value).ok())
        .is_some_and(|mime| {
            mime.ty == mediatype::names::APPLICATION
                && mime.subty == mediatype::names::JSON
                && mime.suffix.is_none()
                && mime.params.iter().all(|(name, value)| {
                    *name != mediatype::names::CHARSET
                        || value.unquoted_str().eq_ignore_ascii_case("utf-8")
                })
        })
}

async fn ready(State(state): State<AppState>) -> Response {
    if state.client.snapshot().accepting {
        Json(serde_json::json!({"status": "ready"})).into_response()
    } else {
        scheduler::Error::Unavailable.into_response()
    }
}

fn static_error(status: StatusCode, code: &'static str, message: &'static str) -> Response {
    error_response(
        status.as_u16(),
        ErrorEnvelope {
            error: ErrorBody { code, message },
        },
    )
}

#[derive(Clone, Copy)]
struct ErrorReason(&'static str);

impl IntoResponse for RequestError {
    fn into_response(self) -> Response {
        error_response(self.status(), self.envelope())
    }
}

impl IntoResponse for scheduler::Error {
    fn into_response(self) -> Response {
        error_response(self.status(), self.envelope())
    }
}

fn error_response(status: u16, envelope: ErrorEnvelope) -> Response {
    let reason = ErrorReason(envelope.error.code);
    let mut response = (
        StatusCode::from_u16(status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR),
        Json(envelope),
    )
        .into_response();
    response.extensions_mut().insert(reason);
    response
}

#[cfg(test)]
mod tests;
