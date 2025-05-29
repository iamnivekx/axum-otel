use http::{Request, Response, HeaderMap, Version};
use http::header::USER_AGENT;
use opentelemetry::{trace::TraceContextExt, KeyValue};
use opentelemetry_http::HeaderExtractor;
use std::fmt::{Debug, Display};
use tracing::Span;
use tracing_opentelemetry::OpenTelemetrySpanExt; // For set_parent

// Re-using ConnectInfo and MatchedPath for enriching spans if available in request extensions.
// These are Axum-specific but their usage here will be through extensions, making it optional.
use axum::extract::{ConnectInfo, MatchedPath}; 
use std::net::SocketAddr;

/// Defines the contract for creating and customizing root spans for HTTP requests.
///
/// This trait is used by [`OtelLayer`](crate::middleware::OtelLayer) to delegate the creation and
/// finalization of the root span associated with each incoming request. Implementors
/// of this trait can control the attributes, name, and parent context of the root span.
///
/// [`OtelLayer`]: crate::middleware::OtelLayer
pub trait RootSpanBuilderTrait {
    /// Called when a new request is received, before it is processed by the inner service.
    ///
    /// This method should create and return a new [`tracing::Span`]. This span will become the
    /// root span for the request's trace. Implementations can customize the span's name,
    /// attributes (e.g., HTTP method, path, user agent), and parent context (e.g., by extracting
    /// trace context from request headers).
    ///
    /// # Arguments
    ///
    /// * `request`: A reference to the incoming [`http::Request`].
    /// * `_custom_attributes`: A slice of [`opentelemetry::KeyValue`] pairs that can be used to
    ///   provide additional, user-defined attributes to the span. *Currently unused by `DefaultRootSpanBuilder`.*
    ///
    /// # Returns
    ///
    /// A [`tracing::Span`] that will serve as the root span for this request.
    fn on_request_start<B>(&self, request: &Request<B>, _custom_attributes: &[KeyValue]) -> Span;

    /// Called after the request has been processed by the inner service and a response (or error) is available.
    ///
    /// This method is responsible for recording final attributes on the given `span`, such as
    /// the HTTP status code, error details if an error occurred, and any other relevant
    /// information from the [`http::Response`] or error.
    ///
    /// # Arguments
    ///
    /// * `span`: The [`tracing::Span`] created by `on_request_start`.
    /// * `outcome`: A reference to the [`Result`] of processing the request. This will be `Ok(Response)`
    ///   on success or `Err(E)` if an error occurred. The error type `E` must implement
    ///   [`std::fmt::Display`] and [`std::fmt::Debug`].
    /// * `_custom_attributes`: A slice of [`opentelemetry::KeyValue`] pairs for additional attributes.
    ///   *Currently unused by `DefaultRootSpanBuilder`.*
    fn on_request_end<ResBody, E: Display + Debug>(
        &self,
        span: Span,
        outcome: &Result<Response<ResBody>, E>,
        _custom_attributes: &[KeyValue],
    );
}

/// The default implementation of [`RootSpanBuilderTrait`].
///
/// This builder creates spans with attributes that align with the
/// [OpenTelemetry semantic conventions](https://opentelemetry.io/docs/specs/semconv/)
/// for HTTP server spans. It's suitable for most common use cases.
///
/// # Captured Attributes:
///
/// The following attributes are typically captured (some depend on available HTTP information):
///
/// *   **On Request Start:**
///     *   `otel.kind`: Always "server".
///     *   `trace_id`: The OTel trace ID, extracted from headers or newly generated.
///     *   `http.request.method`: The HTTP request method (e.g., "GET", "POST").
///     *   `url.path`: The path of the request URL (e.g., "/users/123").
///     *   `url.scheme`: The scheme of the request URL (e.g., "http", "https").
///     *   `http.request.header.user_agent`: The User-Agent header, if present.
///     *   `host.name`: The host name from the request URI, if present.
///     *   `client.address`: The client's IP address (requires `ConnectInfo` extension from Axum).
///     *   `http.route`: The matched Axum route template (requires `MatchedPath` extension).
///     *   `http.flavor`: The HTTP protocol version (e.g., "HTTP/1.1").
/// *   **On Request End:**
///     *   `http.response.status_code`: The HTTP status code of the response (e.g., 200, 404).
///     *   `otel.status_code`: "OK" or "ERROR", based on the HTTP status or if an error occurred.
///     *   `error.message`: If an error occurred, the display message of the error.
///     *   `error.stacktrace`: If an error occurred, the debug representation of the error.
///
/// This builder can be used directly with [`OtelLayer`](crate::middleware::OtelLayer) or as a
/// reference for creating custom `RootSpanBuilderTrait` implementations.
///
/// [`OtelLayer`]: crate::middleware::OtelLayer
pub struct DefaultRootSpanBuilder;

