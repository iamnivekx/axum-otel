use crate::{get_request_id, set_otel_parent};
use axum::{
    extract::{ConnectInfo, MatchedPath},
    http,
};
use opentelemetry::trace::SpanKind;
use std::net::SocketAddr;
use tower_http::trace::MakeSpan;
use tracing::{
    field::{debug, Empty},
    Level,
};

/// An implementor of [`MakeSpan`] which creates `tracing` spans populated with information about
/// the request received by an `axum` web server.
///
/// Original implementation from [tower-http](https://github.com/tower-rs/tower-http/blob/main/tower-http/src/trace/make_span.rs).
///
/// This span creator automatically adds attributes to each span. The set of attributes
/// recorded can be customized using the fluent builder methods provided:
/// - Start with a predefined set: [`Self::select_full_set()`] (default),
///   [`Self::select_basic_set()`], or [`Self::select_none()`].
/// - Incrementally add attributes: [`Self::with_token()`].
/// - Incrementally remove attributes: [`Self::without_token()`].
///
/// A simpler way to choose between `Full` and `Basic` predefined sets is via the
/// [`.attribute_verbosity()`](Self::attribute_verbosity) method.
///
/// See the [crate-level documentation on Attribute Configuration](../index.html#attribute-configuration)
/// for a detailed explanation of "tokens", mandatory attributes, and the available sets.
/// The default configuration records all available attributes (`Full` verbosity).
///
/// ### Basic Attributes (Always Included):
/// - `otel.name`: Span name (e.g., "GET /users/:id")
/// - `otel.kind`: Always `SERVER`
/// - `http.request.method`: The HTTP method (e.g., "GET")
/// - `http.route`: The matched Axum route (e.g., "/users/:id")
/// - `url.path`: The actual request path (e.g., "/users/123")
/// - `request_id`: A unique request identifier
/// - `trace_id`: The OpenTelemetry trace ID (populated by `set_otel_parent`)
/// - `http.response.status_code`: Placeholder, filled by `AxumOtelOnResponse` or `AxumOtelOnFailure`.
/// - `otel.status_code`: Placeholder, filled by `AxumOtelOnResponse` or `AxumOtelOnFailure`.
///
/// ### Full Attributes (Included when `AttributeVerbosity::Full`):
/// - `client.address`: Client's IP address.
/// - `network.protocol.version`: HTTP protocol version (e.g., "HTTP/1.1").
/// - `server.address`: Value of the `Host` header.
/// - `url.scheme`: URI scheme (e.g., "http" or "https").
/// - `url.query`: URI query parameters.
/// - `user_agent.original`: `User-Agent` header.
/// - `network.protocol.name`: Network protocol name (e.g., "http" or "https").
/// - `server.port`: Server port extracted from the URI.
/// - `url.full`: The full reconstructed URL.
///
/// The default verbosity is `AttributeVerbosity::Full`.
///
/// # Example
///
/// ```rust
/// use axum_otel::{AxumOtelSpanCreator, Level};
use crate::{AttributeVerbosity, config}; // Removed AttributeSelection, added config
use std::collections::HashSet; // For selected_tokens

/// use tower_http::trace::TraceLayer;
///
/// let layer = TraceLayer::new_for_http()
///     .make_span_with(AxumOtelSpanCreator::new().level(Level::INFO));
/// ```
#[derive(Clone, Debug)]
pub struct AxumOtelSpanCreator {
    level: Level,
    selected_tokens: HashSet<String>,
}

impl AxumOtelSpanCreator {
    /// Create a new `AxumOtelSpanCreator`.
    ///
    /// By default, it uses `Level::TRACE` and selects all recognized attributes for recording
    /// (equivalent to `AttributeVerbosity::Full`).
    pub fn new() -> Self {
        Self {
            level: Level::TRACE,
            selected_tokens: config::ALL_RECOGNIZED_TOKENS.iter().map(|s| s.to_string()).collect(),
        }
    }

    /// Set the [`Level`] used for [tracing events].
    ///
    /// Defaults to [`Level::TRACE`].
    ///
    /// [tracing events]: https://docs.rs/tracing/latest/tracing/#events
    pub fn level(mut self, level: Level) -> Self {
        self.level = level;
        self
    }

