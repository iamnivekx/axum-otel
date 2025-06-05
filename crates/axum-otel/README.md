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
    AttributeSelection, AttributeVerbosity, AxumOtelOnFailure, AxumOtelOnResponse,
    AxumOtelSpanCreator, config, // config for token constants if you want to be explicit
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
                    AxumOtelSpanCreator::new()
                        .level(Level::INFO)
                        // Example: Set to Basic verbosity level.
                        // Default is AttributeSelection::Level(AttributeVerbosity::Full).
                        .attribute_selection(AttributeSelection::Level(AttributeVerbosity::Basic)),
                )
                .on_response(
                    AxumOtelOnResponse::new()
                        .level(Level::INFO)
                        .attribute_selection(AttributeSelection::Level(AttributeVerbosity::Basic)),
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

`axum-otel` offers flexible control over the telemetry attributes recorded on spans via the `AttributeSelection` enum. This allows tailoring the data to specific needs, potentially reducing telemetry volume and associated costs.

The selection is configured on `AxumOtelSpanCreator` and `AxumOtelOnResponse`. It's token-based: each piece of information (e.g., HTTP method) has a corresponding "token" string.

**Selection Strategies:**

1.  **Predefined Levels (`AttributeSelection::Level(AttributeVerbosity)`)**:
    *   `AttributeVerbosity::Full` (Default): Records all recognized attributes.
        ```rust
        # use axum_otel::{AxumOtelSpanCreator, AttributeSelection, AttributeVerbosity, Level};
        # let mut span_creator = AxumOtelSpanCreator::new();
        span_creator.attribute_selection(AttributeSelection::Level(AttributeVerbosity::Full));
        // Or, as it's the default:
        // let span_creator_full = AxumOtelSpanCreator::new();
        // Or using the convenience method:
        // span_creator.attribute_verbosity(AttributeVerbosity::Full);
        ```
    *   `AttributeVerbosity::Basic`: Records a minimal set of essential attributes.
        ```rust
        # use axum_otel::{AxumOtelSpanCreator, AttributeSelection, AttributeVerbosity, Level};
        # let mut span_creator = AxumOtelSpanCreator::new();
        span_creator.attribute_selection(AttributeSelection::Level(AttributeVerbosity::Basic));
        // Or using the convenience method:
        // span_creator.attribute_verbosity(AttributeVerbosity::Basic);
        ```

2.  **Include List (`AttributeSelection::Include(Vec<String>)`)**:
    Records only attributes for the specified tokens, plus a [mandatory minimal set](#mandatory-attributes).
    ```rust
    # use axum_otel::{AxumOtelSpanCreator, AttributeSelection, config, Level};
    # let mut span_creator = AxumOtelSpanCreator::new();
    span_creator.attribute_selection(AttributeSelection::Include(vec![
        config::TOKEN_HTTP_REQUEST_METHOD.to_string(), // "http.request.method"
        config::TOKEN_USER_AGENT_ORIGINAL.to_string(), // "user_agent.original"
        // ... other desired tokens
    ]));
    ```

3.  **Exclude List (`AttributeSelection::Exclude(Vec<String>)`)**:
    Records all attributes from the `Full` set *except* those for the specified tokens. Mandatory attributes are always included.
    ```rust
    # use axum_otel::{AxumOtelSpanCreator, AttributeSelection, config, Level};
    # let mut span_creator = AxumOtelSpanCreator::new();
    span_creator.attribute_selection(AttributeSelection::Exclude(vec![
        config::TOKEN_URL_QUERY.to_string(), // "url.query"
        config::TOKEN_CLIENT_ADDRESS.to_string(), // "client.address"
    ]));
    ```

**Mandatory Attributes:**
A [minimal set of attributes](https://docs.rs/axum-otel/latest/axum_otel/index.html#mandatory-attributes) are always recorded for basic trace utility (e.g., request ID, HTTP method, route).

**Available Tokens:**
For a complete list of available tokens and their descriptions, please refer to the [library documentation (Attribute Configuration section)](https://docs.rs/axum-otel/latest/axum_otel/index.html#attribute-configuration) and the `axum_otel::config` module.

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