impl DefaultRootSpanBuilder {
    /// Creates a new instance of `DefaultRootSpanBuilder`.
    pub fn new() -> Self {
        DefaultRootSpanBuilder
    }
}

impl RootSpanBuilderTrait for DefaultRootSpanBuilder {
    fn on_request_start<B>(&self, request: &Request<B>, _custom_attributes: &[KeyValue]) -> Span {
        let user_agent = request
            .headers()
            .get(USER_AGENT)
            .and_then(|v| v.to_str().ok())
            .map(ToString::to_string);

        let host = request
            .uri()
            .host()
            .map(ToString::to_string);
        
        let http_route = request
            .extensions()
            .get::<MatchedPath>()
            .map(|mp| mp.as_str().to_string());

        let client_ip = request
            .extensions()
            .get::<ConnectInfo<SocketAddr>>()
            .map(|ci| ci.0.ip().to_string());

        // Extract parent context
        let parent_context = opentelemetry::global::get_text_map_propagator(|propagator| {
            propagator.extract(&HeaderExtractor(request.headers()))
        });
        
        let trace_id = parent_context.span().span_context().trace_id();
        let trace_id_str = if trace_id.is_valid() { Some(trace_id.to_string()) } else { None };

        let http_method_str = request.method().as_str().to_string();
        let http_version_str = format!("{:?}", request.version());
        let url_path_str = request.uri().path().to_string();
        let scheme_str = request.uri().scheme_str().map(ToString::to_string);

        // TODO: Add _custom_attributes to the span if they are provided.
        // For now, they are ignored as per instruction.

        let span = tracing::info_span!(
            "HTTP request", // Span name
            "otel.kind" = "server",
            "trace_id" = trace_id_str,
            "http.request.method" = http_method_str,
            "url.path" = url_path_str,
            "url.scheme" = scheme_str,
            "http.request.header.user_agent" = user_agent,
            "host.name" = host,
            "client.address" = client_ip, // If ConnectInfo is available
            "http.route" = http_route, // If MatchedPath is available
            "http.flavor" = http_version_str, // e.g., "HTTP/1.1"
            // Fields to be populated on response end:
            "http.response.status_code" = tracing::field::Empty,
            "otel.status_code" = tracing::field::Empty, // OK or ERROR
            "error.message" = tracing::field::Empty, // If error occurred
            "error.stacktrace" = tracing::field::Empty, // If error occurred
        );

        span.set_parent(parent_context);
        span
    }

    fn on_request_end<ResBody, E: Display + Debug>(
        &self,
        span: Span,
        outcome: &Result<Response<ResBody>, E>,
        _custom_attributes: &[KeyValue],
    ) {
        match outcome {
            Ok(response) => {
                let status = response.status();
                span.record("http.response.status_code", status.as_u16() as i64);
                if status.is_server_error() {
                    span.record("otel.status_code", "ERROR");
                } else {
                    span.record("otel.status_code", "OK");
                }
            }
            Err(e) => {
                // Record error details
                span.record("error.message", e.to_string());
                span.record("error.stacktrace", format!("{:?}", e));
                // For HTTP errors where a response might still be partially formed or an HTTP status code is relevant
                // we might not have a response object. If the error implies a status code, record it.
                // This part is tricky as 'E' is generic. If 'E' is an axum::Error, it might have a status_code.
                // For now, we assume no HTTP status code from a generic error 'E'.
                // If a specific error type in practice carries a status, that logic would go here.
                span.record("otel.status_code", "ERROR"); 
            }
        }
        // TODO: Add _custom_attributes to the span if they are provided.
    }
}

// Kept for reference or if old TracingLogger path needs it temporarily.
// Not used by DefaultRootSpanBuilder directly.
fn handle_error_legacy(span: Span, status_code: http::StatusCode, response_error: &dyn std::error::Error) {
    let display = format!("{}", response_error);
    let debug = format!("{:?}", response_error);
    span.record("exception.message", tracing::field::display(display));
    span.record("exception.stacktrace", tracing::field::display(debug));
    let code: i64 = status_code.as_u16().into();

    span.record("http.status_code", code); // Legacy field name

    if status_code.is_client_error() {
        span.record("otel.status_code", "OK");
    } else {
        span.record("otel.status_code", "ERROR");
    }
}