    /// Sets the attribute recording strategy to a predefined verbosity level.
    ///
    /// This is a convenience method that internally calls [`.select_full_set()`](Self::select_full_set)
    /// or [`.select_basic_set()`](Self::select_basic_set).
    /// For more fine-grained control, use the `select_*`, `with_token`, and `without_token` methods directly.
    ///
    /// The default behavior (if this method is not called) is equivalent to `AttributeVerbosity::Full`.
    pub fn attribute_verbosity(mut self, verbosity: AttributeVerbosity) -> Self {
        match verbosity {
            AttributeVerbosity::Full => self = self.select_full_set(),
            AttributeVerbosity::Basic => self = self.select_basic_set(),
        }
        self
    }

    /// Configures the component to record the "basic" set of attributes.
    ///
    /// This set includes [mandatory attributes](crate::index.html#mandatory-attributes) plus a minimal
    /// selection of common attributes defined by `config::BASIC_TOKENS`.
    /// It clears any previously selected, added, or removed tokens.
    pub fn select_basic_set(mut self) -> Self {
        self.selected_tokens.clear();
        self.selected_tokens.extend(config::MANDATORY_TOKENS.iter().map(|s| s.to_string()));
        self.selected_tokens.extend(config::BASIC_TOKENS.iter().map(|s| s.to_string()));
        self
    }

    /// Configures the component to record the "full" set of all recognized attributes.
    ///
    /// This set includes all attributes defined in `config::ALL_RECOGNIZED_TOKENS`.
    /// This is the default behavior when an `AxumOtelSpanCreator` is created with `::new()`.
    /// It clears any previously selected, added, or removed tokens.
    pub fn select_full_set(mut self) -> Self {
        self.selected_tokens.clear();
        self.selected_tokens.extend(config::ALL_RECOGNIZED_TOKENS.iter().map(|s| s.to_string()));
        self
    }

    /// Configures the component to record only the [mandatory minimal set of attributes](crate::index.html#mandatory-attributes).
    ///
    /// It clears any previously selected, added, or removed tokens before applying the mandatory set.
    pub fn select_none(mut self) -> Self {
        self.selected_tokens.clear();
        self.selected_tokens.extend(config::MANDATORY_TOKENS.iter().map(|s| s.to_string()));
        self
    }

    /// Adds an individual token to the current set of attributes to be recorded.
    ///
    /// The `token` must be one of the constants defined in the [`config`](crate::config) module
    /// (e.g., `config::TOKEN_USER_AGENT_ORIGINAL`). Adding a token that is already part of the
    /// selected set has no effect. Adding a mandatory token also has no practical effect as
    /// mandatory tokens are always included if not using an empty `Include` list.
    ///
    /// # Panics
    /// Panics if the provided `token` is not found in `config::ALL_RECOGNIZED_TOKENS`.
    pub fn with_token(mut self, token: &str) -> Self {
        assert!(config::ALL_RECOGNIZED_TOKENS.contains(token), "Token '{}' is not a recognized attribute token.", token);
        self.selected_tokens.insert(token.to_string());
        self
    }

    /// Removes an individual token from the current set of attributes to be recorded.
    ///
    /// The `token` must be one of the constants defined in the [`config`](crate::config) module.
    /// [Mandatory attributes](crate::index.html#mandatory-attributes) cannot be removed;
    /// attempts to remove them using this method will be silently ignored.
    /// Removing a token not currently in the selected set has no effect.
    pub fn without_token(mut self, token: &str) -> Self {
        if !config::MANDATORY_TOKENS.contains(token) {
            self.selected_tokens.remove(token);
        }
        self
    }
}

impl Default for AxumOtelSpanCreator {
    fn default() -> Self {
        Self::new()
    }
}

