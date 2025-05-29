use http::{
    header::{HOST, USER_AGENT},
    HeaderMap, Request, Response, Version,
};
use opentelemetry::{
    trace::{FutureExt, SpanKind, TraceContextExt},
    Context, KeyValue,
};
use opentelemetry_http::HeaderExtractor;
use std::{marker::PhantomData, net::SocketAddr, time::Duration};
use tower_http::{
    classify::ServerErrorsFailureClass,
    trace::{MakeSpan, OnFailure, OnResponse, TraceLayer},
};
use tower_layer::Layer;
use tower_service::Service;
use tracing::{field::{display, debug, Empty}, Span};
use tracing_opentelemetry::OpenTelemetrySpanExt;
use uuid::Uuid;

// Axum specific extractors, used if available in request extensions
use axum::extract::{ConnectInfo, MatchedPath};

// Define Placeholder Trace Components (DefaultMakeSpan will be enhanced)

/// Default [`MakeSpan`] implementation.
///
/// Creates a detailed `tracing::Span` for each request, conforming to OpenTelemetry HTTP conventions.
#[derive(Clone, Default, Debug)]
pub struct DefaultMakeSpan;

impl<B> MakeSpan<B> for DefaultMakeSpan {
    fn make_span(&mut self, request: &Request<B>) -> Span {
        let http_method = request.method().as_str();
        let http_route_opt = request
            .extensions()
            .get::<MatchedPath>()
            .map(|p| p.as_str().to_owned());
        
        let span_name = http_route_opt
            .as_ref()
            .map_or_else(|| http_method.to_string(), |route| format!("{} {}", http_method, route));

        let user_agent = request
            .headers()
            .get(USER_AGENT)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");

        let host_header = request.headers().get(HOST).and_then(|v| v.to_str().ok());
        let (server_address, server_port_i64) = if let Some(host) = host_header {
            let parts: Vec<&str> = host.split(':').collect();
            let address = parts.get(0).unwrap_or(&"").to_string();
            let port_i64 = parts.get(1).and_then(|p_str| p_str.parse::<i64>().ok());
            (Some(address), port_i64)
        } else {
            (None, None)
        };
        
        let client_ip_s = request
            .extensions()
            .get::<ConnectInfo<SocketAddr>>()
            .map(|ConnectInfo(addr)| addr.ip().to_string());
        
        let client_port_i64 = request
            .extensions()
            .get::<ConnectInfo<SocketAddr>>()
            .map(|ConnectInfo(addr)| addr.port() as i64);

        let request_id = request
            .headers()
            .get("X-Request-ID")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string())
            .unwrap_or_else(|| Uuid::new_v4().to_string());

        let parent_cx = opentelemetry::global::get_text_map_propagator(|propagator| {
            propagator.extract(&HeaderExtractor(request.headers()))
        });
        
        let mut span = tracing::info_span!(
            "HTTP request", // Static part of the span name
            "otel.name" = %span_name,
            "otel.kind" = %SpanKind::Server.as_str(), // Correctly use SpanKind::Server
            "http.request.method" = %http_method,
            "network.protocol.version" = ?request.version(), // Use debug for Version
            "url.scheme" = request.uri().scheme_str().unwrap_or(""),
            "url.path" = request.uri().path(),
            "url.query" = request.uri().query().unwrap_or(""),
            "user_agent.original" = %user_agent,
            "server.address" = server_address.as_deref().unwrap_or(""),
            "server.port" = server_port_i64.map_or(Empty, |p| p),
            "client.address" = client_ip_s.as_deref().unwrap_or(""),
            "client.port" = client_port_i64.map_or(Empty, |p| p),
            "http.route" = http_route_opt.as_deref().unwrap_or(""),
            "request_id" = %request_id,
            "trace_id" = Empty, // Will be populated if parent context is valid
            "http.response.status_code" = Empty,
            "otel.status_code" = Empty,
        );

        span.set_parent(parent_cx);

        // Record trace_id from the now-parented span context
        let remote_span_context = span.context().span().span_context();
        if remote_span_context.is_valid() {
            span.record("trace_id", remote_span_context.trace_id().to_string());
        }
        
