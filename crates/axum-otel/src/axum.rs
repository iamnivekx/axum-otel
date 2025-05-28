//! # axum-otel
//!
//! Middleware for Axum to enable OpenTelemetry tracing.
//!
//! This crate provides a layer that can be added to your Axum router
//! to automatically trace incoming requests. It extracts trace context
//! from request headers, creates spans, and records relevant HTTP attributes.

use axum::{
    extract::{ConnectInfo, MatchedPath}, // Added ConnectInfo
    http::{HeaderMap, Request, StatusCode, Version}, // Added Version
    response::Response,
};
use futures_util::future::BoxFuture;
use opentelemetry::KeyValue; // Added KeyValue for convenience
use opentelemetry::{
    Context, global,
    propagation::Extractor,
    trace::{SpanKind, StatusCode as OtelStatusCode, TraceContextExt, Tracer},
};
use std::net::SocketAddr; // Added SocketAddr
use std::{
    future::Future,
    pin::Pin,
    sync::Arc, // Added Arc
    task::{self, Poll},
    time::SystemTime,
};
use tower_layer::Layer;
use tower_service::Service;
use tracing_crate as tracing; // Renaming to avoid conflict if `tracing` is also a direct dep
use tracing_opentelemetry::OpenTelemetrySpanExt;
use uuid::Uuid; // Added for request_id

// --- Builder Pattern ---

/// Builder for `OtelLayer`.
#[derive(Clone, Debug, Default)]
pub struct OtelLayerBuilder {
    exclusions: Vec<String>,
}

impl OtelLayerBuilder {
    /// Creates a new `OtelLayerBuilder` with default settings.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a single path to be excluded from tracing.
    /// Paths should be exact matches (e.g., "/health/live").
    pub fn exclude_path(mut self, path: String) -> Self {
        self.exclusions.push(path);
        self
    }

    /// Adds multiple paths to be excluded from tracing.
    /// Paths should be exact matches.
    pub fn exclude_paths(mut self, paths: Vec<String>) -> Self {
        self.exclusions.extend(paths);
        self
    }

    /// Builds the `OtelLayer` with the configured options.
    pub fn build(self) -> OtelLayer {
        OtelLayer {
            exclusions: Arc::new(self.exclusions),
        }
    }
}

// Helper struct for header extraction
struct HeaderExtractor<'a>(&'a HeaderMap);

impl<'a> Extractor for HeaderExtractor<'a> {
    fn get(&self, key: &str) -> Option<&str> {
        self.0.get(key).and_then(|value| value.to_str().ok())
    }

    fn keys(&self) -> Vec<&str> {
        self.0.keys().map(|value| value.as_str()).collect()
    }
}

#[derive(Clone, Debug)]
pub struct OtelLayer {
    exclusions: Arc<Vec<String>>,
}

impl OtelLayer {
    /// Returns a new `OtelLayerBuilder` to construct an `OtelLayer`.
    pub fn builder() -> OtelLayerBuilder {
        OtelLayerBuilder::new()
    }
}

impl<S> Layer<S> for OtelLayer {
    type Service = OtelService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        OtelService {
            inner,
            exclusions: self.exclusions.clone(),
        }
    }
}

#[derive(Clone)]
pub struct OtelService<S> {
    inner: S,
}

