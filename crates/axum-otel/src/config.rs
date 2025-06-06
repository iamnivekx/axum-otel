//! This module defines "tokens" that represent telemetry attributes and provides
//! sets of these tokens for configuring attribute recording.
//!
//! The token constants defined herein (e.g., `TOKEN_HTTP_REQUEST_METHOD`) are intended
//! for use with the fluent builder methods `with_token()` and `without_token()` on
//! `AxumOtelSpanCreator` and `AxumOtelOnResponse`.
//!
//! Predefined sets like `MANDATORY_TOKENS`, `BASIC_TOKENS`, and `ALL_RECOGNIZED_TOKENS`
//! are used by the `select_none()`, `select_basic_set()`, and `select_full_set()` builder methods.
//!
//! For a conceptual overview of attribute selection, refer to the main library
//! documentation, particularly the crate-level section on "Attribute Configuration".

use crate::AttributeVerbosity; // AttributeSelection is no longer directly used by users of builders
use phf::{phf_map, phf_set, Set as PhfSet};
use std::collections::HashSet;

// --- Token Definitions ---
// These are abstract names for pieces of information that can be recorded as attributes.
// They are mapped to OpenTelemetry semantic convention keys where applicable via TOKEN_TO_OTEL_KEY.

/// Token for Client's IP address. Maps to `client.address`.
pub const TOKEN_CLIENT_ADDRESS: &str = "client.address";
/// Token for HTTP request method. Maps to `http.request.method`.
pub const TOKEN_HTTP_REQUEST_METHOD: &str = "http.request.method";
/// Token for Matched Axum route. Maps to `http.route`.
pub const TOKEN_HTTP_ROUTE: &str = "http.route";
/// Token for Network protocol name (e.g., "http", "https"). Maps to `network.protocol.name`.
pub const TOKEN_NETWORK_PROTOCOL_NAME: &str = "network.protocol.name";
/// Token for Network protocol version (e.g., "HTTP/1.1"). Maps to `network.protocol.version`.
pub const TOKEN_NETWORK_PROTOCOL_VERSION: &str = "network.protocol.version";
/// Token for OpenTelemetry span kind (e.g., "server"). Maps to `otel.kind`.
pub const TOKEN_OTEL_KIND: &str = "otel.kind";
/// Token for OpenTelemetry span name. Maps to `otel.name`. (Usually the HTTP method and route).
pub const TOKEN_OTEL_NAME: &str = "otel.name";
/// Token for OpenTelemetry span status (e.g., "OK", "ERROR"). Maps to `otel.status_code`.
pub const TOKEN_OTEL_STATUS_CODE: &str = "otel.status_code";
/// Token for Unique request ID. Maps to custom key `request_id`.
pub const TOKEN_REQUEST_ID: &str = "request_id";
/// Token for Server address (from Host header). Maps to `server.address`.
pub const TOKEN_SERVER_ADDRESS: &str = "server.address";
/// Token for Server port. Maps to `server.port`.
pub const TOKEN_SERVER_PORT: &str = "server.port";
/// Token for OpenTelemetry trace ID. Maps to custom key `trace_id`.
pub const TOKEN_TRACE_ID: &str = "trace_id"; // special, populated by set_otel_parent
/// Token for Full request URL. Maps to `url.full`.
pub const TOKEN_URL_FULL: &str = "url.full";
/// Token for Request path. Maps to `url.path`.
pub const TOKEN_URL_PATH: &str = "url.path";
/// Token for Request URL query parameters. Maps to `url.query`.
pub const TOKEN_URL_QUERY: &str = "url.query";
/// Token for Request URL scheme (e.g., "http", "https"). Maps to `url.scheme`.
pub const TOKEN_URL_SCHEME: &str = "url.scheme";
/// Token for User-Agent header. Maps to `user_agent.original`.
pub const TOKEN_USER_AGENT_ORIGINAL: &str = "user_agent.original";

// For OnResponse / OnFailure attributes
/// Token for HTTP response status code. Maps to `http.response.status_code`.
pub const TOKEN_HTTP_RESPONSE_STATUS_CODE: &str = "http.response.status_code";
/// Token for HTTP response body size (Content-Length). Maps to `http.response.body.size`.
pub const TOKEN_HTTP_RESPONSE_BODY_SIZE: &str = "http.response.body.size";
/// Token for Response processing time in milliseconds. Maps to custom key `response_time_ms`.
pub const TOKEN_RESPONSE_TIME_MS: &str = "response_time_ms";

