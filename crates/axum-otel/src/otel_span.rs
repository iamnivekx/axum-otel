//! Module defining utilities for crating `tracing` spans compatible with OpenTelemetry's
//! conventions.
use axum::{
    extract::{ConnectInfo, MatchedPath},
    http,
};
use opentelemetry::trace::TraceContextExt;
use std::net::SocketAddr;
use tower_http::{
    classify::ServerErrorsFailureClass,
    trace::{MakeSpan, OnFailure, OnResponse},
};
use tracing::field::{Empty, debug};
use tracing_opentelemetry::OpenTelemetrySpanExt;
use uuid::Uuid;

/// An implementor of [`MakeSpan`] which creates `tracing` spans populated with information about
/// the request received by an `axum` web server.
#[derive(Clone, Copy)]
pub struct AxumOtelSpanLayer;

impl<B> MakeSpan<B> for AxumOtelSpanLayer {
    fn make_span(&mut self, request: &http::Request<B>) -> tracing::Span {
        let http_method = request.method().as_str();
        let http_route_opt = request
            .extensions()
            .get::<MatchedPath>()
            .map(|p| p.as_str());

        let span_name = http_route_opt.as_ref().map_or_else(
            || http_method.to_string(),
            |route| format!("{} {}", http_method, route),
        );

        let user_agent = request
            .headers()
            .get(http::header::USER_AGENT)
            .and_then(|header| header.to_str().ok());

        let host = request
            .headers()
            .get(http::header::HOST)
            .and_then(|header| header.to_str().ok());

        let http_route = request
            .extensions()
            .get::<MatchedPath>()
            .map(|p| p.as_str());

        let client_ip = request
            .extensions()
            .get::<ConnectInfo<SocketAddr>>()
            .map(|ConnectInfo(ip)| debug(ip));

        let request_id = request
            .headers()
            .get("x-request-id")
            .and_then(|id| id.to_str().map(ToOwned::to_owned).ok())
            .or_else(|| {
                request
                    .headers()
                    .get("request-id")
                    .and_then(|v| v.to_str().map(ToOwned::to_owned).ok())
            })
            .unwrap_or_else(|| Uuid::new_v4().to_string());

        let remote_context = opentelemetry::global::get_text_map_propagator(|p| {
            p.extract(&opentelemetry_http::HeaderExtractor(request.headers()))
        });
        let remote_span = remote_context.span();
        let span_context = remote_span.span_context();
        let trace_id = span_context
            .is_valid()
            .then(|| span_context.trace_id().to_string());

        let span = tracing::error_span!(
            "HTTP request",
            http.client_ip = client_ip,
            http.versions = ?request.version(),
            http.host = host,
            http.method = ?request.method(),
            http.route = http_route,
            http.scheme = request.uri().scheme().map(debug),
            http.status_code = Empty,
            http.target = request.uri().path_and_query().map(|p| p.as_str()),
            http.user_agent = user_agent,
            otel.kind = "server",
            otel.status_code = Empty,
            request_id,
            trace_id,
            org_id = Empty,
            app_id = Empty,
        );

        span.set_parent(remote_context);

        span
    }
}

#[derive(Clone, Copy, Debug)]
pub struct AxumOtelOnResponseLayer;

impl<B> OnResponse<B> for AxumOtelOnResponseLayer {
    fn on_response(
        self,
        response: &http::Response<B>,
        latency: std::time::Duration,
        span: &tracing::Span,
    ) {
        let status = response.status().as_u16().to_string();
        span.record("http.status_code", tracing::field::display(status));
        span.record("otel.status_code", "OK");

        tracing::debug!(
            "finished processing request latency={} ms status={}",
            latency.as_millis(),
            response.status().as_u16(),
        );
    }
}

#[derive(Clone, Copy, Debug)]
pub struct AxumOtelOnFailure;

impl OnFailure<ServerErrorsFailureClass> for AxumOtelOnFailure {
    fn on_failure(
        &mut self,
        failure_classification: ServerErrorsFailureClass,
        _latency: std::time::Duration,
        span: &tracing::Span,
    ) {
        match failure_classification {
            ServerErrorsFailureClass::StatusCode(status) if status.is_server_error() => {
                span.record("otel.status_code", "ERROR");
            }
            _ => {}
        }
    }
}