impl<S, ReqBody, ResBody> Service<Request<ReqBody>> for OtelService<S>
where
    S: Service<Request<ReqBody>, Response = Response<ResBody>> + Send + 'static,
    S::Future: Send + 'static,
    ReqBody: Send + 'static,
    ResBody: Send + 'static,
{
    type Response = Response<ResBody>;
    type Error = S::Error;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, cx: &mut task::Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: Request<ReqBody>) -> Self::Future {
        // Check for exclusions before any tracing logic
        let path_to_check = req.uri().path();
        if self
            .exclusions
            .iter()
            .any(|excluded_path| excluded_path == path_to_check)
        {
            // If the path is in exclusions, bypass tracing and call inner service directly
            return self.inner.call(req);
        }

        init_tracing(); // Ensure tracing is initialized if not excluded

        let parent_cx = global::get_text_map_propagator(|propagator| {
            propagator.extract(&HeaderExtractor(req.headers()))
        });

        let tracer = global::tracer("axum-otel"); // Get a tracer

        // Initial span name, may be updated later with route
        let method_str = req.method().to_string();
        let mut span_name = format!("HTTP {}", method_str);

        let mut attributes = Vec::new();

        attributes.push(KeyValue::new("http.method", method_str.clone()));
        if let Some(path_and_query) = req.uri().path_and_query() {
            attributes.push(KeyValue::new(
                "http.target",
                path_and_query.as_str().to_string(),
            ));
        }
        attributes.push(KeyValue::new("otel.kind", "server")); // OpenTelemetry specific

        // http.flavor
        let http_flavor = match req.version() {
            Version::HTTP_09 => "0.9",
            Version::HTTP_10 => "1.0",
            Version::HTTP_11 => "1.1",
            Version::HTTP_2 => "2.0",
            Version::HTTP_3 => "3.0",
            _ => "unknown",
        };
        attributes.push(KeyValue::new("http.flavor", http_flavor));

        // http.scheme
        let scheme = req
            .headers()
            .get("X-Forwarded-Proto")
            .and_then(|val| val.to_str().ok())
            .unwrap_or_else(|| req.uri().scheme_str().unwrap_or("http"));
        attributes.push(KeyValue::new("http.scheme", scheme.to_string()));

        // http.host
        if let Some(host) = req
            .headers()
            .get(axum::http::header::HOST)
            .and_then(|val| val.to_str().ok())
        {
            attributes.push(KeyValue::new("http.host", host.to_string()));
        } else if let Some(host) = req.uri().host() {
            attributes.push(KeyValue::new("http.host", host.to_string()));
        }

        // http.user_agent
        if let Some(user_agent) = req
            .headers()
            .get(axum::http::header::USER_AGENT)
            .and_then(|val| val.to_str().ok())
        {
            attributes.push(KeyValue::new("http.user_agent", user_agent.to_string()));
        }

        // Client IP resolution
        let client_ip_from_header = req
            .headers()
            .get("X-Forwarded-For")
            .and_then(|value| {
                value
                    .to_str()
                    .ok()
                    .and_then(|s| s.split(',').next().map(str::trim))
            })
            .or_else(|| {
                req.headers().get("Forwarded").and_then(|value| {
                    value.to_str().ok().and_then(|s| {
                        s.split(';').find_map(|part| {
                            let mut pair = part.trim().splitn(2, '=');
                            if pair.next()? == "for" {
                                pair.next()
                            } else {
                                None
                            }
                        })
                    })
                })
            });

        let mut net_peer_ip_str = None;
        if let Some(connect_info) = req.extensions().get::<ConnectInfo<SocketAddr>>() {
            net_peer_ip_str = Some(connect_info.0.ip().to_string());
            attributes.push(KeyValue::new(
                "net.peer.ip",
                connect_info.0.ip().to_string(),
            ));
            if let Some(port) = connect_info.0.port() {
                attributes.push(KeyValue::new("net.peer.port", port.to_string()));
            }
        } else if let Some(client_ip_hdr) = client_ip_from_header {
            net_peer_ip_str = Some(client_ip_hdr.to_string());
            attributes.push(KeyValue::new("net.peer.ip", client_ip_hdr.to_string()));
        }

        if let Some(client_ip) = client_ip_from_header.or_else(|| net_peer_ip_str.as_deref()) {
            attributes.push(KeyValue::new("http.client_ip", client_ip.to_string()));
        }

        // If MatchedPath is available, update span name and add http.route
        if let Some(matched_path) = req.extensions().get::<MatchedPath>() {
            let route = matched_path.as_str().to_string();
            span_name = format!("HTTP {} {}", method_str, route);
            attributes.push(KeyValue::new("http.route", route.clone()));
            // Also update tracing span if needed, though OTel span name is primary
            tracing::Span::current().record("http.route", &route);
        }

        let mut span_builder = tracer.span_builder(span_name);
        span_builder.span_kind = Some(SpanKind::Server);
        span_builder.attributes = Some(attributes);

        let otel_span = tracer.build_with_context(span_builder, &parent_cx);
        let cx = Context::current_with_span(otel_span);

        // Record trace_id and generate request_id within the tracing span's context
        let request_id = Uuid::new_v4().to_string();
        let otel_span_context = cx.span().span_context(); // Now cx refers to the new OTel span
        let otel_trace_id = otel_span_context.trace_id().to_string();

        // This associates the otel trace_id and our request_id with the *tracing* span.
        // The tracing span is created by `#[tracing::instrument]` or implicitly by `OpenTelemetrySpanExt`
        // if this code is within such a span. For a layer, we are typically creating the root OTel span.
        let current_tracing_span = tracing::Span::current();
        current_tracing_span.record("otel.trace_id", &otel_trace_id);
        current_tracing_span.record("request_id", &request_id);
        // Record other new attributes on the tracing span as well for consistency if using tracing collectors
        current_tracing_span.record("http.flavor", &http_flavor);
        current_tracing_span.record("http.scheme", &scheme);
        if let Some(host) = req
            .headers()
            .get(axum::http::header::HOST)
            .and_then(|val| val.to_str().ok())
        {
            current_tracing_span.record("http.host", &host);
        } else if let Some(host) = req.uri().host() {
            current_tracing_span.record("http.host", &host.to_string());
        }
        if let Some(user_agent) = req
            .headers()
            .get(axum::http::header::USER_AGENT)
            .and_then(|val| val.to_str().ok())
        {
            current_tracing_span.record("http.user_agent", &user_agent);
        }
        if let Some(ip) = client_ip_from_header.or_else(|| net_peer_ip_str.as_deref()) {
            current_tracing_span.record("http.client_ip", &ip);
        }
        if let Some(connect_info) = req.extensions().get::<ConnectInfo<SocketAddr>>() {
            current_tracing_span.record("net.peer.ip", &connect_info.0.ip().to_string());
        } else if let Some(client_ip_hdr) = client_ip_from_header {
            current_tracing_span.record("net.peer.ip", &client_ip_hdr);
        }

        let start_time = SystemTime::now();

        // Clone request_id to be moved into the async block for the response header
        let response_request_id = request_id.clone();
        let future = self.inner.call(req);

        Box::pin(async move {
            let mut response_result = future.await;
            let duration = start_time.elapsed().map_or(0.0, |d| d.as_secs_f64());

            let otel_span = cx.span(); // Get the OpenTelemetry span from the context

            match &mut response_result {
                Ok(response) => {
                    let status_code = response.status();
                    otel_span.set_attribute(KeyValue::new(
                        "http.status_code",
                        status_code.as_u16().to_string(),
                    ));
                    if status_code.is_success() {
                        otel_span.set_status(OtelStatusCode::Ok, "Success".to_string());
                    } else {
                        otel_span.set_status(
                            OtelStatusCode::Error,
                            format!("HTTP error: {}", status_code),
                        );
                        if status_code.is_server_error() {
                            // 500-599
                            otel_span.set_attribute(KeyValue::new("error", "true"));
                        }
                    }
                    // Add x-request-id header
                    response.headers_mut().insert(
                        "x-request-id",
                        response_request_id
                            .parse()
                            .expect("request_id is not a valid header value"),
                    );
                }
                Err(_) => {
                    // Assuming 500 for unhandled errors
                    otel_span.set_attribute(KeyValue::new("http.status_code", "500"));
                    otel_span
                        .set_status(OtelStatusCode::Error, "Internal Server Error".to_string());
                    otel_span.set_attribute(KeyValue::new("error", "true")); // Error attribute for S::Error case
                }
            }

            otel_span.set_attribute(KeyValue::new("otel.duration_secs", duration.to_string()));
            otel_span.end(); // End the OpenTelemetry span

            response_result
        })
    }
}