// --- Token to OpenTelemetry Key Mapping ---
/// Maps internal tokens to their corresponding OpenTelemetry semantic convention attribute keys.
/// If a token represents a custom attribute or does not have a direct OTel key,
/// its own token name might be used as the key or handled specially.
pub static TOKEN_TO_OTEL_KEY: phf::Map<&'static str, &'static str> = phf_map! {
    TOKEN_CLIENT_ADDRESS => "client.address",
    TOKEN_HTTP_REQUEST_METHOD => "http.request.method",
    TOKEN_HTTP_ROUTE => "http.route",
    TOKEN_NETWORK_PROTOCOL_NAME => "network.protocol.name",
    TOKEN_NETWORK_PROTOCOL_VERSION => "network.protocol.version",
    TOKEN_OTEL_KIND => "otel.kind", // This is a tracing concept, not directly an OTel attribute key for spans usually.
                                    // It's set via `SpanBuilder::with_kind`. Let's map it for consistency if used as a field.
                                    // However, otel.name and otel.kind are usually set when span is created.
                                    // For our purpose, otel.name is the span name.
    TOKEN_OTEL_NAME => "otel.name", // This is the span name itself.
    TOKEN_OTEL_STATUS_CODE => "otel.status_code", // OpenTelemetry span status (OK/ERROR)
    TOKEN_REQUEST_ID => "request_id", // Custom
    TOKEN_SERVER_ADDRESS => "server.address",
    TOKEN_SERVER_PORT => "server.port",
    TOKEN_TRACE_ID => "trace_id", // Custom, but standard field in logs
    TOKEN_URL_FULL => "url.full",
    TOKEN_URL_PATH => "url.path",
    TOKEN_URL_QUERY => "url.query",
    TOKEN_URL_SCHEME => "url.scheme",
    TOKEN_USER_AGENT_ORIGINAL => "user_agent.original",
    // OnResponse / OnFailure specific
    TOKEN_HTTP_RESPONSE_STATUS_CODE => "http.response.status_code", // HTTP status code
    TOKEN_HTTP_RESPONSE_BODY_SIZE => "http.response.body.size",
    // TOKEN_RESPONSE_TIME_MS is custom and doesn't map to a standard OTel key directly.
    // It will be recorded with its own token name as the key.
};

// --- Token Sets ---

/// Tokens that are absolutely mandatory for minimal trace identification and utility.
/// These should generally always be included if possible.
pub static MANDATORY_TOKENS: PhfSet<&'static str> = phf_set! {
    TOKEN_OTEL_NAME, // Span name is fundamental
    TOKEN_OTEL_KIND, // Span kind is fundamental
    TOKEN_HTTP_REQUEST_METHOD, // Essential for HTTP context
    TOKEN_HTTP_ROUTE,          // Often the most useful for aggregation
    TOKEN_URL_PATH,            // Actual path
    TOKEN_HTTP_RESPONSE_STATUS_CODE, // Essential for response outcome (filled by OnResponse/OnFailure)
    TOKEN_OTEL_STATUS_CODE,    // Span status (OK/ERROR) (filled by OnResponse/OnFailure)
    TOKEN_REQUEST_ID,          // For correlation
    TOKEN_TRACE_ID,            // For correlation (though usually part of trace context, good to log)
};

/// Tokens included in the `AttributeVerbosity::Basic` level.
/// This set should include `MANDATORY_TOKENS`.
pub static BASIC_TOKENS: PhfSet<&'static str> = phf_set! {
    // Includes all MANDATORY_TOKENS implicitly by design of the filtering logic,
    // but explicitly listing helps clarity if desired, or we can rely on the union.
    // For now, assume logic will handle union; these are *additional* to mandatory if any.
    // Or, Basic = Mandatory + these few. Let's define Basic as the complete set for that level.
    TOKEN_OTEL_NAME,
    TOKEN_OTEL_KIND,
    TOKEN_HTTP_REQUEST_METHOD,
    TOKEN_HTTP_ROUTE,
    TOKEN_URL_PATH,
    TOKEN_HTTP_RESPONSE_STATUS_CODE,
    TOKEN_OTEL_STATUS_CODE,
    TOKEN_REQUEST_ID,
    TOKEN_TRACE_ID,
    // No extras for Basic beyond what's in Mandatory for now.
    // If response_time_ms is basic:
    // TOKEN_RESPONSE_TIME_MS,
};

