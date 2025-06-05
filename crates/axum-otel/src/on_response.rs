use crate::event_dynamic_lvl;
use axum::http;
use tower_http::trace::OnResponse;
use tracing::Level;

/// An implementor of [`OnResponse`] which records the response status code and latency.
///
/// Original implementation from [tower-http](https://github.com/tower-rs/tower-http/blob/main/tower-http/src/trace/on_response.rs).
///
/// This component adds attributes to the span based on the configured [`AttributeSelection`]
/// strategy, typically related to the HTTP response. See the
/// [crate-level documentation on Attribute Configuration](../index.html#attribute-configuration)
/// for a detailed explanation of tokens, mandatory attributes, and selection strategies
/// (`Level`, `Include`, `Exclude`).
///
/// The selection strategy can be set using the [`.attribute_selection()`](AxumOtelOnResponse::attribute_selection) method,
/// or via the convenience method [`.attribute_verbosity()`](AxumOtelOnResponse::attribute_verbosity) for predefined levels.
/// The default is equivalent to `AttributeSelection::Level(AttributeVerbosity::Full)`.
///
/// ### Basic Attributes (Always Included):
/// - `http.response.status_code`: The HTTP response status code (e.g., 200, 404).
/// - `otel.status_code`: The OpenTelemetry canonical status code (typically "OK" for successful responses handled here).
///
/// ### Full Attributes (Included when `AttributeVerbosity::Full`):
/// - `http.response.body.size`: The `Content-Length` of the response body, if present in headers.
///
/// The default verbosity is `AttributeVerbosity::Full`.
///
/// # Example
///
/// ```rust
/// use axum_otel::{AxumOtelOnResponse, Level};
use crate::{AttributeVerbosity, AttributeSelection}; // Import enums

/// use tower_http::trace::TraceLayer;
///
/// let layer = TraceLayer::new_for_http()
///     .on_response(AxumOtelOnResponse::new().level(Level::INFO));
/// ```
#[derive(Clone, Debug)] // AttributeSelection::Include/Exclude(Vec<String>) means not Copy
pub struct AxumOtelOnResponse {
    level: Level,
    attribute_selection: AttributeSelection,
}

impl Default for AxumOtelOnResponse {
    fn default() -> Self {
        Self {
            level: Level::DEBUG,
            attribute_selection: AttributeSelection::default(),
        }
    }
}

impl AxumOtelOnResponse {
    /// Create a new `DefaultOnResponse`.
    ///
    /// By default, it uses `Level::DEBUG` and `AttributeSelection::Level(AttributeVerbosity::Full)`.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the [`Level`] used for [tracing events].
    ///
    /// Please note that while this will set the level for the tracing events
    /// themselves, it might cause them to lack expected information, like
    /// request method or path. You can address this using
    /// [`AxumOtelOnResponse::level`].
    ///
    /// Defaults to [`Level::DEBUG`].
    ///
    /// [tracing events]: https://docs.rs/tracing/latest/tracing/#events
    /// [`AxumOtelOnResponse::level`]: crate::make_span::AxumOtelSpanCreator::level
    pub fn level(mut self, level: Level) -> Self {
        self.level = level;
        self
    }

    /// Sets the attribute recording strategy to a predefined verbosity level.
    ///
    /// This is a convenience method that sets the `attribute_selection` to
    /// [`AttributeSelection::Level(verbosity)`].
    /// For more advanced control (e.g., include/exclude lists of tokens), use the
    /// [`.attribute_selection()`](Self::attribute_selection) method.
    ///
    /// The default behavior (if neither method is called) is equivalent to
    /// `AttributeVerbosity::Full`.
    pub fn attribute_verbosity(mut self, verbosity: AttributeVerbosity) -> Self {
        self.attribute_selection = AttributeSelection::Level(verbosity);
        self
    }

