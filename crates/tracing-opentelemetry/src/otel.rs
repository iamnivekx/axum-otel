//! OpenTelemetry integration for tracing.
//!
//! This module provides utilities for initializing and configuring OpenTelemetry
//! tracing and metrics in your application. It includes functions for:
//!
//! - Configuring resource attributes
//! - Initializing tracer and meter providers

use anyhow::{Context, Result};
use opentelemetry::global;
use opentelemetry_sdk::{
    metrics::{MeterProviderBuilder, PeriodicReader, SdkMeterProvider},
    propagation::TraceContextPropagator,
    trace::{RandomIdGenerator, Sampler, SdkTracerProvider},
    Resource,
};

/// Initializes a tracer provider for OpenTelemetry tracing.
///
/// This function sets up a tracer provider with the following features:
/// - Parent-based sampling
/// - Random ID generation
/// - OTLP exporter
/// - Custom resource attributes
///
/// # Arguments
///
/// * `resource` - The OpenTelemetry resource to use
/// * `sample_ratio` - The ratio of traces to sample (0.0 to 1.0)
///
/// # Returns
///
/// Returns a `Result` containing the configured `SdkTracerProvider` or an error
/// if initialization fails.
///
/// # Examples
///
/// ```rust
/// use tracing_opentelemetry_extra::{get_resource, init_tracer_provider};
/// use opentelemetry::KeyValue;
///
/// #[tokio::main]
/// async fn main() -> anyhow::Result<()> {
///     let resource = get_resource("my-service", &[]);
///     let tracer_provider = init_tracer_provider(&resource, 1.0)?;
///     Ok(())
/// }
/// ```
pub fn init_tracer_provider(resource: &Resource, sample_ratio: f64) -> Result<SdkTracerProvider> {
    global::set_text_map_propagator(TraceContextPropagator::new());

    let exporter = opentelemetry_otlp::SpanExporter::builder()
        .with_tonic()
        .build()
        .context("Failed to build OTLP exporter")?;

    let tracer_provider = SdkTracerProvider::builder()
        .with_sampler(Sampler::ParentBased(Box::new(Sampler::TraceIdRatioBased(
            sample_ratio,
        ))))
        .with_id_generator(RandomIdGenerator::default())
        .with_resource(resource.clone())
        .with_batch_exporter(exporter)
        .build();

    global::set_tracer_provider(tracer_provider.clone());

    Ok(tracer_provider)
}

/// Initializes a meter provider for OpenTelemetry metrics.
///
/// This function sets up a meter provider with the following features:
/// - Periodic metric collection
/// - OTLP exporter
/// - Custom resource attributes
///
/// # Arguments
///
/// * `resource` - The OpenTelemetry resource to use
/// * `metrics_interval_secs` - The interval in seconds between metric collections
///
/// # Returns
///
/// Returns a `Result` containing the configured `SdkMeterProvider` or an error
/// if initialization fails.
///
/// # Examples
///
/// ```rust
/// use tracing_opentelemetry_extra::otel::{get_resource, init_meter_provider};
/// use opentelemetry::KeyValue;
///
/// #[tokio::main]
/// async fn main() -> anyhow::Result<()> {
///     let resource = get_resource("my-service", &[]);
///     let meter_provider = init_meter_provider(&resource, 30)?;
///     Ok(())
/// }
/// ```
pub fn init_meter_provider(
    resource: &Resource,
    metrics_interval_secs: u64,
) -> Result<SdkMeterProvider> {
    let exporter = opentelemetry_otlp::MetricExporter::builder()
        .with_tonic()
        .with_temporality(opentelemetry_sdk::metrics::Temporality::default())
        .build()
        .context("Failed to build OTLP exporter")?;

    let reader = PeriodicReader::builder(exporter)
        .with_interval(std::time::Duration::from_secs(metrics_interval_secs))
        .build();

    let meter_builder = MeterProviderBuilder::default()
        .with_resource(resource.clone())
        .with_reader(reader);

    let meter_provider = meter_builder.build();
    global::set_meter_provider(meter_provider.clone());

    Ok(meter_provider)
}