/// All tokens that this library can potentially record.
/// This represents the `AttributeVerbosity::Full` level.
pub static ALL_RECOGNIZED_TOKENS: PhfSet<&'static str> = phf_set! {
    TOKEN_CLIENT_ADDRESS,
    TOKEN_HTTP_REQUEST_METHOD,
    TOKEN_HTTP_ROUTE,
    TOKEN_NETWORK_PROTOCOL_NAME,
    TOKEN_NETWORK_PROTOCOL_VERSION,
    TOKEN_OTEL_KIND,
    TOKEN_OTEL_NAME,
    TOKEN_OTEL_STATUS_CODE,
    TOKEN_REQUEST_ID,
    TOKEN_SERVER_ADDRESS,
    TOKEN_SERVER_PORT,
    TOKEN_TRACE_ID,
    TOKEN_URL_FULL,
    TOKEN_URL_PATH,
    TOKEN_URL_QUERY,
    TOKEN_URL_SCHEME,
    TOKEN_USER_AGENT_ORIGINAL,
    TOKEN_HTTP_RESPONSE_STATUS_CODE,
    TOKEN_HTTP_RESPONSE_BODY_SIZE,
    TOKEN_RESPONSE_TIME_MS,
};

/// All tokens that `AxumOtelOnResponse` can potentially record.
/// This represents the `AttributeVerbosity::Full` level for the `OnResponse` handler.
pub static ALL_ON_RESPONSE_RECOGNIZED_TOKENS: PhfSet<&'static str> = phf_set! {
    // Specific to OnResponse
    TOKEN_HTTP_RESPONSE_STATUS_CODE,
    TOKEN_OTEL_STATUS_CODE, // Set to "OK"
    TOKEN_HTTP_RESPONSE_BODY_SIZE,
    TOKEN_RESPONSE_TIME_MS,
    // Relevant from MANDATORY_TOKENS (if not already covered)
    // Note: Most MANDATORY_TOKENS are request-related or span identity.
    // TOKEN_REQUEST_ID, TOKEN_TRACE_ID might be on the span but not set by OnResponse itself.
    // We include them here if `OnResponse` might *read* or *could* set them,
    // or if "full" for response means including all identifiable parts of the span.
    // For now, focusing on what OnResponse *writes*.
};


// --- Helper Function for Filtering ---

/// Determines if a given token should be recorded based on the attribute selection strategy.
pub fn should_record_token(
    token: &str,
    selection: &AttributeSelection,
    // Pre-calculated HashSets for user-defined lists if applicable
    user_include_set: Option<&HashSet<String>>,
    user_exclude_set: Option<&HashSet<String>>,
) -> bool {
    if MANDATORY_TOKENS.contains(token) {
        return true;
    }

    match selection {
        AttributeSelection::Level(verbosity) => match verbosity {
            AttributeVerbosity::Full => ALL_RECOGNIZED_TOKENS.contains(token),
            AttributeVerbosity::Basic => BASIC_TOKENS.contains(token),
        },
        AttributeSelection::Include(ref include_list) => {
            // If user_include_set is provided, use it. Otherwise, build it on the fly (less efficient for many calls).
            match user_include_set {
                Some(set) => set.contains(token),
                None => include_list.iter().any(|s| s == token),
            }
        }
        AttributeSelection::Exclude(ref exclude_list) => {
            // If it's in the recognized set of all possible tokens AND not in the user's exclude list.
            let excluded = match user_exclude_set {
                Some(set) => set.contains(token),
                None => exclude_list.iter().any(|s| s == token),
            };
            if excluded {
                false
            } else {
                // If not explicitly excluded, it's included if it's a known "Full" attribute.
                ALL_RECOGNIZED_TOKENS.contains(token)
            }
        }
    }
}