    /// Sets the attribute selection strategy for attributes recorded by this component.
    ///
    /// This allows for fine-grained control over which attributes are recorded,
    /// using either predefined levels (`AttributeSelection::Level`), or include/exclude lists
    /// based on attribute tokens. See [`AttributeSelection`] for more details.
    ///
    /// The default is `AttributeSelection::Level(AttributeVerbosity::Full)`.
    ///
    /// # Example
    /// ```rust
    /// # use axum_otel::{AxumOtelOnResponse, AttributeSelection, AttributeVerbosity, config, Level};
    /// let on_response_basic = AxumOtelOnResponse::new()
    ///     .attribute_selection(AttributeSelection::Level(AttributeVerbosity::Basic));
    ///
    /// let on_response_include = AxumOtelOnResponse::new()
    ///     .attribute_selection(AttributeSelection::Include(vec![
    ///         config::TOKEN_HTTP_RESPONSE_BODY_SIZE.to_string(),
    ///         config::TOKEN_RESPONSE_TIME_MS.to_string(),
    ///     ]));
    /// ```
    pub fn attribute_selection(mut self, selection: AttributeSelection) -> Self {
        self.attribute_selection = selection;
        self
    }
}

impl<B> OnResponse<B> for AxumOtelOnResponse {
    fn on_response(
        self,
        response: &http::Response<B>,
        latency: std::time::Duration,
        span: &tracing::Span,
    ) {
        let status = response.status().as_u16();
use crate::config; // Import config
use std::collections::HashSet; // For include/exclude sets

// ... (AxumOtelOnResponse struct and impl AxumOtelOnResponse block remain the same) ...

impl<B> OnResponse<B> for AxumOtelOnResponse {
    fn on_response(
        self,
        response: &http::Response<B>,
        latency: std::time::Duration,
        span: &tracing::Span,
    ) {
        let status = response.status().as_u16();

        let user_include_set: Option<HashSet<String>> = match &self.attribute_selection {
            AttributeSelection::Include(list) => Some(list.iter().cloned().collect()),
            _ => None,
        };
        let user_exclude_set: Option<HashSet<String>> = match &self.attribute_selection {
            AttributeSelection::Exclude(list) => Some(list.iter().cloned().collect()),
            _ => None,
        };

        // --- Record attributes based on selection strategy ---

        // http.response.status_code (Mandatory)
        if config::should_record_token(config::TOKEN_HTTP_RESPONSE_STATUS_CODE, &self.attribute_selection, user_include_set.as_ref(), user_exclude_set.as_ref()) {
            span.record(config::get_otel_key_for_token(config::TOKEN_HTTP_RESPONSE_STATUS_CODE), status);
        }

        // otel.status_code (Mandatory for success)
        if config::should_record_token(config::TOKEN_OTEL_STATUS_CODE, &self.attribute_selection, user_include_set.as_ref(), user_exclude_set.as_ref()) {
            span.record(config::get_otel_key_for_token(config::TOKEN_OTEL_STATUS_CODE), "OK");
        }

        // http.response.body.size
        if config::should_record_token(config::TOKEN_HTTP_RESPONSE_BODY_SIZE, &self.attribute_selection, user_include_set.as_ref(), user_exclude_set.as_ref()) {
            if let Some(content_length_header) = response.headers().get(http::header::CONTENT_LENGTH) {
                if let Ok(content_length_str) = content_length_header.to_str() {
                    if let Ok(content_length) = content_length_str.parse::<u64>() {
                        span.record(config::get_otel_key_for_token(config::TOKEN_HTTP_RESPONSE_BODY_SIZE), content_length);
                    }
                }
            }
        }

        // response_time_ms (Custom token)
        if config::should_record_token(config::TOKEN_RESPONSE_TIME_MS, &self.attribute_selection, user_include_set.as_ref(), user_exclude_set.as_ref()) {
            span.record(config::TOKEN_RESPONSE_TIME_MS, latency.as_millis() as u64); // Record as u64
        }

        event_dynamic_lvl!(
            self.level,
            latency = %latency.as_millis(),
            status = %status,
            "finished processing request"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::AttributeVerbosity; // Ensure this is imported if not already
    use axum::http::{HeaderMap, Response, StatusCode};
    use std::collections::HashMap;
    use std::time::Duration;
    use tracing::Level;
    use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, Registry};

    // Helper to collect span attributes by creating a test span and applying OnResponse
    fn get_on_response_attributes(
        on_response_config: AxumOtelOnResponse,
        response_status: StatusCode,
        response_headers: HeaderMap,
    ) -> HashMap<String, String> {
        let (collector, handle) = tracing_subscriber::test_collector::TestCollector::new();
        let subscriber = Registry::default().with(collector);
        let mut attributes = HashMap::new();

        tracing::subscriber::with_default(subscriber, || {
            let test_span = tracing::span!(Level::INFO, "test_on_response_span");

            // Simulate what TraceLayer would do: call on_response
            let mut response_builder = Response::builder().status(response_status);
            for (name, value) in response_headers.iter() {
                response_builder = response_builder.header(name, value);
            }
            let response = response_builder.body(()).unwrap(); // Body type doesn't matter for these tests

            on_response_config.on_response(&response, Duration::from_millis(100), &test_span);

            // Use MockVisitor to extract attributes recorded on test_span
            struct MockVisitor<'a>(&'a mut HashMap<String, String>);
            impl<'a> tracing::field::Visit for MockVisitor<'a> {
                fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
                    self.0.insert(field.name().to_string(), format!("{:?}", value));
                }
                fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
                    self.0.insert(field.name().to_string(), value.to_string());
                }
                fn record_u64(&mut self, field: &tracing::field::Field, value: u64) {
                    self.0.insert(field.name().to_string(), value.to_string());
                }
                // Add other record types if needed
            }
            test_span.record(&mut MockVisitor(&mut attributes));
        });

        handle.shutdown();
        attributes
    }

