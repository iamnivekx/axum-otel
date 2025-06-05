use axum::http::HeaderMap;
use opentelemetry::Context;

/// Set the parent span for the current span and record the trace id.
///
/// The implementation is based on the approach used in
/// operator-rs:
/// https://github.com/stackabletech/operator-rs/blob/1b610a8a8e040889ba9a73e062b1d058f1ad590b/crates/stackable-telemetry/src/instrumentation/axum/mod.rs#L459-L473
/// and svix-webhooks:
/// https://github.com/svix/svix-webhooks/blob/d33de2e49e9e8f2cf876023a9c9726d832b7d890/server/svix-server/src/core/otel_spans.rs#L60-L67
///
/// This function extracts the OpenTelemetry context from HTTP headers and sets it as the parent
/// span for the current span. It also records the trace ID for better distributed tracing visualization.
///
/// # Arguments
///
/// * `headers` - The HTTP headers containing the trace context
/// * `span` - The current tracing span to set the parent for
///
/// # Behavior
///
/// 1. Extracts the remote context from HTTP headers using OpenTelemetry's text map propagator
/// 2. If a valid remote span context exists:
///    - Uses the remote span's trace ID
///    - Sets the remote context as the parent span
/// 3. If no valid remote span context exists:
///    - Uses the current span's trace ID
/// 4. Records the trace ID in the span for logging purposes
///
/// # Example
///
/// ```rust
/// use axum::http::HeaderMap;
/// use tracing::Span;
/// use axum_otel::set_otel_parent;
///
/// let headers = HeaderMap::new();
/// let span = Span::current();
/// set_otel_parent(&headers, &span);
/// ```
pub fn set_otel_parent(headers: &HeaderMap, span: &tracing::Span) {
    use opentelemetry::trace::TraceContextExt as _;
    use tracing_opentelemetry::OpenTelemetrySpanExt as _;

    let remote_context = opentelemetry::global::get_text_map_propagator(|propagator| {
        propagator.extract(&opentelemetry_http::HeaderExtractor(headers))
    });

    // If we have a remote parent span, this will be the parent's trace identifier.
    // If not, it will be the newly generated trace identifier with this request as root span.
    let remote_span = remote_context.span();
    let remote_span_context = remote_span.span_context();
    let trace_id = if remote_span_context.is_valid() {
        remote_span_context.trace_id().to_string()
    } else {
        span.context().span().span_context().trace_id().to_string()
    };
    span.set_parent(remote_context);
    span.record("trace_id", tracing::field::display(trace_id));
}

#[cfg(test)]
mod tests {
    use super::*;
    use opentelemetry::trace::TraceContextExt as _;
    use opentelemetry::trace::TracerProvider as _;
    use opentelemetry::{global, KeyValue};
    use opentelemetry_sdk::propagation::TraceContextPropagator;
    use opentelemetry_sdk::trace::Config;
    use opentelemetry_sdk::trace::Sampler;
    use opentelemetry_sdk::Resource;
    use tracing::{field, Level, Span};
    use tracing_opentelemetry::OpenTelemetrySpanExt as _;
    use tracing_subscriber::layer::SubscriberExt;
    use tracing_subscriber::util::SubscriberInitExt;

    fn init_tracing() {
        // Set up the propagator
        global::set_text_map_propagator(TraceContextPropagator::new());

        let otlp_exporter = opentelemetry_otlp::SpanExporter::builder()
            .with_tonic()
            .build()
            .expect("Failed to build the span exporter");
        let provider = opentelemetry_sdk::trace::SdkTracerProvider::builder()
            .with_batch_exporter(otlp_exporter)
            .with_resource(
                Resource::builder()
                    .with_attribute(KeyValue::new("service.name", env!("CARGO_CRATE_NAME")))
                    .build(),
            )
            .build();
        let tracer = provider.tracer(env!("CARGO_CRATE_NAME"));
        let telemetry = tracing_opentelemetry::layer().with_tracer(tracer);
        tracing_subscriber::registry()
            .with(telemetry)
            .try_init()
            .expect("Failed to initialize tracing subscriber for tests");

        // Create a root span to ensure we have a valid trace context
        let _root_span = tracing::info_span!("root_span").entered();
    }

    fn create_span() -> Span {
        tracing::span!(Level::INFO, "test_span")
    }

    #[tokio::test]
    async fn test_set_otel_parent_without_headers() {
        init_tracing();
        let headers = HeaderMap::new();
        let span = create_span();
        set_otel_parent(&headers, &span);

        // Verify that a new trace ID was generated
        let trace_id = span.context().span().span_context().trace_id().to_string();
        assert!(!trace_id.is_empty(), "Expected a trace ID to be set");
        assert_ne!(
            trace_id, "00000000000000000000000000000000",
            "Expected a non-zero trace ID"
        );
    }

    #[tokio::test]
    async fn test_set_otel_parent_with_invalid_traceparent() {
        init_tracing();
        let mut headers = HeaderMap::new();
        headers.insert("traceparent", "invalid".parse().unwrap());

        let span = create_span();
        set_otel_parent(&headers, &span);

        // Verify that a new trace ID was generated despite invalid header
        let trace_id = span.context().span().span_context().trace_id().to_string();
        assert!(!trace_id.is_empty(), "Expected a trace ID to be set");
        assert_ne!(
            trace_id, "00000000000000000000000000000000",
            "Expected a non-zero trace ID"
        );
    }

    #[tokio::test]
    async fn test_set_otel_parent_with_valid_traceparent() {
        init_tracing();
        let mut headers = HeaderMap::new();
        let expected_trace_id = "4bf92f3577b34da6a3ce929d0e0e4736";
        let traceparent = format!("00-{}-00f067aa0ba902b7-01", expected_trace_id);
        println!("Setting traceparent header: {}", traceparent);
        headers.insert("traceparent", traceparent.parse().unwrap());

        let span = create_span();
        println!(
            "Before set_otel_parent - span trace_id: {}",
            span.context().span().span_context().trace_id().to_string()
        );
        set_otel_parent(&headers, &span);
        println!(
            "After set_otel_parent - span trace_id: {}",
            span.context().span().span_context().trace_id().to_string()
        );

        // Verify that the trace ID from the header was used
        let trace_id = span.context().span().span_context().trace_id().to_string();
        println!("Final trace_id: {}", trace_id);
        println!("Expected trace_id: {}", expected_trace_id);
        assert_eq!(
            trace_id, expected_trace_id,
            "Expected trace ID to match the one from the header"
        );
    }
}
