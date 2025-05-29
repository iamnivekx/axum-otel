use axum::{routing::get, Router};
use opentelemetry::trace::TracerProvider; // Keep for provider type in init_telemetry
use opentelemetry::{global, KeyValue};
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::{propagation::TraceContextPropagator, Resource};
use opentelemetry_semantic_conventions::resource;
use std::io; // Keep for main return type
use std::sync::LazyLock;
use axum_otel::OtelTraceLayer; // Changed to OtelTraceLayer
use tracing_bunyan_formatter::{BunyanFormattingLayer, JsonStorageLayer};
use tracing_subscriber::{layer::SubscriberExt, EnvFilter, Registry};
use tokio::net::TcpListener; // For Axum server

const APP_NAME: &str = "axum-otel-demo";

static RESOURCE: LazyLock<Resource> = LazyLock::new(|| {
    Resource::builder()
        .with_attribute(KeyValue::new(resource::SERVICE_NAME, APP_NAME))
        .build()
});

async fn hello() -> &'static str {
    "Hello world!"
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
    let tracer = provider.tracer(APP_NAME);

    // Filter based on level - trace, debug, info, warn, error
    // Tunable via `RUST_LOG` env variable
    let env_filter = EnvFilter::try_from_default_env().unwrap_or(EnvFilter::new("info"));
    // Create a `tracing` layer using the otlp tracer
    let telemetry = tracing_opentelemetry::layer().with_tracer(tracer);
    // Create a `tracing` layer to emit spans as structured logs to stdout
    let formatting_layer = BunyanFormattingLayer::new(APP_NAME.into(), std::io::stdout);
    // Combined them all together in a `tracing` subscriber
    let subscriber = Registry::default()
        .with(env_filter)
        .with(telemetry)
        .with(JsonStorageLayer)
        .with(formatting_layer);

    tracing::subscriber::set_global_default(subscriber)
        .expect("Failed to install `tracing` subscriber.");

    provider
}

#[tokio::main]
async fn main() -> io::Result<()> { // Consider changing to anyhow::Result for broader error handling
    let provider = init_telemetry();

    // Setup Axum router and server
    let app = Router::new()
        .route("/hello", get(hello))
        // Apply the OtelTraceLayer with default components.
        .layer(OtelTraceLayer::new());


    let listener = TcpListener::bind("127.0.0.1:8080").await?;
    println!("Listening on http://127.0.0.1:8080/hello");
    axum::serve(listener, app.into_make_service()).await?;

    // Ensure all spans have been shipped.
    // In a real application, this might be part of a more graceful shutdown sequence.
    provider
        .shutdown()
        .expect("Failed to shutdown tracer provider."); // Use .expect for more context on panic

    Ok(())
}