    #[test]
    fn test_on_response_full_verbosity() {
        let mut headers = HeaderMap::new();
        headers.insert(http::header::CONTENT_LENGTH, "12345".parse().unwrap());

        let on_response = AxumOtelOnResponse::new().level(Level::INFO); // Default is Full verbosity
        let attributes = get_on_response_attributes(on_response, StatusCode::OK, headers);

        assert_eq!(
            attributes.get("http.response.status_code").unwrap(),
            "200"
        );
        assert_eq!(attributes.get("otel.status_code").unwrap(), "OK");
        assert_eq!(attributes.get("http.response.body.size").unwrap(), "12345");
    }

    #[test]
    fn test_on_response_basic_verbosity() {
        let mut headers = HeaderMap::new();
        headers.insert(http::header::CONTENT_LENGTH, "12345".parse().unwrap());

        let on_response = AxumOtelOnResponse::new()
            .attribute_verbosity(AttributeVerbosity::Basic)
            .level(Level::INFO);
        let attributes = get_on_response_attributes(on_response, StatusCode::CREATED, headers);

        assert_eq!(
            attributes.get("http.response.status_code").unwrap(),
            "201"
        );
        assert_eq!(attributes.get("otel.status_code").unwrap(), "OK");
        assert!(!attributes.contains_key("http.response.body.size"));
    }

    #[test]
    fn test_on_response_full_verbosity_no_content_length() {
        let headers = HeaderMap::new(); // No content-length header

        let on_response = AxumOtelOnResponse::new().level(Level::INFO); // Default is Full verbosity
        let attributes = get_on_response_attributes(on_response, StatusCode::NO_CONTENT, headers);

        assert_eq!(
            attributes.get("http.response.status_code").unwrap(),
            "204"
        );
        assert_eq!(attributes.get("otel.status_code").unwrap(), "OK");
        assert!(!attributes.contains_key("http.response.body.size")); // Should not be present if header missing
    }
}
