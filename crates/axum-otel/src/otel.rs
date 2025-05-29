use axum::Request;
use opentelemetry::propagation::Extractor;

pub(crate) fn set_otel_parent(req: &Request, span: &tracing::Span) {
    use opentelemetry::trace::TraceContextExt as _;
    use tracing_opentelemetry::OpenTelemetrySpanExt as _;

    let parent_context = opentelemetry::global::get_text_map_propagator(|propagator| {
        propagator.extract(&RequestHeaderCarrier::new(req.headers()))
    });
    span.set_parent(parent_context);
    let trace_id = span.context().span().span_context().trace_id().to_hex();
    span.record("trace_id", tracing::field::display(trace_id));
}
