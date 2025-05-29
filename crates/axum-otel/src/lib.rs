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
//! *   **Automatic Trace Creation:** Generates a root span for each incoming HTTP request.
//! *   **Context Propagation:** Correctly propagates OpenTelemetry context, linking client-side
//!     traces with server-side traces.
//! *   **Customizable Span Attributes:** Use the [`RootSpanBuilderTrait`] to define custom
//!     attributes for your root spans based on request and response data.
//! *   **Default Implementation:** Provides a [`DefaultRootSpanBuilder`] that adheres to
//!     OpenTelemetry semantic conventions for HTTP server spans.
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
//! The core component is the [`OtelLayer`]. You create it, typically with the
//! [`DefaultRootSpanBuilder`], and add it to your Axum router.
//!
//! ```rust,no_run
//! use axum::{routing::get, Router};
//! use axum_otel::{OtelLayer, DefaultRootSpanBuilder, RootSpanBuilderTrait}; // Import the trait
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
//!         // Add the OtelLayer.
//!         // OtelLayer::<DefaultRootSpanBuilder> will use the default span creation logic.
//!         .layer(OtelLayer::<DefaultRootSpanBuilder>::new());
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
//! ## Customizing Spans
//!
//! If you need to customize the attributes of the root spans (e.g., add application-specific
//! details from request headers or based on the route), you can implement the
//! [`RootSpanBuilderTrait`].
//!
//! ```rust
//! use axum_otel::{RootSpanBuilderTrait, DefaultRootSpanBuilder};
//! use http::{Request, Response};
//! use opentelemetry::KeyValue;
//! use std::fmt::{Debug, Display};
//! use tracing::Span;
//!
//! #[derive(Clone, Default)] // Default is required by OtelLayer if used as R where R: Default
//! struct MyCustomSpanBuilder;
//!
//! impl RootSpanBuilderTrait for MyCustomSpanBuilder {
//!     fn on_request_start<B>(&self, request: &Request<B>, _custom_attributes: &[KeyValue]) -> Span {
//!         // Use DefaultRootSpanBuilder to get standard attributes, then customize.
//!         let default_builder = DefaultRootSpanBuilder;
//!         let span = default_builder.on_request_start(request, _custom_attributes);
//!
//!         // Add a custom attribute.
//!         span.record("my.custom.attribute", "hello_world");
//!
//!         // You could also extract headers here:
//!         if let Some(custom_header) = request.headers().get("x-custom-header") {
//!             if let Ok(value_str) = custom_header.to_str() {
//!                 span.record("http.request.header.x_custom_header", value_str);
//!             }
//!         }
//!         span
//!     }
//!
//!     fn on_request_end<ResBody, E: Display + Debug>(
//!         &self,
//!         span: Span,
//!         outcome: &Result<Response<ResBody>, E>,
//!         _custom_attributes: &[KeyValue],
//!     ) {
//!         // Call the default implementation first.
//!         let default_builder = DefaultRootSpanBuilder;
//!         default_builder.on_request_end(span.clone(), outcome, _custom_attributes); // Clone span if needed after use
//!
//!         // Add custom logic based on the response or error.
//!         if let Ok(response) = outcome {
//!             if response.status().is_success() {
//!                 span.record("my.custom.status", "successful_request");
//!             }
//!         }
//!     }
//! }
//!
//! // Then, in your Axum setup:
//! // let app = Router::new().layer(OtelLayer::<MyCustomSpanBuilder>::new());
//! ```
//!
//! ## Deprecated `TracingLogger`
//!
//! The previous `TracingLogger` (an Axum `Transform`-based middleware) is now deprecated
//! in favor of the Tower-idiomatic `OtelLayer`. Users are encouraged to migrate.
//!
//! For more details on specific components, please refer to their respective documentation:
//! *   [`OtelLayer`]
//! *   [`RootSpanBuilderTrait`]
//! *   [`DefaultRootSpanBuilder`]

mod axum;
mod header_extractor;
mod middleware;
mod root_span;
mod root_span_builder;

// New Tower-idiomatic exports
pub use crate::middleware::OtelLayer;
pub use crate::root_span_builder::RootSpanBuilderTrait;
pub use crate::root_span_builder::DefaultRootSpanBuilder;

// Existing exports (TracingLogger is now deprecated via attribute in middleware.rs)
pub use middleware::{StreamSpan, TracingLogger};
pub use root_span::RootSpan;
// Problematic old export below is removed.
// Re-exporting the `Level` enum since it's used in our `root_span!` macro
pub use tracing::Level;

mod otel;
mod otel_span;
#[doc(hidden)]
pub mod root_span_macro; // Consider if this should be public or removed if not used by new API