        span
    }
}

/// Default [`OnResponse`] implementation.
///
/// Records `http.response.status_code`, `otel.status_code` ("OK" or "ERROR"),
/// and `http.server.duration` on the trace span upon receiving a response.
#[derive(Clone, Default, Debug)]
pub struct DefaultOnResponse;

impl<B> OnResponse<B> for DefaultOnResponse {
    fn on_response(self, response: &Response<B>, latency: Duration, span: &Span) {
        let status = response.status();
        span.record("http.response.status_code", status.as_u16() as i64);

        if status.is_success() {
            span.record("otel.status_code", "OK");
        } else if status.is_server_error() {
            span.record("otel.status_code", "ERROR");
        }
        // For 3xx and 4xx, OTel generally doesn't mark as ERROR unless it's also a server-side error.
        // So, we leave otel.status_code as is (potentially unset or "OK" if not a server error).

        span.record("http.server.duration", latency.as_millis() as i64);
    }
}

/// Default [`OnFailure`] implementation.
///
/// Records `otel.status_code` as "ERROR", an `error.message` derived from the
/// `failure_classification`, and `http.server.duration` on the trace span
/// when a request fails.
#[derive(Clone, Default, Debug)]
pub struct DefaultOnFailure;

impl OnFailure<ServerErrorsFailureClass> for DefaultOnFailure {
    fn on_failure(
        &mut self,
        failure_classification: ServerErrorsFailureClass,
        latency: Duration,
        span: &Span,
    ) {
        span.record("otel.status_code", "ERROR");
        span.record(
            "error.message",
            tracing::field::display(format!("{:?}", failure_classification)),
        );
        span.record("http.server.duration", latency.as_millis() as i64);
    }
}

/// A Tower [`Layer`] that wraps services with `tower_http::trace::TraceLayer`
/// configured with OpenTelemetry-specific behaviors.
///
/// This layer provides a convenient way to apply consistent HTTP tracing
/// with OpenTelemetry semantics to Axum applications.
///
/// # Generic Parameters
///
/// *   `MS`: The [`MakeSpan`] implementation, defaulting to [`DefaultMakeSpan`].
/// *   `OR`: The [`OnResponse`] implementation, defaulting to [`DefaultOnResponse`].
/// *   `OF`: The [`OnFailure`] implementation, defaulting to [`DefaultOnFailure`].
#[derive(Clone, Debug)]
pub struct OtelTraceLayer<
    MS = DefaultMakeSpan,
    OR = DefaultOnResponse,
    OF = DefaultOnFailure,
> {
    make_span: MS,
    on_response: OR,
    on_failure: OF,
}

impl OtelTraceLayer<DefaultMakeSpan, DefaultOnResponse, DefaultOnFailure> {
    /// Creates a new `OtelTraceLayer` with default [`MakeSpan`], [`OnResponse`],
    /// and [`OnFailure`] implementations.
    pub fn new() -> Self {
        Self {
            make_span: DefaultMakeSpan::default(),
            on_response: DefaultOnResponse::default(),
            on_failure: DefaultOnFailure::default(),
        }
    }
}

impl Default for OtelTraceLayer<DefaultMakeSpan, DefaultOnResponse, DefaultOnFailure> {
    fn default() -> Self {
        Self::new()
    }
}

impl<S, MS, OR, OF> Layer<S> for OtelTraceLayer<MS, OR, OF>
where
    S: Service<Request<axum::body::Body>> + Clone + Send + 'static, // Assuming axum::body::Body for typical Axum services
    S::Future: Send + 'static,
    MS: MakeSpan<axum::body::Body> + Clone,
    OR: OnResponse<axum::body::Body> + Clone,
    OF: OnFailure<ServerErrorsFailureClass> + Clone,
{
    type Service = tower_http::trace::Trace<S, MS, OR, OF>;

    fn layer(&self, inner: S) -> Self::Service {
        TraceLayer::new_for_http()
            .make_span_with(self.make_span.clone())
            .on_response(self.on_response.clone())
            .on_failure(self.on_failure.clone())
            .layer(inner)
    }
}
