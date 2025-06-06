use crate::event_dynamic_lvl;
use axum::http;
use tower_http::trace::OnResponse;
use tracing::Level;

/// An implementor of [`OnResponse`] which records the response status code and latency.
///
/// Original implementation from [tower-http](https://github.com/tower-rs/tower-http/blob/main/tower-http/src/trace/on_response.rs).
///
/// This component adds attributes to the span related to the HTTP response. The set of
/// attributes recorded can be customized using the fluent builder methods provided:
/// - Start with a predefined set: [`Self::select_full_set()`] (default),
///   [`Self::select_basic_set()`], or [`Self::select_none()`].
/// - Incrementally add attributes: [`Self::with_token()`].
/// - Incrementally remove attributes: [`Self::without_token()`].
///
/// A simpler way to choose between `Full` and `Basic` predefined sets is via the
/// [`.attribute_verbosity()`](Self::attribute_verbosity) method.
///
/// See the [crate-level documentation on Attribute Configuration](../index.html#attribute-configuration)
/// for a detailed explanation of "tokens", mandatory attributes, and the available sets
/// relevant to response handling.
/// The default configuration records all available response-related attributes (`Full` verbosity).
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
use crate::{AttributeVerbosity, config}; // Removed AttributeSelection, added config
use std::collections::HashSet; // For selected_tokens

/// use tower_http::trace::TraceLayer;
///
/// let layer = TraceLayer::new_for_http()
///     .on_response(AxumOtelOnResponse::new().level(Level::INFO));
/// ```
#[derive(Clone, Debug)]
pub struct AxumOtelOnResponse {
    level: Level,
    selected_tokens: HashSet<String>,
}

impl Default for AxumOtelOnResponse {
    fn default() -> Self {
        Self {
            level: Level::DEBUG,
            selected_tokens: config::ALL_RECOGNIZED_TOKENS.iter().map(|s| s.to_string()).collect(),
        }
    }
}

impl AxumOtelOnResponse {
    /// Create a new `DefaultOnResponse`.
    ///
    /// By default, it uses `Level::DEBUG` and selects all recognized attributes for recording
    /// (equivalent to `AttributeVerbosity::Full`).
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

    /// Configures the component to record the "basic" set of response-related attributes.
    ///
    /// This set includes [mandatory attributes](crate::index.html#mandatory-attributes) relevant to
    /// response handling (like `http.response.status_code`, `otel.status_code`) and common essential
    /// response attributes defined by `config::BASIC_TOKENS` (if they are response-specific).
    /// It clears any previously selected, added, or removed tokens.
    pub fn select_basic_set(mut self) -> Self {
        self.selected_tokens.clear();
        self.selected_tokens.extend(config::MANDATORY_TOKENS.iter().map(|s| s.to_string()));
        self.selected_tokens.extend(config::BASIC_TOKENS.iter().map(|s| s.to_string()));
        // Ensure response-specific mandatory/basic tokens are explicitly included
        self.selected_tokens.insert(config::TOKEN_HTTP_RESPONSE_STATUS_CODE.to_string());
        self.selected_tokens.insert(config::TOKEN_OTEL_STATUS_CODE.to_string());
        self
    }

    /// Configures the component to record the "full" set of all recognized response-related attributes.
    ///
    /// This set includes all attributes defined in `config::ALL_ON_RESPONSE_RECOGNIZED_TOKENS`
    /// and relevant general mandatory tokens.
    /// This is the default behavior when an `AxumOtelOnResponse` is created with `::new()`.
    /// It clears any previously selected, added, or removed tokens.
    pub fn select_full_set(mut self) -> Self {
        self.selected_tokens.clear();
        self.selected_tokens.extend(config::ALL_ON_RESPONSE_RECOGNIZED_TOKENS.iter().map(|s| s.to_string()));
        // Ensure general mandatory tokens are also included, as they provide context.
        self.selected_tokens.extend(config::MANDATORY_TOKENS.iter().map(|s| s.to_string()));
        self
    }

    /// Configures the component to record only the [mandatory minimal set of attributes](crate::index.html#mandatory-attributes)
    /// relevant to response handling.
    ///
    /// It clears any previously selected, added, or removed tokens before applying the mandatory set.
    /// This includes general mandatory tokens and specific response-related mandatory tokens.
    pub fn select_none(mut self) -> Self {
        self.selected_tokens.clear();
        self.selected_tokens.extend(config::MANDATORY_TOKENS.iter().map(|s| s.to_string()));
        // Ensure response-specific mandatory tokens are explicitly included
        self.selected_tokens.insert(config::TOKEN_HTTP_RESPONSE_STATUS_CODE.to_string());
        self.selected_tokens.insert(config::TOKEN_OTEL_STATUS_CODE.to_string());
        self
    }