// Helper to get the OpenTelemetry key for a token, or the token itself if no direct mapping.
pub fn get_otel_key_for_token(token: &str) -> &str {
    TOKEN_TO_OTEL_KEY.get(token).copied().unwrap_or(token)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_should_record_logic() {
        let mut user_list = HashSet::new();
        user_list.insert(TOKEN_USER_AGENT_ORIGINAL.to_string());
        user_list.insert(TOKEN_URL_QUERY.to_string());

        // Mandatory always true
        assert!(should_record_token(TOKEN_HTTP_REQUEST_METHOD, &AttributeSelection::Level(AttributeVerbosity::Basic), None, None));
        assert!(should_record_token(TOKEN_HTTP_REQUEST_METHOD, &AttributeSelection::Include(vec![TOKEN_CLIENT_ADDRESS.to_string()]), Some(&HashSet::from([TOKEN_CLIENT_ADDRESS.to_string()])), None));
        assert!(should_record_token(TOKEN_HTTP_REQUEST_METHOD, &AttributeSelection::Exclude(vec![TOKEN_HTTP_REQUEST_METHOD.to_string()]), None, Some(&HashSet::from([TOKEN_HTTP_REQUEST_METHOD.to_string()])) ));

        // Level::Full
        let full_selection = AttributeSelection::Level(AttributeVerbosity::Full);
        assert!(should_record_token(TOKEN_USER_AGENT_ORIGINAL, &full_selection, None, None));
        assert!(should_record_token(TOKEN_HTTP_REQUEST_METHOD, &full_selection, None, None));

        // Level::Basic
        let basic_selection = AttributeSelection::Level(AttributeVerbosity::Basic);
        assert!(!should_record_token(TOKEN_USER_AGENT_ORIGINAL, &basic_selection, None, None)); // Not in Basic
        assert!(should_record_token(TOKEN_HTTP_ROUTE, &basic_selection, None, None));      // In Basic (and Mandatory)

        // Include
        let include_selection = AttributeSelection::Include(vec![TOKEN_USER_AGENT_ORIGINAL.to_string(), TOKEN_URL_QUERY.to_string()]);
        assert!(should_record_token(TOKEN_USER_AGENT_ORIGINAL, &include_selection, Some(&user_list), None));
        assert!(!should_record_token(TOKEN_CLIENT_ADDRESS, &include_selection, Some(&user_list), None));
        assert!(should_record_token(TOKEN_HTTP_REQUEST_METHOD, &include_selection, Some(&user_list), None)); // Mandatory

        // Exclude
        let exclude_selection = AttributeSelection::Exclude(vec![TOKEN_USER_AGENT_ORIGINAL.to_string()]);
        let user_exclude_list_for_test = HashSet::from([TOKEN_USER_AGENT_ORIGINAL.to_string()]);

        assert!(!should_record_token(TOKEN_USER_AGENT_ORIGINAL, &exclude_selection, None, Some(&user_exclude_list_for_test)));
        assert!(should_record_token(TOKEN_CLIENT_ADDRESS, &exclude_selection, None, Some(&user_exclude_list_for_test))); // Part of Full, not excluded
        assert!(should_record_token(TOKEN_HTTP_REQUEST_METHOD, &exclude_selection, None, Some(&user_exclude_list_for_test))); // Mandatory
        // Test excluding something not in ALL_RECOGNIZED_TOKENS (should still be false)
        assert!(!should_record_token("some.random.token", &exclude_selection, None, Some(&user_exclude_list_for_test)));
         // Test excluding something from basic (but not mandatory)
        // (Assuming TOKEN_RESPONSE_TIME_MS is in ALL but not MANDATORY and not BASIC for this example)
        // If we add TOKEN_RESPONSE_TIME_MS to ALL_RECOGNIZED_TOKENS but not Basic/Mandatory
        // let exclude_resp_time = AttributeSelection::Exclude(vec![TOKEN_RESPONSE_TIME_MS.to_string()]);
        // let user_exclude_resp_time = HashSet::from([TOKEN_RESPONSE_TIME_MS.to_string()]);
        // assert!(!should_record_token(TOKEN_RESPONSE_TIME_MS, &exclude_resp_time, None, Some(&user_exclude_resp_time)));
    }

    #[test]
    fn test_get_otel_key() {
        assert_eq!(get_otel_key_for_token(TOKEN_HTTP_REQUEST_METHOD), "http.request.method");
        assert_eq!(get_otel_key_for_token(TOKEN_RESPONSE_TIME_MS), TOKEN_RESPONSE_TIME_MS); // Custom, returns token itself
        assert_eq!(get_otel_key_for_token("non.existent.token"), "non.existent.token");
    }
}
