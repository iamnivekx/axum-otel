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
//! [`AxumOtelOnFailure`]. Attribute recording is customized using the fluent builder API
//! on `AxumOtelSpanCreator` and `AxumOtelOnResponse`.
//!
//! ```rust
//! use axum::{
//!     routing::get,
//!     Router,
//! };
//! use axum_otel::{
//!     AxumOtelOnFailure, AxumOtelOnResponse, AxumOtelSpanCreator, Level,
//!     AttributeVerbosity, // For the simple verbosity preset
//!     config // To reference token constants for fine-grained control
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
//!                     // Example: Start with Basic set and add User-Agent.
//!                     .select_basic_set()
//!                     .with_token(config::TOKEN_USER_AGENT_ORIGINAL)
//!             )
//!             .on_response(
//!                 AxumOtelOnResponse::new()
//!                     .level(Level::INFO)
//!                     // Example: Start with Basic set and add response time.
//!                     .select_basic_set()
//!                     .with_token(config::TOKEN_RESPONSE_TIME_MS)
//!             )
//!             .on_failure(AxumOtelOnFailure::new()), // OnFailure records minimal error info by default
//!     );
//! ```
//!
//! ## Attribute Configuration
//!
//! `axum-otel` offers fine-grained control over telemetry attributes recorded on spans,
//! managed via fluent builder methods on [`AxumOtelSpanCreator`] and [`AxumOtelOnResponse`].
//! This allows precise tailoring of telemetry data, which can be crucial for managing
//! data volume, cost, and signal-to-noise ratio in telemetry backends.
//!
//! The default configuration for both components is to record all available attributes
//! (equivalent to calling `.select_full_set()`).
//!
//! ### Configuration Methods:
//!
//! 1.  **Start with a Predefined Set:**
//!     *   `.select_full_set()`: Selects all recognized attributes for recording. This is the default.
//!     *   `.select_basic_set()`: Selects a predefined "basic" set of attributes, including
//!         [mandatory attributes](#mandatory-attributes) and common essential ones.
//!     *   `.select_none()`: Selects only the [mandatory minimal set](#mandatory-attributes).
//!
//! 2.  **Incrementally Add or Remove Attributes (by Token):**
//!     *   `.with_token(token: &str) -> Self`: Adds an attribute corresponding to the given token.
//!         Panics if the token is not recognized (see [Available Attribute Tokens](#available-attribute-tokens)).
//!     *   `.without_token(token: &str) -> Self`: Removes an attribute corresponding to the given token.
//!         Mandatory attributes cannot be removed; attempts to do so are silently ignored.
//!
//! 3.  **Simple Presets (`attribute_verbosity`)**:
//!     *   `.attribute_verbosity(AttributeVerbosity::Full)`: Convenience for `.select_full_set()`.
//!     *   `.attribute_verbosity(AttributeVerbosity::Basic)`: Convenience for `.select_basic_set()`.
//!
//! **Example Scenarios:**
//!
//! ```rust
//! # use axum_otel::{AxumOtelSpanCreator, Level, AttributeVerbosity, config};
//! // Scenario 1: Start with Basic, add specific tokens
//! let creator1 = AxumOtelSpanCreator::new()
//!     .select_basic_set()
//!     .with_token(config::TOKEN_USER_AGENT_ORIGINAL)
//!     .with_token(config::TOKEN_URL_QUERY);
//!
//! // Scenario 2: Start with Full (default), remove a few noisy tokens
//! let creator2 = AxumOtelSpanCreator::new() // Defaults to Full set
//!     .without_token(config::TOKEN_CLIENT_ADDRESS)
//!     .without_token(config::TOKEN_URL_FULL);
//!
//! // Scenario 3: Start with None (only mandatory), add very specific tokens
//! let creator3 = AxumOtelSpanCreator::new()
//!     .select_none()
//!     .with_token(config::TOKEN_HTTP_REQUEST_METHOD) // Already mandatory
//!     .with_token(config::TOKEN_USER_AGENT_ORIGINAL);
//!
//! // Scenario 4: Simplest way for Basic set
//! let creator4 = AxumOtelSpanCreator::new().attribute_verbosity(AttributeVerbosity::Basic);
//! ```
//!
//! Using these methods helps optimize the telemetry data sent to backends, reducing cost and
//! improving the utility of collected traces.
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
/// for [`AxumOtelSpanCreator`] and [`AxumOtelOnResponse`], and correspond to the token sets
/// `config::BASIC_TOKENS` and `config::ALL_RECOGNIZED_TOKENS` respectively (always including
/// `config::MANDATORY_TOKENS`).
///
/// This enum is used with the `.attribute_verbosity()` convenience method on span configuration
/// components like [`AxumOtelSpanCreator`] and [`AxumOtelOnResponse`], which internally call
/// methods like `.select_full_set()` or `.select_basic_set()`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttributeVerbosity {
    /// Corresponds to the `.select_full_set()` configuration.
    /// Records all available attributes recognized by this crate.
    Full,
    /// Corresponds to the `.select_basic_set()` configuration.
    /// Records a reduced set of essential attributes.
    Basic,
}

// The `AttributeSelection` enum was part of a previous API for attribute configuration.
// The current recommended way to configure attributes is via the fluent builder methods
// on `AxumOtelSpanCreator` and `AxumOtelOnResponse` (e.g., `select_basic_set()`, `with_token()`).
// This enum is kept for potential internal use or future evolution but is not directly
// used in the primary configuration API anymore.
#[derive(Debug, Clone)]
#[doc(hidden)] // Hidden from public docs as it's not for direct user configuration now.
enum AttributeSelection {
    Level(AttributeVerbosity),
    Include(Vec<String>),
    Exclude(Vec<String>),
}

impl Default for AttributeSelection {
    fn default() -> Self {
        AttributeSelection::Level(AttributeVerbosity::Full)
    }
}
