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
/// This span creator automatically adds attributes to each span based on the configured
/// [`AttributeSelection`] strategy. See the [crate-level documentation on Attribute Configuration](../index.html#attribute-configuration)
/// for a detailed explanation of tokens, mandatory attributes, and selection strategies
/// (`Level`, `Include`, `Exclude`).
///
/// The selection strategy can be set using the [`.attribute_selection()`](AxumOtelSpanCreator::attribute_selection) method,
/// or via the convenience method [`.attribute_verbosity()`](AxumOtelSpanCreator::attribute_verbosity) for predefined levels.
/// The default is equivalent to `AttributeSelection::Level(AttributeVerbosity::Full)`.
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
use crate::{AttributeVerbosity, AttributeSelection}; // Import enums

/// use tower_http::trace::TraceLayer;
///
/// let layer = TraceLayer::new_for_http()
///     .make_span_with(AxumOtelSpanCreator::new().level(Level::INFO));
/// ```
#[derive(Clone, Debug)] // AttributeSelection::Include/Exclude(Vec<String>) means not Copy
pub struct AxumOtelSpanCreator {
    level: Level,
    attribute_selection: AttributeSelection,
}

impl AxumOtelSpanCreator {
    /// Create a new `AxumOtelSpanCreator`.
    ///
    /// By default, it uses `Level::TRACE` and `AttributeSelection::Level(AttributeVerbosity::Full)`.
    pub fn new() -> Self {
        Self {
            level: Level::TRACE,
            attribute_selection: AttributeSelection::default(),
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
    /// # use axum_otel::{AxumOtelSpanCreator, AttributeSelection, AttributeVerbosity, config, Level};
    /// let creator_basic = AxumOtelSpanCreator::new()
    ///     .attribute_selection(AttributeSelection::Level(AttributeVerbosity::Basic));
    ///
    /// let creator_include = AxumOtelSpanCreator::new()
    ///     .attribute_selection(AttributeSelection::Include(vec![
    ///         config::TOKEN_HTTP_REQUEST_METHOD.to_string(),
    ///         config::TOKEN_USER_AGENT_ORIGINAL.to_string(),
    ///     ]));
    ///
    /// let creator_exclude = AxumOtelSpanCreator::new()
    ///     .attribute_selection(AttributeSelection::Exclude(vec![
    ///         config::TOKEN_URL_QUERY.to_string(),
    ///     ]));
    /// ```
    pub fn attribute_selection(mut self, selection: AttributeSelection) -> Self {
        self.attribute_selection = selection;
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

        // Prepare include/exclude sets for efficient lookup if that variant is chosen
        let user_include_set: Option<HashSet<String>> = match &self.attribute_selection {
            AttributeSelection::Include(list) => Some(list.iter().cloned().collect()),
            _ => None,
        };
        let user_exclude_set: Option<HashSet<String>> = match &self.attribute_selection {
            AttributeSelection::Exclude(list) => Some(list.iter().cloned().collect()),
            _ => None,
        };

        // Create the span with minimal initial fields (otel.name is set by tracing macro from span_name)
        // otel.kind is set by the tracing library's OpenTelemetry layer typically.
        // However, if we want to ensure it, we can add it if `should_record_token` says so for TOKEN_OTEL_KIND.
        // For now, let's rely on the tracing macro for name and level, and record others.
        let span = tracing::span!(self.level, %span_name, otel.kind = ?SpanKind::Server);

        // Enter the span to record attributes on it.
        let _enter = span.enter();

        // --- Record attributes based on selection strategy ---

        // TOKEN_OTEL_NAME is handled by span_name in tracing::span!
        // TOKEN_OTEL_KIND is handled by otel.kind = ?SpanKind::Server in tracing::span!

        // http.request.method
        if config::should_record_token(config::TOKEN_HTTP_REQUEST_METHOD, &self.attribute_selection, user_include_set.as_ref(), user_exclude_set.as_ref()) {
            span.record(config::get_otel_key_for_token(config::TOKEN_HTTP_REQUEST_METHOD), request.method().as_str());
        }

        // http.route
        if config::should_record_token(config::TOKEN_HTTP_ROUTE, &self.attribute_selection, user_include_set.as_ref(), user_exclude_set.as_ref()) {
            if let Some(r) = route_str {
                span.record(config::get_otel_key_for_token(config::TOKEN_HTTP_ROUTE), r);
            }
        }

        // url.path
        if config::should_record_token(config::TOKEN_URL_PATH, &self.attribute_selection, user_include_set.as_ref(), user_exclude_set.as_ref()) {
            span.record(config::get_otel_key_for_token(config::TOKEN_URL_PATH), request.uri().path());
        }

        // request_id
        if config::should_record_token(config::TOKEN_REQUEST_ID, &self.attribute_selection, user_include_set.as_ref(), user_exclude_set.as_ref()) {
            span.record(config::get_otel_key_for_token(config::TOKEN_REQUEST_ID), get_request_id(request.headers()));
        }

        // trace_id (This is usually handled by the OpenTelemetry layer, but we record it for logging)
        // The set_otel_parent function records this. We ensure it's only called if the token is allowed.
        // For now, set_otel_parent is called unconditionally at the end.
        // If TOKEN_TRACE_ID needs to be conditional for recording, set_otel_parent call needs modification/wrapping.
        // Let's assume set_otel_parent internally checks or TOKEN_TRACE_ID is mandatory for its purpose.
        // The current logic in set_otel_parent records "trace_id" unconditionally.

        // http.response.status_code & otel.status_code are placeholders, recorded by OnResponse/OnFailure
        // We can record them as Empty if the tokens are active.
        if config::should_record_token(config::TOKEN_HTTP_RESPONSE_STATUS_CODE, &self.attribute_selection, user_include_set.as_ref(), user_exclude_set.as_ref()) {
             span.record(config::get_otel_key_for_token(config::TOKEN_HTTP_RESPONSE_STATUS_CODE), Empty);
        }
        if config::should_record_token(config::TOKEN_OTEL_STATUS_CODE, &self.attribute_selection, user_include_set.as_ref(), user_exclude_set.as_ref()) {
            span.record(config::get_otel_key_for_token(config::TOKEN_OTEL_STATUS_CODE), Empty);
        }


        // --- Full Attributes (conditionally recorded) ---
        let client_ip_val = request.extensions().get::<ConnectInfo<SocketAddr>>().map(|ConnectInfo(ip)| ip);
        if config::should_record_token(config::TOKEN_CLIENT_ADDRESS, &self.attribute_selection, user_include_set.as_ref(), user_exclude_set.as_ref()) {
            if let Some(ip) = client_ip_val {
                 span.record(config::get_otel_key_for_token(config::TOKEN_CLIENT_ADDRESS), field::debug(ip));
            }
        }

        if config::should_record_token(config::TOKEN_NETWORK_PROTOCOL_VERSION, &self.attribute_selection, user_include_set.as_ref(), user_exclude_set.as_ref()) {
            span.record(config::get_otel_key_for_token(config::TOKEN_NETWORK_PROTOCOL_VERSION), field::debug(request.version()));
        }

        let host_val = request.headers().get(http::header::HOST).and_then(|h| h.to_str().ok());
        if config::should_record_token(config::TOKEN_SERVER_ADDRESS, &self.attribute_selection, user_include_set.as_ref(), user_exclude_set.as_ref()) {
            if let Some(h) = host_val {
                span.record(config::get_otel_key_for_token(config::TOKEN_SERVER_ADDRESS), h);
            }
        }

        let scheme_str_val = request.uri().scheme_str();
        if config::should_record_token(config::TOKEN_URL_SCHEME, &self.attribute_selection, user_include_set.as_ref(), user_exclude_set.as_ref()) {
            if let Some(s) = scheme_str_val {
                span.record(config::get_otel_key_for_token(config::TOKEN_URL_SCHEME), s);
            }
        }

        if config::should_record_token(config::TOKEN_NETWORK_PROTOCOL_NAME, &self.attribute_selection, user_include_set.as_ref(), user_exclude_set.as_ref()) {
            span.record(config::get_otel_key_for_token(config::TOKEN_NETWORK_PROTOCOL_NAME), scheme_str_val.unwrap_or("http"));
        }

        if config::should_record_token(config::TOKEN_URL_QUERY, &self.attribute_selection, user_include_set.as_ref(), user_exclude_set.as_ref()) {
            if let Some(q) = request.uri().query() {
                span.record(config::get_otel_key_for_token(config::TOKEN_URL_QUERY), q);
            }
        }

        let user_agent_val = request.headers().get(http::header::USER_AGENT).and_then(|ua| ua.to_str().ok());
        if config::should_record_token(config::TOKEN_USER_AGENT_ORIGINAL, &self.attribute_selection, user_include_set.as_ref(), user_exclude_set.as_ref()) {
            if let Some(ua) = user_agent_val {
                span.record(config::get_otel_key_for_token(config::TOKEN_USER_AGENT_ORIGINAL), ua);
            }
        }

        if config::should_record_token(config::TOKEN_SERVER_PORT, &self.attribute_selection, user_include_set.as_ref(), user_exclude_set.as_ref()) {
            if let Some(port) = request.uri().port_u16() {
                span.record(config::get_otel_key_for_token(config::TOKEN_SERVER_PORT), port);
            }
        }

        if config::should_record_token(config::TOKEN_URL_FULL, &self.attribute_selection, user_include_set.as_ref(), user_exclude_set.as_ref()) {
            if let (Some(scheme), Some(host)) = (scheme_str_val, host_val) { // host_val already extracted
                let path_and_query = request.uri().path_and_query().map_or("", |pq| pq.as_str());
                span.record(config::get_otel_key_for_token(config::TOKEN_URL_FULL), format!("{}://{}{}", scheme, host, path_and_query).as_str());
            }
        }

        // Special handling for TOKEN_TRACE_ID if set_otel_parent needs to be conditional
        // For now, assume trace_id from set_otel_parent is always desired if tracing is active.
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
}
