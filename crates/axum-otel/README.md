# axum-otel

A structured logging middleware for Axum web framework that integrates with OpenTelemetry.

## Features

- Structured logging middleware for Axum
- OpenTelemetry integration
- Request tracing
- Metrics collection
- Customizable span attributes

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
axum-otel = "0.29.0"
axum = { version = "0.8", features = ["macros"] }
tower-http = { version = "0.6.5", features = ["trace"] }
opentelemetry = { version = "0.29.0", features = ["metrics"] }
opentelemetry_sdk = { version = "0.29.0", features = ["rt-tokio"] }
opentelemetry-otlp = { version = "0.29.0", features = ["metrics", "grpc-tonic"] }
```

## Quick Start

```rust
use axum::{
    routing::get,
    Router,
};
// Import for attribute control
use axum_otel::{
    AttributeVerbosity, // For simple presets
    AxumOtelOnFailure, AxumOtelOnResponse, AxumOtelSpanCreator,
    config, // For token constants (e.g., config::TOKEN_USER_AGENT_ORIGINAL)
};
use opentelemetry::sdk::trace::Config;
use opentelemetry_otlp::{WithExportConfig, Protocol};
use std::net::SocketAddr;
use tower_http::trace::TraceLayer;
use tracing::Level;

async fn handler() -> &'static str {
    "Hello, world!"
}

#[tokio::main]
async fn main() {
    // Initialize OpenTelemetry
    let tracer = opentelemetry_otlp::new_pipeline()
        .tracing()
        .with_exporter(
            opentelemetry_otlp::new_exporter()
                .tonic()
                .with_endpoint("http://localhost:4317")
                .with_protocol(Protocol::Grpc)
        )
        .with_trace_config(Config::default())
        .install_batch(opentelemetry::runtime::Tokio)
        .expect("Failed to initialize OpenTelemetry");

    // Build our application with a route
    let app = Router::new()
        .route("/", get(handler))
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(
                    AxumOtelSpanCreator::new() // Defaults to .select_full_set()
                        .level(Level::INFO)
                        // Example: Start with basic, then add specific tokens
                        // .select_basic_set()
                        // .with_token(config::TOKEN_USER_AGENT_ORIGINAL)
                )
                .on_response(
                    AxumOtelOnResponse::new() // Defaults to .select_full_set()
                        .level(Level::INFO)
                        // Example: Use basic verbosity for response attributes
                        // .attribute_verbosity(AttributeVerbosity::Basic)
                )
                .on_failure(AxumOtelOnFailure::new()),
        );

    // Run it
    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
    tracing::debug!("listening on {}", addr);
    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await
        .unwrap();
}
```

### Attribute Selection and Control

`axum-otel` provides fine-grained control over telemetry attributes using a fluent builder API on `AxumOtelSpanCreator` and `AxumOtelOnResponse`. This allows you to precisely manage the set of recorded attributes, optimizing data volume and relevance.

**Key Concepts:**
- **Tokens:** Each attribute corresponds to a "token" (a string constant defined in `axum_otel::config`).
- **Selected Set:** Each component maintains a set of tokens for attributes it will record.
- **Mandatory Attributes:** A [minimal set of essential attributes](https_docs.rs/axum-otel/latest/axum_otel/index.html#mandatory-attributes) are always recorded. These cannot be removed by `without_token()`.

**Builder Methods:**

-   `::new()`: Initializes with all recognized tokens selected (Full verbosity).
-   `.select_full_set()`: Selects all recognized tokens.
-   `.select_basic_set()`: Selects mandatory and basic tokens.
-   `.select_none()`: Selects only mandatory tokens.
-   `.with_token(token_str)`: Adds a specific token to the selected set.
-   `.without_token(token_str)`: Removes a specific token (if not mandatory).
-   `.attribute_verbosity(AttributeVerbosity::Basic/Full)`: A simpler way to select predefined Basic or Full sets.

**Configuration Examples:**

1.  **Default (Full Verbosity):**
    ```rust
    # use axum_otel::{AxumOtelSpanCreator, Level};
    let span_creator = AxumOtelSpanCreator::new(); // Records all attributes
    ```

2.  **Basic Set:** For a lean set of attributes.
    ```rust
    # use axum_otel::{AxumOtelSpanCreator, Level};
    let span_creator = AxumOtelSpanCreator::new().select_basic_set();
    // Or using the convenience method:
    // let span_creator = AxumOtelSpanCreator::new().attribute_verbosity(axum_otel::AttributeVerbosity::Basic);
    ```

3.  **Basic Set + Specific Additions:**
    ```rust
    # use axum_otel::{AxumOtelSpanCreator, config, Level};
    let span_creator = AxumOtelSpanCreator::new()
        .select_basic_set()
        .with_token(config::TOKEN_USER_AGENT_ORIGINAL) // Add User-Agent
        .with_token(config::TOKEN_URL_QUERY);        // Add URL Query
    ```

4.  **Full Set - Specific Exclusions:**
    ```rust
    # use axum_otel::{AxumOtelSpanCreator, config, Level};
    let span_creator = AxumOtelSpanCreator::new() // Starts with Full set by default
        .without_token(config::TOKEN_CLIENT_ADDRESS)    // Remove client IP
        .without_token(config::TOKEN_URL_FULL);         // Remove full URL
    ```

5.  **Minimal (Mandatory Only) + Specific Additions:**
    ```rust
    # use axum_otel::{AxumOtelSpanCreator, config, Level};
    let span_creator = AxumOtelSpanCreator::new()
        .select_none() // Start with only mandatory attributes
        .with_token(config::TOKEN_USER_AGENT_ORIGINAL); // Add User-Agent
    ```

**Available Tokens:**
For a complete list of available tokens (like `config::TOKEN_HTTP_REQUEST_METHOD`, `config::TOKEN_RESPONSE_TIME_MS`, etc.) and their descriptions, please refer to the [library documentation's "Attribute Configuration" section](https_docs.rs/axum-otel/latest/axum_otel/index.html#attribute-configuration) and the `axum_otel::config` module.

## Examples

Check out the [examples](https://github.com/iamnivekx/axum-otel/tree/main/examples) directory for more usage examples:

- [Basic OpenTelemetry integration](https://github.com/iamnivekx/axum-otel/tree/main/examples/otel)

## Documentation

For more detailed documentation, visit [docs.rs](https://docs.rs/axum-otel/).

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request.

## License

This project is licensed under either of

 * Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
 * MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option. 