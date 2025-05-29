#![deny(unsafe_code)]
#![warn(
    missing_docs,
    missing_debug_implementations,
    missing_copy_implementations,
    trivial_casts,
    trivial_numeric_casts,
    unused_import_braces,
    unused_qualifications
)]
#![doc(html_root_url = "https://docs.rs/axum-otel/0.1.0")] // TODO: Update version
// Removed: #![doc = include_str!("../README.md")] 

//! # axum-otel: OpenTelemetry Tracing for Axum Web Framework
//!
//! `axum-otel` provides a Tower [`Layer`](tower_layer::Layer) to automatically create
//! OpenTelemetry traces for requests handled by the [Axum](https://github.com/tokio-rs/axum)
//! web framework.
//!
//! This crate aims to simplify the integration of distributed tracing into Axum applications,
//! providing sensible defaults while allowing customization.
//!
//! ## Features
//!
//! *   **Automatic Trace Creation:** Generates a root span for each incoming HTTP request using `tower-http`'s `TraceLayer`.
//! *   **Context Propagation:** Correctly propagates OpenTelemetry context.
//! *   **Customizable Span Lifecycle:** Provides hooks for customizing span creation (`MakeSpan`), response handling (`OnResponse`), and failure handling (`OnFailure`).
//! *   **Default Implementations:** Offers `DefaultMakeSpan`, `DefaultOnResponse`, and `DefaultOnFailure` that implement OpenTelemetry semantic conventions for HTTP server spans.
//!
//! ## Getting Started
//!
//! Add `axum-otel` to your `Cargo.toml`:
//!
//! ```toml
//! [dependencies]
//! axum = "0.7" # Or your desired axum version
//! axum-otel = "0.1.0" # Replace with the latest version
//! tokio = { version = "1", features = ["full"] }
//! opentelemetry = { version = "0.22", features = ["rt-tokio"] }
//! opentelemetry_otlp = { version = "0.15", features = ["tonic"] }
//! tracing = "0.1"
//! tracing-opentelemetry = "0.23"
//! tracing-subscriber = { version = "0.3", features = ["env-filter"] }
//! # For a complete tracing setup, you might also include:
//! # tracing-bunyan-formatter = "0.3"
//! ```
//!
//! ## Usage Example
//!
//! The core component is the [`OtelTraceLayer`]. You create it and add it to your Axum router.
//! It uses `DefaultMakeSpan`, `DefaultOnResponse`, and `DefaultOnFailure` by default.
//!
//! ```rust,no_run
//! use axum::{routing::get, Router};
//! use axum_otel::OtelTraceLayer; // Updated main export
//! use opentelemetry::global;
//! use opentelemetry_otlp::WithExportConfig;
//! use opentelemetry_sdk::{propagation::TraceContextPropagator, Resource};
//! use opentelemetry_semantic_conventions as semconv;
//! use std::net::SocketAddr;
//! use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter, Registry};
//!
//! // Simple handler function
//! async fn hello() -> &'static str {
//!     // The `INFO` log event will be recorded within the context of the root span
//!     // created by OtelLayer.
//!     tracing::info!("Processing hello request!");
//!     "Hello, world!"
//! }
//!
//! // Function to initialize OpenTelemetry Rust SDK
//! fn init_telemetry() -> opentelemetry_sdk::trace::SdkTracerProvider {
//!     global::set_text_map_propagator(TraceContextPropagator::new());
//!     let exporter = opentelemetry_otlp::SpanExporter::builder()
//!         .with_tonic()
//!         .with_endpoint("http://localhost:4317") // Default OTLP gRPC endpoint
//!         .build()
//!         .expect("Failed to create OTLP exporter.");
//!
//!     opentelemetry_sdk::trace::SdkTracerProvider::builder()
//!         .with_batch_exporter(exporter)
//!         .with_resource(Resource::new(vec![
//!             semconv::resource::SERVICE_NAME.string("axum-otel-example"),
//!         ]))
//!         .build()
//! }
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     // Initialize OpenTelemetry provider.
//!     let provider = init_telemetry();
//!     let tracer = provider.tracer("axum-otel-example-tracer");
//!
//!     // Setup tracing subscriber.
//!     Registry::default()
//!         .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
//!         .with(tracing_opentelemetry::layer().with_tracer(tracer))
//!         // You can add other layers like a formatting layer for console output:
//!         // .with(tracing_subscriber::fmt::layer())
//!         .init();
//!
//!     // Create an Axum router.
//!     let app = Router::new()
//!         .route("/hello", get(hello))
//!         // Add the OtelTraceLayer.
//!         // This will use DefaultMakeSpan, DefaultOnResponse, and DefaultOnFailure.
//!         .layer(OtelTraceLayer::new());
//!
//!     // Define the address and start the server.
//!     let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
//!     tracing::info!("Listening on {}", addr);
//!
//!     let listener = tokio::net::TcpListener::bind(addr).await?;
//!     axum::serve(listener, app.into_make_service()).await?;
//!
//!     // Shutdown the tracer provider to ensure all spans are flushed.
//!     global::shutdown_tracer_provider();
//!
//!     Ok(())
//! }
//! ```
//!
//! ## Customizing Span Behavior
//!
//! `OtelTraceLayer` is built upon `tower-http::trace::TraceLayer`. You can customize its behavior
//! by providing your own implementations of `MakeSpan`, `OnResponse`, or `OnFailure`.
//!
//! `DefaultMakeSpan`, `DefaultOnResponse`, and `DefaultOnFailure` are provided by this crate
//! and aim to follow OpenTelemetry semantic conventions. You can wrap them or replace them.
//!
//! ### Example: Customizing `MakeSpan`
//!
//! ```rust,no_run
//! use axum_otel::{OtelTraceLayer, DefaultMakeSpan}; // Import relevant components
//! use http::Request;
//! use tower_http::trace::MakeSpan;
//! use tracing::Span;
//!
//! #[derive(Clone, Default)]
//! struct MyMakeSpan {
//!     // You can wrap the default or implement entirely custom logic
//!     inner: DefaultMakeSpan,
//! }
//!
//! impl<B> MakeSpan<B> for MyMakeSpan {
//!     fn make_span(&mut self, request: &Request<B>) -> Span {
//!         let span = self.inner.make_span(request); // Call default logic
//!         // Add custom attributes
//!         span.record("my.custom.makespan.attribute", "some_value");
//!         span
//!     }
//! }
//!
//! // In your Axum setup:
//! // let layer = OtelTraceLayer::new()
//! //     .make_span_with(MyMakeSpan::default());
//! // let app = Router::new().layer(layer);
//! ```
//!
//! Refer to the documentation for `tower_http::trace::TraceLayer` for more details on these traits.
//!
//! ## Deprecated Components
//!
//! The old `TracingLogger` (Axum `Transform`-based) and `OtelLayer` (custom Tower Layer)
//! are now deprecated. Please migrate to `OtelTraceLayer`.
//!
//! For more details on specific components, please refer to their respective documentation:
//! *   [`OtelTraceLayer`]
//! *   [`DefaultMakeSpan`]
//! *   [`DefaultOnResponse`]
//! *   [`DefaultOnFailure`]

mod axum;
mod header_extractor;
mod middleware;
mod root_span;
mod root_span_builder; // File is now empty, contains only comments.

// Exports for the tower-http::trace::TraceLayer based middleware
pub use crate::middleware::OtelTraceLayer;
pub use crate::middleware::DefaultMakeSpan;
pub use crate::middleware::DefaultOnResponse;
pub use crate::middleware::DefaultOnFailure;

// RootSpanBuilderTrait and DefaultRootSpanBuilder are removed.
// pub use crate::root_span_builder::RootSpanBuilderTrait; // Removed
// pub use crate::root_span_builder::DefaultRootSpanBuilder; // Removed

// Deprecated TracingLogger and associated StreamSpan
pub use middleware::{StreamSpan, TracingLogger};
pub use root_span::RootSpan;
// Problematic old export below is removed.
// Re-exporting the `Level` enum since it's used in our `root_span!` macro
pub use tracing::Level;

mod otel;
mod otel_span;
#[doc(hidden)]
pub mod root_span_macro; // Consider if this should be public or removed if not used by new API
