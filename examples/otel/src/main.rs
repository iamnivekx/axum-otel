use anyhow::Result;
use axum::extract::Query;
use axum::{routing::get, Router};
use axum_otel::{
    AttributeSelection, // Import for advanced configuration
    AttributeVerbosity,
    AxumOtelOnFailure,
    AxumOtelOnResponse,
    AxumOtelSpanCreator,
    Level,
    config, // To reference token constants for clarity in example
};
use opentelemetry::trace::TracerProvider;
use opentelemetry::{global, KeyValue};
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::{propagation::TraceContextPropagator, Resource};
use opentelemetry_semantic_conventions::resource;
use serde::Deserialize;
use std::sync::LazyLock;
use tokio::net::TcpListener;
use tower::ServiceBuilder;
use tower_http::request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer};
use tower_http::trace::TraceLayer;
use tracing::{debug, info};
use tracing_subscriber::fmt::format::FmtSpan;
use tracing_subscriber::{layer::SubscriberExt, EnvFilter, Registry}; // For Axum server

static RESOURCE: LazyLock<Resource> = LazyLock::new(|| {
    Resource::builder()
        .with_attribute(KeyValue::new(
            resource::SERVICE_NAME,
            env!("CARGO_CRATE_NAME"),
        ))
        .build()
});

#[derive(Deserialize, Debug)]
pub struct HelloQuery {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub offset: Option<usize>,
}

#[tracing::instrument]
async fn hello(q: Query<HelloQuery>) -> &'static str {
    debug!("hello request query: {:?}", q);
    "Hello world!"
}

#[tracing::instrument]
async fn health() -> &'static str {
    "OK"
}

fn init_telemetry() -> opentelemetry_sdk::trace::SdkTracerProvider {
    // Start a new otlp trace pipeline.
    // Spans are exported in batch - recommended setup for a production application.
    global::set_text_map_propagator(TraceContextPropagator::new());
    let otlp_exporter = opentelemetry_otlp::SpanExporter::builder()
        .with_tonic()
        .with_endpoint("http://localhost:4317") // Ensure OTel collector is running at this address
        .build()
        .expect("Failed to build the span exporter");
    let provider = opentelemetry_sdk::trace::SdkTracerProvider::builder()
        .with_batch_exporter(otlp_exporter)
        .with_resource(RESOURCE.clone())
        .build();
    let tracer = provider.tracer(env!("CARGO_CRATE_NAME"));

    // Filter based on level - trace, debug, info, warn, error
    // Tunable via `RUST_LOG` env variable
    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        // axum logs rejections from built-in extractors with the `axum::rejection`
        // target, at `TRACE` level. `axum::rejection=trace` enables showing those events
        format!(
            "{}=trace,axum::rejection=trace,axum_otel=trace",
            env!("CARGO_CRATE_NAME")
        )
        .into()
    });
    // Create a `tracing` layer using the otlp tracer
    let telemetry = tracing_opentelemetry::layer().with_tracer(tracer);
    // Create a `tracing` layer to emit spans as structured logs to stdout
    let formatting_layer = tracing_subscriber::fmt::layer()
        .compact()
        .with_writer(std::io::stdout)
        .with_ansi(true)
        .with_level(true)
        .with_span_events(FmtSpan::NEW | FmtSpan::CLOSE);

    // Combined them all together in a `tracing` subscriber
    let subscriber = Registry::default()
        .with(env_filter)
        .with(telemetry)
        .with(formatting_layer);

    tracing::subscriber::set_global_default(subscriber)
        .expect("Failed to install `tracing` subscriber.");

    provider
}

#[tokio::main]
async fn main() -> Result<()> {
    // Consider changing to anyhow::Result for broader error handling
    let provider = init_telemetry();

    // Setup Axum router and server
    let app = Router::new()
        .route("/hello", get(hello))
        .route("/health", get(health))
        .layer(
            ServiceBuilder::new()
                .layer(SetRequestIdLayer::x_request_id(MakeRequestUuid))
                .layer(
                    TraceLayer::new_for_http()
                        // Example: Configure attribute selection using an Include list.
                        // Only specified tokens (and mandatory ones) will be recorded.
                        .make_span_with(
                            AxumOtelSpanCreator::new()
                                .level(Level::INFO)
                                .attribute_selection(AttributeSelection::Include(vec![
                                    config::TOKEN_HTTP_REQUEST_METHOD.to_string(),
                                    config::TOKEN_URL_PATH.to_string(),
                                    config::TOKEN_USER_AGENT_ORIGINAL.to_string(),
                                    // Note: http.route is mandatory, will be included anyway
                                ])),
                        )
                        .on_response(
                            AxumOtelOnResponse::new()
                                .level(Level::INFO)
                                .attribute_selection(AttributeSelection::Include(vec![
                                    config::TOKEN_HTTP_RESPONSE_STATUS_CODE.to_string(),
                                    config::TOKEN_RESPONSE_TIME_MS.to_string(),
                                    // Note: otel.status_code is mandatory, will be included anyway
                                ])),
                        )
                        .on_failure(AxumOtelOnFailure::new().level(Level::ERROR)),
                )
                .layer(PropagateRequestIdLayer::x_request_id()),
        );

    // Comments explaining AttributeSelection:
    // The example above uses `AttributeSelection::Include` to specify exactly which attributes
    // (identified by their tokens) should be recorded, in addition to a set of always-on
    // mandatory attributes (like request_id, trace_id, http.route, otel.name, otel.kind).
    // For example, with the Include list above, you'd get:
    // - From AxumOtelSpanCreator: http.request.method, url.path, user_agent.original (plus mandatory).
    // - From AxumOtelOnResponse: http.response.status_code, response_time_ms (plus mandatory otel.status_code).
    // Other attributes like `client.address`, `url.query`, `http.response.body.size` would be omitted.

    // Other AttributeSelection options:
    //
    // 1. Exclude List: Record all attributes (Full verbosity) *except* those specified.
    //    Useful for removing a few specific, noisy attributes.
    //    Example:
    //    .attribute_selection(AttributeSelection::Exclude(vec![
    //        config::TOKEN_USER_AGENT_ORIGINAL.to_string(),
    //        config::TOKEN_URL_QUERY.to_string(),
    //    ]))
    //
    // 2. Predefined Levels (simpler):
    //    - `AttributeSelection::Level(AttributeVerbosity::Full)`: Records all defined attributes.
    //      This is the default if `attribute_selection()` or `attribute_verbosity()` isn't called.
    //    - `AttributeSelection::Level(AttributeVerbosity::Basic)`: Records a minimal set of attributes.
    //    Example:
    //    .attribute_selection(AttributeSelection::Level(AttributeVerbosity::Basic))
    //    // or using the convenience method:
    //    // .attribute_verbosity(AttributeVerbosity::Basic)
    //
    // Refer to the library documentation (especially the `config` module) for a full list of
    // available tokens and their corresponding OpenTelemetry attribute keys.

    let listener = TcpListener::bind("127.0.0.1:8080").await?;
    info!("Server is running on http://127.0.0.1:8080 (ensure OTLP collector is at http://localhost:4317)");
    axum::serve(listener, app.into_make_service()).await?;

    // Ensure all spans have been shipped.
    // In a real application, this might be part of a more graceful shutdown sequence.
    provider
        .shutdown()
        .expect("Failed to shutdown tracer provider."); // Use .expect for more context on panic

    Ok(())
}