impl<B> MakeSpan<B> for AxumOtelSpanCreator {
    fn make_span(&mut self, request: &http::Request<B>) -> tracing::Span {
        // TODO: Adapt the attribute recording logic later to use self.attribute_selection
        let http_method = request.method().as_str();
        let http_route = request
            .extensions()
            .get::<MatchedPath>()
            .map(|p| p.as_str());

        let user_agent = request
            .headers()
            .get(http::header::USER_AGENT)
            .and_then(|header| header.to_str().ok());

        let host = request
            .headers()
            .get(http::header::HOST)
            .and_then(|header| header.to_str().ok());

        let client_ip = request
            .extensions()
            .get::<ConnectInfo<SocketAddr>>()
            .map(|ConnectInfo(ip)| debug(ip));

        let span_name = http_route.as_ref().map_or_else(
            || http_method.to_string(),
            |route| format!("{} {}", http_method, route),
        );
use crate::config::{self, AttributeSelection, AttributeVerbosity}; // Updated imports
use std::collections::HashSet; // For include/exclude sets
use tracing::field; // For field::debug

// ... (rest of imports remain the same) ...

// ... (AxumOtelSpanCreator struct and impl AxumOtelSpanCreator block remain the same) ...

impl<B> MakeSpan<B> for AxumOtelSpanCreator {
    fn make_span(&mut self, request: &http::Request<B>) -> tracing::Span {
        let http_method_str = request.method().as_str();
        let route_str = request.extensions().get::<MatchedPath>().map(|p| p.as_str());
        let span_name = route_str.map_or_else(
            || http_method_str.to_string(),
            |route| format!("{} {}", http_method_str, route),
        );

        // Create the span with minimal initial fields
        let span = tracing::span!(self.level, %span_name, otel.kind = ?SpanKind::Server);

        // Enter the span to record attributes on it.
        let _enter = span.enter();

        // --- Record attributes based on selected tokens ---

        // Helper closure to avoid repeating the same value extraction logic
        // For &str values
        let mut record_str_if_selected = |token: &str, value_fn: &dyn Fn() -> Option<&str>| {
            if self.selected_tokens.contains(token) {
                if let Some(value) = value_fn() {
                    span.record(config::get_otel_key_for_token(token), value);
                }
            }
        };
        // For Debug values
        let mut record_debug_if_selected = |token: &str, value_fn: &dyn Fn() -> Option<Box<dyn std::fmt::Debug + Send + Sync + 'static>>| {
            if self.selected_tokens.contains(token) {
                if let Some(value) = value_fn() {
                    span.record(config::get_otel_key_for_token(token), field::debug(value));
                }
            }
        };
         // For u64 values
        let mut record_u64_if_selected = |token: &str, value_fn: &dyn Fn() -> Option<u64>| {
            if self.selected_tokens.contains(token) {
                if let Some(value) = value_fn() {
                    span.record(config::get_otel_key_for_token(token), value);
                }
            }
        };

        record_str_if_selected(config::TOKEN_HTTP_REQUEST_METHOD, &|| Some(request.method().as_str()));
        record_str_if_selected(config::TOKEN_HTTP_ROUTE, &|| route_str);
        record_str_if_selected(config::TOKEN_URL_PATH, &|| Some(request.uri().path()));
        record_str_if_selected(config::TOKEN_REQUEST_ID, &|| Some(get_request_id(request.headers())));

        if self.selected_tokens.contains(config::TOKEN_HTTP_RESPONSE_STATUS_CODE) {
            span.record(config::get_otel_key_for_token(config::TOKEN_HTTP_RESPONSE_STATUS_CODE), Empty);
        }
        if self.selected_tokens.contains(config::TOKEN_OTEL_STATUS_CODE) {
            span.record(config::get_otel_key_for_token(config::TOKEN_OTEL_STATUS_CODE), Empty);
        }

        record_debug_if_selected(config::TOKEN_CLIENT_ADDRESS, &|| request.extensions().get::<ConnectInfo<SocketAddr>>().map(|ConnectInfo(ip)| Box::new(*ip) as Box<dyn std::fmt::Debug + Send + Sync + 'static>));
        record_debug_if_selected(config::TOKEN_NETWORK_PROTOCOL_VERSION, &|| Some(Box::new(request.version()) as Box<dyn std::fmt::Debug + Send + Sync + 'static>));

        let host_val = request.headers().get(http::header::HOST).and_then(|h| h.to_str().ok());
        record_str_if_selected(config::TOKEN_SERVER_ADDRESS, &|| host_val);

        let scheme_str_val = request.uri().scheme_str();
        record_str_if_selected(config::TOKEN_URL_SCHEME, &|| scheme_str_val);
        record_str_if_selected(config::TOKEN_NETWORK_PROTOCOL_NAME, &|| Some(scheme_str_val.unwrap_or("http")));
        record_str_if_selected(config::TOKEN_URL_QUERY, &|| request.uri().query());

        record_str_if_selected(config::TOKEN_USER_AGENT_ORIGINAL, &|| request.headers().get(http::header::USER_AGENT).and_then(|ua| ua.to_str().ok()));
        record_u64_if_selected(config::TOKEN_SERVER_PORT, &|| request.uri().port_u16().map(u64::from));

        if self.selected_tokens.contains(config::TOKEN_URL_FULL) {
            if let (Some(scheme), Some(host)) = (scheme_str_val, host_val) {
                let path_and_query = request.uri().path_and_query().map_or("", |pq| pq.as_str());
                span.record(config::get_otel_key_for_token(config::TOKEN_URL_FULL), format!("{}://{}{}", scheme, host, path_and_query).as_str());
            }
        }

        set_otel_parent(request.headers(), &span);
        span
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{
        self, AttributeSelection, AttributeVerbosity, MANDATORY_TOKENS, BASIC_TOKENS, ALL_RECOGNIZED_TOKENS, TOKEN_TO_OTEL_KEY,
        TOKEN_HTTP_RESPONSE_STATUS_CODE, TOKEN_OTEL_STATUS_CODE, TOKEN_HTTP_RESPONSE_BODY_SIZE, TOKEN_RESPONSE_TIME_MS,
        // Import specific tokens needed for assertions by their const name for clarity
        TOKEN_CLIENT_ADDRESS, TOKEN_USER_AGENT_ORIGINAL, TOKEN_URL_FULL, TOKEN_HTTP_REQUEST_METHOD, TOKEN_HTTP_ROUTE, TOKEN_URL_PATH, TOKEN_REQUEST_ID,
        TOKEN_SERVER_PORT, TOKEN_URL_SCHEME, TOKEN_URL_QUERY, TOKEN_NETWORK_PROTOCOL_NAME, TOKEN_NETWORK_PROTOCOL_VERSION, TOKEN_SERVER_ADDRESS,
    };
    use crate::on_response::AxumOtelOnResponse; // To test the full lifecycle
    use axum::{
        body::Body,
        extract::ConnectInfo,
        http::{HeaderMap, Method, Request, StatusCode, Uri, Version, Response},
        routing::get,
        Router,
    };
    use std::collections::{HashMap, HashSet};
    use std::net::SocketAddr;
    use std::time::Duration;
    use tower_http::trace::TraceLayer;
    use tower::ServiceExt;
    use tracing::field::Visit;
    use tracing::Level;
    use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, Registry};

    // --- Test Helpers ---

    fn create_test_request(uri_str: &str, method: Method, headers: HeaderMap, client_ip: Option<SocketAddr>, matched_route_str: &str) -> Request<Body> {
        let uri: Uri = uri_str.parse().unwrap();
        let builder = Request::builder().uri(uri).method(method).version(Version::HTTP_11);
        let builder = headers.into_iter().fold(builder, |b, (name, value)| {
            if let Some(header_name) = name {
                b.header(header_name, value)
            } else {
                b
            }
        });
        let mut req = builder.body(Body::empty()).unwrap();
        if let Some(ip) = client_ip {
            req.extensions_mut().insert(ConnectInfo(ip));
        }
        req.extensions_mut().insert(axum::extract::MatchedPath::new(matched_route_str.to_string()));
        req
    }

    fn create_test_response(status_code: StatusCode, headers: HeaderMap) -> Response<()> {
        let mut builder = Response::builder().status(status_code);
        for (name, value) in headers.iter() {
            builder = builder.header(name, value);
        }
        builder.body(()).unwrap()
    }

    struct MockAttributeVisitor<'a>(pub &'a mut HashMap<String, String>);
    impl<'a> Visit for MockAttributeVisitor<'a> {
        fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
            self.0.insert(field.name().to_string(), format!("{:?}", value));
        }
        fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
            self.0.insert(field.name().to_string(), value.to_string());
        }
        fn record_u64(&mut self, field: &tracing::field::Field, value: u64) {
            self.0.insert(field.name().to_string(), value.to_string());
        }
        fn record_i64(&mut self, field: &tracing::field::Field, value: i64) {
            self.0.insert(field.name().to_string(), value.to_string());
        }
        fn record_bool(&mut self, field: &tracing::field::Field, value: bool) {
            self.0.insert(field.name().to_string(), value.to_string());
        }
        // Empty fields are not recorded by this visitor
        fn record_empty(&mut self, _field: &tracing::field::Field) {}
    }

    fn run_request_cycle_and_collect_attributes(
        creator_selection: AttributeSelection,
        response_selection: AttributeSelection,
        response_content_length: Option<u64>,
    ) -> HashMap<String, String> {
        let (collector, handle) = tracing_subscriber::test_collector::TestCollector::new();
        let subscriber = Registry::default().with(collector);
        let mut attributes = HashMap::new();

        tracing::subscriber::with_default(subscriber, || {
            let mut headers = HeaderMap::new();
            headers.insert(http::header::USER_AGENT, "TestCycleAgent".parse().unwrap());
            headers.insert(http::header::HOST, "testcyclehost.com".parse().unwrap());
            let client_ip: SocketAddr = "127.0.0.1:54321".parse().unwrap();

            let request = create_test_request(
                "https://testcyclehost.com/cycle/path?query=true",
                Method::POST,
                headers.clone(), // clone for request
                Some(client_ip),
                "/cycle/path",
            );

            let mut span_creator = AxumOtelSpanCreator::new()
                .level(Level::INFO)
                .attribute_selection(creator_selection);

            let on_response_handler = AxumOtelOnResponse::new()
                .level(Level::INFO)
                .attribute_selection(response_selection);

            let span = span_creator.make_span(&request);
            let _enter_guard = span.enter(); // Keep span entered for on_response

            // Simulate response
            let mut response_headers = HeaderMap::new();
            if let Some(len) = response_content_length {
                response_headers.insert(http::header::CONTENT_LENGTH, len.to_string().parse().unwrap());
            }
            let response = create_test_response(StatusCode::OK, response_headers);

            on_response_handler.on_response(&response, Duration::from_millis(55), &span);

            span.record(&mut MockAttributeVisitor(&mut attributes));
        });

        handle.shutdown();
        attributes
    }

    fn assert_attributes_presence(
        recorded_attributes: &HashMap<String, String>,
        tokens_to_check: &HashSet<&'static str>,
        should_be_present: bool,
        context: &str,
    ) {
        for token in tokens_to_check {
            let otel_key = config::get_otel_key_for_token(token);
            if should_be_present {
                assert!(recorded_attributes.contains_key(otel_key), "Context: {}. Expected attribute '{}' (token '{}') to be present.", context, otel_key, token);
            } else {
                // Don't assert for Empty fields being absent if they were placeholders.
                // The MockAttributeVisitor doesn't record Empty fields.
                // This check is for fields that would have a value.
                if *token != TOKEN_HTTP_RESPONSE_STATUS_CODE && *token != TOKEN_OTEL_STATUS_CODE { // These are filled by on_response
                     assert!(!recorded_attributes.contains_key(otel_key), "Context: {}. Expected attribute '{}' (token '{}') to be absent. Found: {:?}", context, otel_key, token, recorded_attributes.get(otel_key));
                }
            }
        }
    }

    #[test]
    fn test_level_basic_verbosity() {
        let attributes = run_request_cycle_and_collect_attributes(
            AttributeSelection::Level(AttributeVerbosity::Basic),
            AttributeSelection::Level(AttributeVerbosity::Basic),
            Some(100), // with content length
        );

        let expected_basic_otel_keys: HashSet<&'static str> = BASIC_TOKENS.iter().map(|t| config::get_otel_key_for_token(t)).collect();
        let all_otel_keys: HashSet<&'static str> = ALL_RECOGNIZED_TOKENS.iter().map(|t| config::get_otel_key_for_token(t)).collect();
        let non_basic_otel_keys: HashSet<&'static str> = all_otel_keys.difference(&expected_basic_otel_keys).copied().collect();

        assert_attributes_presence(&attributes, &BASIC_TOKENS, true, "Basic Verbosity - Basic Tokens");

        // Filter out mandatory tokens from non_basic_otel_keys before checking for absence,
        // as mandatory tokens are always present.
        let mut truly_non_basic_tokens_to_check_absence = HashSet::new();
        for token in ALL_RECOGNIZED_TOKENS.iter() {
            if !BASIC_TOKENS.contains(token) && !MANDATORY_TOKENS.contains(token) {
                 // Special case: response_time_ms is not in BASIC_TOKENS by default in config.rs
                if *token == TOKEN_RESPONSE_TIME_MS {
                    if !BASIC_TOKENS.contains(TOKEN_RESPONSE_TIME_MS) { // Double check our assumption for this test
                         truly_non_basic_tokens_to_check_absence.insert(*token);
                    }
                }
                // Special case: http_response_body_size is not in BASIC_TOKENS
                else if *token == TOKEN_HTTP_RESPONSE_BODY_SIZE {
                     if !BASIC_TOKENS.contains(TOKEN_HTTP_RESPONSE_BODY_SIZE) {
                        truly_non_basic_tokens_to_check_absence.insert(*token);
                     }
                }
                else {
                    truly_non_basic_tokens_to_check_absence.insert(*token);
                }
            }
        }
        assert_attributes_presence(&attributes, &truly_non_basic_tokens_to_check_absence, false, "Basic Verbosity - Non-Basic/Non-Mandatory Tokens");

        // Specifically check response_time_ms and http.response.body.size based on Basic definition
        // Assuming TOKEN_RESPONSE_TIME_MS and TOKEN_HTTP_RESPONSE_BODY_SIZE are NOT in config::BASIC_TOKENS
        assert!(!attributes.contains_key(TOKEN_RESPONSE_TIME_MS), "response_time_ms should be absent for Basic");
        assert!(!attributes.contains_key(config::get_otel_key_for_token(TOKEN_HTTP_RESPONSE_BODY_SIZE)), "http.response.body.size should be absent for Basic");
    }

    #[test]
    fn test_level_full_verbosity() {
        let attributes = run_request_cycle_and_collect_attributes(
            AttributeSelection::Level(AttributeVerbosity::Full),
            AttributeSelection::Level(AttributeVerbosity::Full),
            Some(200),
        );
        // In Full, all recognized tokens that can be populated should be.
        // We check a subset of important ones.
        let mut expected_full_tokens = ALL_RECOGNIZED_TOKENS.clone();
        // Empty fields are not recorded by MockAttributeVisitor, so don't check for their presence directly in the map if they remained empty.
        // The logic in make_span records Empty for TOKEN_HTTP_RESPONSE_STATUS_CODE and TOKEN_OTEL_STATUS_CODE initially.
        // These are then overwritten by on_response. So they should be present.

        assert_attributes_presence(&attributes, &expected_full_tokens, true, "Full Verbosity - All Tokens");
        assert!(attributes.contains_key(TOKEN_RESPONSE_TIME_MS), "response_time_ms should be present for Full");
        assert!(attributes.contains_key(config::get_otel_key_for_token(TOKEN_HTTP_RESPONSE_BODY_SIZE)), "http.response.body.size should be present for Full with Content-Length");
    }

    #[test]
    fn test_include_tokens_selection() {
        let include_list = vec![TOKEN_HTTP_REQUEST_METHOD.to_string(), TOKEN_USER_AGENT_ORIGINAL.to_string(), TOKEN_RESPONSE_TIME_MS.to_string()];
        let attributes = run_request_cycle_and_collect_attributes(
            AttributeSelection::Include(include_list.clone()),
            AttributeSelection::Include(include_list), // Same for on_response
            Some(300),
        );

        let mut expected_present = MANDATORY_TOKENS.clone();
        expected_present.insert(TOKEN_HTTP_REQUEST_METHOD); // Already mandatory, but fine
        expected_present.insert(TOKEN_USER_AGENT_ORIGINAL);
        expected_present.insert(TOKEN_RESPONSE_TIME_MS);
        // http.response.status_code and otel.status_code are mandatory and set by on_response
        expected_present.insert(TOKEN_HTTP_RESPONSE_STATUS_CODE);
        expected_present.insert(TOKEN_OTEL_STATUS_CODE);


        assert_attributes_presence(&attributes, &expected_present, true, "Include - Expected Tokens");

        // Check a token that's not mandatory and not in the include list - should be absent
        let mut unexpected_tokens = HashSet::new();
        unexpected_tokens.insert(TOKEN_CLIENT_ADDRESS); // Example of a Full token not included
        unexpected_tokens.insert(TOKEN_URL_FULL);       // Example of a Full token not included
        // Ensure TOKEN_HTTP_RESPONSE_BODY_SIZE is not included unless explicitly listed and it's not here
        unexpected_tokens.insert(TOKEN_HTTP_RESPONSE_BODY_SIZE);

        assert_attributes_presence(&attributes, &unexpected_tokens, false, "Include - Unexpected Tokens");
    }

    #[test]
    fn test_include_empty_list() {
        let attributes = run_request_cycle_and_collect_attributes(
            AttributeSelection::Include(vec![]),
            AttributeSelection::Include(vec![]),
            None,
        );
        // Only mandatory tokens should be present.
        // Note: http.response.status_code and otel.status_code are mandatory and added by on_response.
        assert_attributes_presence(&attributes, &MANDATORY_TOKENS, true, "Include Empty - Mandatory Tokens");

        let mut non_mandatory_tokens = ALL_RECOGNIZED_TOKENS.clone();
        MANDATORY_TOKENS.iter().for_each(|t| { non_mandatory_tokens.remove(t); });

        assert_attributes_presence(&attributes, &non_mandatory_tokens, false, "Include Empty - Non-Mandatory Tokens");
    }

    #[test]
    fn test_exclude_tokens_selection() {
        let exclude_list_tokens = vec![TOKEN_USER_AGENT_ORIGINAL.to_string(), TOKEN_URL_QUERY.to_string(), TOKEN_HTTP_RESPONSE_BODY_SIZE.to_string()];
        let attributes = run_request_cycle_and_collect_attributes(
            AttributeSelection::Exclude(exclude_list_tokens.clone()),
            AttributeSelection::Exclude(exclude_list_tokens),
            Some(400), // Provide content length, but it should be excluded
        );

        let mut excluded_as_set = HashSet::new();
        excluded_as_set.insert(TOKEN_USER_AGENT_ORIGINAL);
        excluded_as_set.insert(TOKEN_URL_QUERY);
        excluded_as_set.insert(TOKEN_HTTP_RESPONSE_BODY_SIZE);

        assert_attributes_presence(&attributes, &excluded_as_set, false, "Exclude - Excluded Tokens");

        // Check that other Full tokens (not mandatory, not excluded) ARE present
        let mut expected_present_due_to_full_minus_exclude = ALL_RECOGNIZED_TOKENS.clone();
        excluded_as_set.iter().for_each(|t| { expected_present_due_to_full_minus_exclude.remove(t); });
        // Mandatory tokens are always there, so this check implicitly covers them too.
        assert_attributes_presence(&attributes, &expected_present_due_to_full_minus_exclude, true, "Exclude - Expected Full minus Excluded");
    }

     #[test]
    fn test_exclude_mandatory_token() {
        // Excluding a mandatory token should still result in it being present.
        let exclude_list = vec![TOKEN_HTTP_REQUEST_METHOD.to_string()]; // Try to exclude a mandatory token
        let attributes = run_request_cycle_and_collect_attributes(
            AttributeSelection::Exclude(exclude_list.clone()),
            AttributeSelection::Exclude(exclude_list),
            None,
        );

        let mut mandatory_to_check = HashSet::new();
        mandatory_to_check.insert(TOKEN_HTTP_REQUEST_METHOD);
        assert_attributes_presence(&attributes, &mandatory_to_check, true, "Exclude Mandatory - Mandatory token should still be present");
    }

    // --- Tests for selected_tokens builder methods ---
    fn assert_sets_equal(selected: &HashSet<String>, expected_phf_set: &phf::Set<&'static str>, context: &str) {
        let expected_hs: HashSet<String> = expected_phf_set.iter().map(|s| s.to_string()).collect();
        assert_eq!(*selected, expected_hs, "Context: {}", context);
    }

    fn assert_sets_equal_with_extra(selected: &HashSet<String>, phf_set1: &phf::Set<&'static str>, phf_set2: Option<&phf::Set<&'static str>>, extras: &[&str], context: &str) {
        let mut expected_hs: HashSet<String> = phf_set1.iter().map(|s| s.to_string()).collect();
        if let Some(set2) = phf_set2 {
            expected_hs.extend(set2.iter().map(|s| s.to_string()));
        }
        for extra in extras {
            expected_hs.insert(extra.to_string());
        }
        assert_eq!(*selected, expected_hs, "Context: {}", context);
    }

    #[test]
    fn span_creator_default_initialization() {
        let creator = AxumOtelSpanCreator::new();
        assert_sets_equal(&creator.selected_tokens, &config::ALL_RECOGNIZED_TOKENS, "Default init should be Full set");
    }

    #[test]
    fn span_creator_select_basic_set() {
        let creator = AxumOtelSpanCreator::new().select_basic_set();
        let mut expected = config::MANDATORY_TOKENS.iter().map(|s|s.to_string()).collect::<HashSet<String>>();
        expected.extend(config::BASIC_TOKENS.iter().map(|s|s.to_string()));
        assert_eq!(creator.selected_tokens, expected, "select_basic_set check");
    }

    #[test]
    fn span_creator_select_full_set() {
        let creator = AxumOtelSpanCreator::new().select_basic_set().select_full_set(); // Start different, then switch
        assert_sets_equal(&creator.selected_tokens, &config::ALL_RECOGNIZED_TOKENS, "select_full_set check");
    }

    #[test]
    fn span_creator_select_none() {
        let creator = AxumOtelSpanCreator::new().select_none();
        assert_sets_equal(&creator.selected_tokens, &config::MANDATORY_TOKENS, "select_none should only have Mandatory tokens");
    }

    #[test]
    fn span_creator_with_token() {
        let creator = AxumOtelSpanCreator::new()
            .select_none()
            .with_token(config::TOKEN_USER_AGENT_ORIGINAL)
            .with_token(config::TOKEN_HTTP_REQUEST_METHOD); // Mandatory, should not change outcome vs select_none

        let mut expected = config::MANDATORY_TOKENS.iter().map(|s|s.to_string()).collect::<HashSet<String>>();
        expected.insert(config::TOKEN_USER_AGENT_ORIGINAL.to_string());

        assert_eq!(creator.selected_tokens, expected, "with_token check");
    }

    #[test]
    #[should_panic(expected = "Token 'unknown_token' is not a recognized attribute token.")]
    fn span_creator_with_unknown_token_panics() {
        AxumOtelSpanCreator::new().with_token("unknown_token");
    }

    #[test]
    fn span_creator_without_token() {
        let creator = AxumOtelSpanCreator::new() // Starts with Full set
            .without_token(config::TOKEN_USER_AGENT_ORIGINAL)
            .without_token(config::TOKEN_HTTP_REQUEST_METHOD); // Attempt to remove mandatory

        assert!(!creator.selected_tokens.contains(config::TOKEN_USER_AGENT_ORIGINAL), "TOKEN_USER_AGENT_ORIGINAL should be removed");
        assert!(creator.selected_tokens.contains(config::TOKEN_HTTP_REQUEST_METHOD), "Mandatory TOKEN_HTTP_REQUEST_METHOD should still be present");

        // Check if other Full set tokens (not mandatory, not TOKEN_USER_AGENT_ORIGINAL) are still there
        assert!(creator.selected_tokens.contains(config::TOKEN_CLIENT_ADDRESS), "Other Full set token TOKEN_CLIENT_ADDRESS should be present");
    }

    #[test]
    fn span_creator_attribute_verbosity_method() {
        let creator_basic = AxumOtelSpanCreator::new().attribute_verbosity(AttributeVerbosity::Basic);
        let mut expected_basic = config::MANDATORY_TOKENS.iter().map(|s|s.to_string()).collect::<HashSet<String>>();
        expected_basic.extend(config::BASIC_TOKENS.iter().map(|s|s.to_string()));
        assert_eq!(creator_basic.selected_tokens, expected_basic, "attribute_verbosity(Basic) check");

        let creator_full = AxumOtelSpanCreator::new().attribute_verbosity(AttributeVerbosity::Full);
        assert_sets_equal(&creator_full.selected_tokens, &config::ALL_RECOGNIZED_TOKENS, "attribute_verbosity(Full) check");
    }

    #[test]
    fn span_creator_chaining_complex() {
        let creator = AxumOtelSpanCreator::new()
            .select_basic_set() // Starts with Basic + Mandatory
            .with_token(config::TOKEN_URL_FULL) // Add one from Full set
            .without_token(config::TOKEN_URL_PATH) // Remove one from Basic (but it's also Mandatory)
            .with_token(config::TOKEN_SERVER_PORT); // Add another from Full set

        let mut expected = config::MANDATORY_TOKENS.iter().map(|s|s.to_string()).collect::<HashSet<String>>();
        expected.extend(config::BASIC_TOKENS.iter().map(|s|s.to_string()));
        expected.insert(config::TOKEN_URL_FULL.to_string());
        expected.insert(config::TOKEN_SERVER_PORT.to_string());
        // TOKEN_URL_PATH is mandatory, so without_token should not have removed it.

        assert_eq!(creator.selected_tokens, expected, "Complex chaining check");
    }
}