    /// Adds an individual token to the current set of attributes to be recorded by this component.
    ///
    /// The `token` must be one of the constants defined in the [`config`](crate::config) module
    /// relevant to response attributes (e.g., `config::TOKEN_HTTP_RESPONSE_BODY_SIZE`).
    ///
    /// # Panics
    /// Panics if the provided `token` is not found in `config::ALL_RECOGNIZED_TOKENS` (this check
    /// can be refined to `config::ALL_ON_RESPONSE_RECOGNIZED_TOKENS` if strictly only response tokens are allowed).
    pub fn with_token(mut self, token: &str) -> Self {
        assert!(config::ALL_RECOGNIZED_TOKENS.contains(token), "Token '{}' is not a recognized attribute token for OnResponse.", token);
        self.selected_tokens.insert(token.to_string());
        self
    }

    /// Removes an individual token from the current set of attributes to be recorded by this component.
    ///
    /// The `token` must be one of the constants defined in the [`config`](crate::config) module.
    /// [Mandatory attributes](crate::index.html#mandatory-attributes) (especially response-specific ones like
    /// `TOKEN_HTTP_RESPONSE_STATUS_CODE` and `TOKEN_OTEL_STATUS_CODE`) cannot be removed.
    pub fn without_token(mut self, token: &str) -> Self {
        if !(token == config::TOKEN_HTTP_RESPONSE_STATUS_CODE || token == config::TOKEN_OTEL_STATUS_CODE || config::MANDATORY_TOKENS.contains(token)) {
            self.selected_tokens.remove(token);
        }
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

        // --- Record attributes based on selected tokens ---
        if self.selected_tokens.contains(config::TOKEN_HTTP_RESPONSE_STATUS_CODE) {
            span.record(config::get_otel_key_for_token(config::TOKEN_HTTP_RESPONSE_STATUS_CODE), status);
        }

        if self.selected_tokens.contains(config::TOKEN_OTEL_STATUS_CODE) {
            span.record(config::get_otel_key_for_token(config::TOKEN_OTEL_STATUS_CODE), "OK");
        }

        if self.selected_tokens.contains(config::TOKEN_HTTP_RESPONSE_BODY_SIZE) {
            if let Some(content_length_header) = response.headers().get(http::header::CONTENT_LENGTH) {
                if let Ok(content_length_str) = content_length_header.to_str() {
                    if let Ok(content_length) = content_length_str.parse::<u64>() {
                        span.record(config::get_otel_key_for_token(config::TOKEN_HTTP_RESPONSE_BODY_SIZE), content_length);
                    }
                }
            }
        }

        if self.selected_tokens.contains(config::TOKEN_RESPONSE_TIME_MS) {
            span.record(config::TOKEN_RESPONSE_TIME_MS, latency.as_millis() as u64);
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
    use crate::config::{
        self, MANDATORY_TOKENS, BASIC_TOKENS, ALL_ON_RESPONSE_RECOGNIZED_TOKENS, ALL_RECOGNIZED_TOKENS,
        TOKEN_HTTP_RESPONSE_BODY_SIZE, TOKEN_RESPONSE_TIME_MS, TOKEN_HTTP_RESPONSE_STATUS_CODE, TOKEN_OTEL_STATUS_CODE,
    };
    use crate::AttributeVerbosity;
    use axum::http::{HeaderMap, Response, StatusCode};
    use std::collections::{HashMap, HashSet};
    use std::time::Duration;
    use tracing::Level;
    use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, Registry};
    use tracing::field::Visit; // Required for MockAttributeVisitor if used here

    // Helper to compare HashSet<String> with phf::Set<&'static str>
    fn assert_selected_tokens_equal(selected: &HashSet<String>, expected_phf_set: &phf::Set<&'static str>, context: &str) {
        let expected_hs: HashSet<String> = expected_phf_set.iter().map(|s| s.to_string()).collect();
        assert_eq!(*selected, expected_hs, "Context: {}", context);
    }

    // Helper to build expected sets for on_response tests, as its "Full" set is specific
    fn build_expected_set(
        base_set: Option<&phf::Set<&'static str>>,
        plus_mandatory: bool,
        plus_basic: bool,
        extras: &[&str]
    ) -> HashSet<String> {
        let mut expected = HashSet::new();
        if let Some(base) = base_set {
            expected.extend(base.iter().map(|s| s.to_string()));
        }
        if plus_mandatory {
            expected.extend(config::MANDATORY_TOKENS.iter().map(|s| s.to_string()));
        }
        if plus_basic {
            expected.extend(config::BASIC_TOKENS.iter().map(|s| s.to_string()));
        }
        for extra in extras {
            expected.insert(extra.to_string());
        }
        // OnResponse specific mandatory tokens
        expected.insert(config::TOKEN_HTTP_RESPONSE_STATUS_CODE.to_string());
        expected.insert(config::TOKEN_OTEL_STATUS_CODE.to_string());
        expected
    }


    #[test]
    fn on_response_default_initialization() {
        let on_response = AxumOtelOnResponse::default();
        // Default for OnResponse should be its own tailored "Full" set
        let expected = build_expected_set(Some(&config::ALL_ON_RESPONSE_RECOGNIZED_TOKENS), true, false, &[]);
        assert_eq!(on_response.selected_tokens, expected, "Default init for OnResponse");
    }

    #[test]
    fn on_response_select_basic_set() {
        let on_response = AxumOtelOnResponse::default().select_basic_set();
        let expected = build_expected_set(None, true, true, &[]);
        assert_eq!(on_response.selected_tokens, expected, "OnResponse select_basic_set");
    }

    #[test]
    fn on_response_select_full_set() {
        let on_response = AxumOtelOnResponse::default().select_basic_set().select_full_set();
        let expected = build_expected_set(Some(&config::ALL_ON_RESPONSE_RECOGNIZED_TOKENS), true, false, &[]);
        assert_eq!(on_response.selected_tokens, expected, "OnResponse select_full_set");
    }

    #[test]
    fn on_response_select_none() {
        let on_response = AxumOtelOnResponse::default().select_none();
        // select_none for OnResponse means mandatory for OnResponse + general mandatory
        let expected = build_expected_set(None, true, false, &[]);
        assert_eq!(on_response.selected_tokens, expected, "OnResponse select_none");
    }

    #[test]
    fn on_response_with_token() {
        let on_response = AxumOtelOnResponse::default()
            .select_none() // Start with OnResponse mandatory + general mandatory
            .with_token(config::TOKEN_HTTP_RESPONSE_BODY_SIZE);

        let expected = build_expected_set(None, true, false, &[config::TOKEN_HTTP_RESPONSE_BODY_SIZE]);
        assert_eq!(on_response.selected_tokens, expected, "OnResponse with_token");
    }

    #[test]
    #[should_panic(expected = "Token 'unknown_token_for_response' is not a recognized attribute token for OnResponse.")]
    fn on_response_with_unknown_token_panics() {
        AxumOtelOnResponse::default().with_token("unknown_token_for_response");
    }

    #[test]
    fn on_response_without_token() {
        let on_response = AxumOtelOnResponse::default() // Starts with OnResponse Full set
            .without_token(config::TOKEN_HTTP_RESPONSE_BODY_SIZE); // Try to remove a specific on_response token

        assert!(!on_response.selected_tokens.contains(config::TOKEN_HTTP_RESPONSE_BODY_SIZE), "TOKEN_HTTP_RESPONSE_BODY_SIZE should be removed");
        // Check that mandatory on_response tokens are still there
        assert!(on_response.selected_tokens.contains(config::TOKEN_HTTP_RESPONSE_STATUS_CODE), "Mandatory TOKEN_HTTP_RESPONSE_STATUS_CODE should still be present");
        assert!(on_response.selected_tokens.contains(config::TOKEN_OTEL_STATUS_CODE), "Mandatory TOKEN_OTEL_STATUS_CODE should still be present");
        // Check that response_time_ms (part of OnResponse Full) is still there
        assert!(on_response.selected_tokens.contains(config::TOKEN_RESPONSE_TIME_MS), "TOKEN_RESPONSE_TIME_MS should still be present");
    }

    #[test]
    fn on_response_without_mandatory_token() {
         let on_response = AxumOtelOnResponse::default() // Starts with OnResponse Full set
            .without_token(config::TOKEN_HTTP_RESPONSE_STATUS_CODE); // Attempt to remove a mandatory on_response token

        assert!(on_response.selected_tokens.contains(config::TOKEN_HTTP_RESPONSE_STATUS_CODE), "Mandatory TOKEN_HTTP_RESPONSE_STATUS_CODE should still be present after trying to remove it");
    }


    #[test]
    fn on_response_attribute_verbosity_method() {
        let on_response_basic = AxumOtelOnResponse::default().attribute_verbosity(AttributeVerbosity::Basic);
        let expected_basic = build_expected_set(None, true, true, &[]);
        assert_eq!(on_response_basic.selected_tokens, expected_basic, "OnResponse attribute_verbosity(Basic)");

        let on_response_full = AxumOtelOnResponse::default().attribute_verbosity(AttributeVerbosity::Full);
        let expected_full = build_expected_set(Some(&config::ALL_ON_RESPONSE_RECOGNIZED_TOKENS), true, false, &[]);
        assert_eq!(on_response_full.selected_tokens, expected_full, "OnResponse attribute_verbosity(Full)");
    }

    // Previous tests for on_response attribute recording logic can remain,
    // they test the consumption of selected_tokens, not the builders themselves.
    // MockAttributeVisitor and get_on_response_attributes would be needed if those tests are kept here.
    // For this subtask, focus is on builder methods.
}
