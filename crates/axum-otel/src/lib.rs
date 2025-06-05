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
#![doc(html_root_url = "https://docs.rs/axum-otel/latest")]
#![macro_use]
#![allow(unused_imports)]

//! OpenTelemetry tracing for axum based on tower-http.
//!
//! This crate provides a middleware for Axum web framework that automatically instruments HTTP requests
//! and responses, and adds OpenTelemetry tracing to the request and response spans.
//!
//! ## Features
//!
//! - Automatic request and response tracing
//! - OpenTelemetry integration
//! - Request ID tracking
//! - Customizable span attributes
//! - Configurable attribute verbosity (`Full` or `Basic` sets)
//! - Error tracking
//!
//! ## Usage
//!
//! The primary way to use `axum-otel` is by creating a `tower_http::trace::TraceLayer`
//! and configuring its components: [`AxumOtelSpanCreator`], [`AxumOtelOnResponse`], and
//! [`AxumOtelOnFailure`]. Attribute recording can be customized using [`AttributeSelection`].
//!
//! ```rust
//! use axum::{
//!     routing::get,
//!     Router,
//! };
//! use axum_otel::{
//!     AxumOtelOnFailure, AxumOtelOnResponse, AxumOtelSpanCreator, Level,
//!     AttributeSelection, AttributeVerbosity, // For attribute control
//!     config // To reference token constants for clarity, if desired
//! };
//! use tower_http::trace::TraceLayer;
//!
//! async fn handler() -> &'static str {
//!     "Hello, world!"
//! }
//!
//! // Build our application with a route
//! let app: Router<()> = Router::new()
//!     .route("/", get(handler))
//!     .layer(
//!         TraceLayer::new_for_http()
//!             .make_span_with(
//!                 AxumOtelSpanCreator::new()
//!                     .level(Level::INFO)
//!                     // Example: Using a predefined Basic set of attributes.
//!                     // Default is AttributeSelection::Level(AttributeVerbosity::Full).
//!                     .attribute_selection(AttributeSelection::Level(AttributeVerbosity::Basic))
//!                     // For more examples (Include/Exclude lists), see "Attribute Configuration" below.
//!             )
//!             .on_response(
//!                 AxumOtelOnResponse::new()
//!                     .level(Level::INFO)
//!                     .attribute_selection(AttributeSelection::Level(AttributeVerbosity::Basic))
//!             )
//!             .on_failure(AxumOtelOnFailure::new()), // OnFailure typically records minimal error info
//!     );
//! ```
//!
//! ## Attribute Configuration
//!
//! `axum-otel` provides flexible control over which telemetry attributes are recorded on spans.
//! This is managed by the [`AttributeSelection`] enum, which can be configured on both
//! [`AxumOtelSpanCreator`] and [`AxumOtelOnResponse`].
//!
//! The default configuration is `AttributeSelection::Level(AttributeVerbosity::Full)`, which
//! records all available attributes.
//!
//! ### Strategies:
//!
//! 1.  **Predefined Levels (`AttributeSelection::Level`)**:
//!     *   [`AttributeVerbosity::Full`]: Records all recognized attributes (see full token list below).
//!     *   [`AttributeVerbosity::Basic`]: Records a smaller, essential set of attributes.
//! 2.  **Include Lists (`AttributeSelection::Include(Vec<String>)`)**:
//!     Records only attributes corresponding to the "tokens" in the provided list, plus a
//!     [mandatory minimal set](#mandatory-attributes).
//!     Example: `AttributeSelection::Include(vec![config::TOKEN_HTTP_REQUEST_METHOD.to_string(), config::TOKEN_USER_AGENT_ORIGINAL.to_string()])`
//! 3.  **Exclude Lists (`AttributeSelection::Exclude(Vec<String>)`)**:
//!     Records all attributes from the `Full` set *except* those corresponding to the "tokens"
//!     in the provided list. Mandatory attributes are always included.
//!     Example: `AttributeSelection::Exclude(vec![config::TOKEN_USER_AGENT_ORIGINAL.to_string()])`
//!
//! Using `Basic`, `Include`, or `Exclude` can help reduce telemetry data volume and cost.
//!
//! <span id="mandatory-attributes"></span>
//! ### Mandatory Attributes
//!
//! A minimal set of attributes are always recorded, regardless of the selection strategy,
//! to ensure basic trace utility. These correspond to the following tokens (defined in the [`config`] module):
//! - **`config::TOKEN_OTEL_NAME`**: Span name (e.g., "GET /users/:id").
//! - **`config::TOKEN_OTEL_KIND`**: Span kind (always "SERVER").
//! - **`config::TOKEN_HTTP_REQUEST_METHOD`**: The HTTP request method.
//! - **`config::TOKEN_HTTP_ROUTE`**: The matched Axum route pattern.
//! - **`config::TOKEN_URL_PATH`**: The actual request path.
//! - **`config::TOKEN_HTTP_RESPONSE_STATUS_CODE`**: HTTP response status code (on response/failure).
//! - **`config::TOKEN_OTEL_STATUS_CODE`**: OpenTelemetry span status (OK/ERROR) (on response/failure).
//! - **`config::TOKEN_REQUEST_ID`**: Unique request identifier.
//! - **`config::TOKEN_TRACE_ID`**: Trace ID (if available and part of an active trace, recorded by `set_otel_parent`).
//!
//! <span id="available-attribute-tokens"></span>
//! ### Available Attribute Tokens
//!
//! The following tokens can be used in `Include` and `Exclude` lists. They map to
//! well-known OpenTelemetry semantic conventions where applicable. All tokens are defined as constants
//! in the [`config`] module (e.g., `config::TOKEN_CLIENT_ADDRESS`).
//!
//! **Recorded by `AxumOtelSpanCreator`:**
//! - `config::TOKEN_CLIENT_ADDRESS`: Client's IP address.
//! - `config::TOKEN_HTTP_REQUEST_METHOD`: (Mandatory) HTTP request method.
//! - `config::TOKEN_HTTP_ROUTE`: (Mandatory) Matched Axum route.
//! - `config::TOKEN_NETWORK_PROTOCOL_NAME`: Network protocol name (e.g., "http", "https").
//! - `config::TOKEN_NETWORK_PROTOCOL_VERSION`: HTTP protocol version.
//! - `config::TOKEN_OTEL_KIND`: (Mandatory) Span kind ("SERVER"). Set at span creation.
//! - `config::TOKEN_OTEL_NAME`: (Mandatory) Span name. Set at span creation.
//! - `config::TOKEN_REQUEST_ID`: (Mandatory) Unique request ID.
//! - `config::TOKEN_SERVER_ADDRESS`: Server address from Host header.
//! - `config::TOKEN_SERVER_PORT`: Server port from URI.
//! - `config::TOKEN_TRACE_ID`: (Mandatory) Trace ID. Recorded by `set_otel_parent`.
//! - `config::TOKEN_URL_FULL`: Full reconstructed request URL.
//! - `config::TOKEN_URL_PATH`: (Mandatory) Request path.
//! - `config::TOKEN_URL_QUERY`: URL query parameters.
//! - `config::TOKEN_URL_SCHEME`: URL scheme (e.g., "http", "https").
//! - `config::TOKEN_USER_AGENT_ORIGINAL`: User-Agent header.
//! - *Placeholder for `config::TOKEN_HTTP_RESPONSE_STATUS_CODE`*: (Mandatory) Initialized as empty.
//! - *Placeholder for `config::TOKEN_OTEL_STATUS_CODE`*: (Mandatory) Initialized as empty.
//!
//! **Recorded by `AxumOtelOnResponse`:**
//! - `config::TOKEN_HTTP_RESPONSE_STATUS_CODE`: (Mandatory) HTTP response status code.
//! - `config::TOKEN_OTEL_STATUS_CODE`: (Mandatory) OpenTelemetry span status (usually "OK").
//! - `config::TOKEN_HTTP_RESPONSE_BODY_SIZE`: Response `Content-Length`.
//! - `config::TOKEN_RESPONSE_TIME_MS`: Response processing time in milliseconds.
//!
//! Refer to the [`config`] module for the authoritative list of token constants and their
//! exact string values and mappings to OpenTelemetry keys.
//!
//! ## Components
//!
//! - [`AxumOtelSpanCreator`] - Creates spans for each request with relevant HTTP information
//! - [`AxumOtelOnResponse`] - Records response status and latency
//! - [`AxumOtelOnFailure`] - Handles error cases and updates span status
//!
//! See the [examples](https://github.com/iamnivekx/axum-otel/tree/main/examples) directory for complete examples.

