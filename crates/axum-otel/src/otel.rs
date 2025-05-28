use axum::Request;
use axum::http::header::HeaderMap;
use opentelemetry::propagation::Extractor;

pub(crate) struct RequestHeaderCarrier<'a> {
    headers: &'a HeaderMap,
}

impl<'a> RequestHeaderCarrier<'a> {
    pub(crate) fn new(headers: &'a HeaderMap) -> Self {
        RequestHeaderCarrier { headers }
    }
}

impl Extractor for RequestHeaderCarrier<'_> {
    fn get(&self, key: &str) -> Option<&str> {
        self.headers.get(key).and_then(|v| v.to_str().ok())
    }

    fn keys(&self) -> Vec<&str> {
        self.headers.keys().map(|header| header.as_str()).collect()
    }
}

pub(crate) fn set_otel_parent(req: &ServiceRequest, span: &tracing::Span) {
    use opentelemetry::trace::TraceContextExt as _;
    use tracing_opentelemetry::OpenTelemetrySpanExt as _;

    let parent_context = opentelemetry::global::get_text_map_propagator(|propagator| {
        propagator.extract(&RequestHeaderCarrier::new(req.headers()))
    });
    span.set_parent(parent_context);
    let trace_id = span.context().span().span_context().trace_id().to_hex();
    span.record("trace_id", tracing::field::display(trace_id));
}