/// Returns an instance of `OtelLayer` with default settings (no exclusions).
/// To configure exclusions, use `OtelLayer::builder()`.
pub fn init_otel_layer() -> OtelLayer {
    OtelLayer::builder().build()
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{Router, routing::get};
    use opentelemetry::trace::{Span, SpanId, TraceError, TracerProvider as _};
    use opentelemetry_sdk::{
        testing::logs::InMemoryExporter,
        trace::{self as sdktrace, Sampler, TracerProvider as SdkTracerProvider, config},
    };
    use std::sync::Mutex;
    use tokio::net::TcpListener;
    use tower::ServiceExt;

    // Helper to setup a test tracer and return an InMemoryExporter to check spans
    fn setup_test_tracer() -> InMemoryExporter {
        let exporter = InMemoryExporter::default();
        let provider = SdkTracerProvider::builder()
            .with_simple_exporter(exporter.clone())
            .with_config(sdktrace::config().with_sampler(Sampler::AlwaysOn))
            .build();
        global::set_tracer_provider(provider);
        exporter
    }

    async fn simple_handler() -> &'static str {
        "Hello, world!"
    }

    async fn health_handler() -> &'static str {
        "Healthy"
    }

    #[tokio::test]
    async fn test_otel_layer_traces_by_default() {
        let exporter = setup_test_tracer();

        let app = Router::new()
            .route("/test", get(simple_handler))
            .layer(OtelLayer::builder().build()); // Using builder

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, app.into_make_service())
                .await
                .unwrap();
        });

        let client = reqwest::Client::new();
        let _res = client
            .get(format!("http://{}/test", addr))
            .send()
            .await
            .unwrap();

        let provider = global::tracer_provider();
        provider.force_flush(); // Ensure spans are flushed to exporter

        let spans = exporter.get_finished_spans().unwrap();
        assert_eq!(spans.len(), 1, "Expected one span for /test route");
        assert_eq!(spans[0].name, "HTTP GET /test");
    }

    #[tokio::test]
    async fn test_otel_layer_excludes_path() {
        let exporter = setup_test_tracer();

        let layer = OtelLayer::builder()
            .exclude_path("/health".to_string())
            .build();

        let app = Router::new()
            .route("/test", get(simple_handler))
            .route("/health", get(health_handler))
            .layer(layer);

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, app.into_make_service())
                .await
                .unwrap();
        });

        let client = reqwest::Client::new();
        // Request to non-excluded path
        let _res_test = client
            .get(format!("http://{}/test", addr))
            .send()
            .await
            .unwrap();
        // Request to excluded path
        let _res_health = client
            .get(format!("http://{}/health", addr))
            .send()
            .await
            .unwrap();

        let provider = global::tracer_provider();
        provider.force_flush();

        let spans = exporter.get_finished_spans().unwrap();

        // Debugging: Print all spans received
        // for span_data in &spans {
        //     println!("Span: {}, TraceID: {}, SpanID: {}", span_data.name, span_data.span_context.trace_id(), span_data.span_context.span_id());
        // }

        assert_eq!(
            spans.len(),
            1,
            "Expected only one span, /health should be excluded."
        );
        if !spans.is_empty() {
            assert_eq!(
                spans[0].name, "HTTP GET /test",
                "The traced span should be for /test."
            );
        }
    }

    #[tokio::test]
    async fn test_otel_layer_init_otel_layer_default() {
        let exporter = setup_test_tracer();
        // init_tracing(); // Call the actual init_tracing from the lib

        let app = Router::new()
            .route("/default_test", get(simple_handler))
            .layer(init_otel_layer()); // Uses the default constructor

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, app.into_make_service())
                .await
                .unwrap();
        });

        let client = reqwest::Client::new();
        let _res = client
            .get(format!("http://{}/default_test", addr))
            .send()
            .await
            .unwrap();

        global::tracer_provider().force_flush();
        let spans = exporter.get_finished_spans().unwrap();
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].name, "HTTP GET /default_test");
    }
}