mod event_macro;
mod make_span;
mod on_failure;
mod on_response;
mod otel;
mod request_id;
pub mod config; // Expose the new config module

// re-exports crates library
pub use otel::set_otel_parent;
pub use request_id::get_request_id;

// Exports for the tower-http::trace::TraceLayer based middleware
pub use make_span::AxumOtelSpanCreator;
pub use on_failure::AxumOtelOnFailure;
pub use on_response::AxumOtelOnResponse;

// Re-export the Level enum from tracing crate
pub use tracing::Level;

/// Defines the verbosity level for span attributes recorded by `axum-otel` components.
///
/// This enum allows users to choose between a comprehensive set of attributes (`Full`)
/// or a more limited, essential set (`Basic`). Using `Basic` can help reduce the volume
/// of telemetry data produced, which can be beneficial for performance and cost in
/// high-traffic applications.
///
/// The specific attributes included in `Basic` vs `Full` are detailed in the documentation
/// for [`AxumOtelSpanCreator`] and [`AxumOtelOnResponse`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttributeVerbosity {
    /// Records all available attributes, providing the most detailed telemetry.
    /// This is generally the default.
    Full,
    /// Records a reduced set of essential attributes. For example, for HTTP spans,
    /// this might include method, route, status code, and request ID, but exclude
    /// detailed headers like User-Agent or verbose URL components like query parameters.
    Basic,
}

/// Defines how attributes are selected for recording on spans.
///
/// This enum provides more granular control over attribute recording beyond the
/// simple `Full` or `Basic` levels offered by [`AttributeVerbosity`].
#[derive(Debug, Clone)]
pub enum AttributeSelection {
    /// Select attributes based on a predefined verbosity level.
    Level(AttributeVerbosity),
    /// Include only the attributes whose keys are specified in the list.
    /// All other attributes will be excluded.
    Include(Vec<String>),
    /// Exclude all attributes whose keys are specified in the list.
    /// All other attributes will be included (respecting `AttributeVerbosity::Full` as a baseline).
    Exclude(Vec<String>),
}

impl Default for AttributeSelection {
    fn default() -> Self {
        AttributeSelection::Level(AttributeVerbosity::Full)
    }
}
