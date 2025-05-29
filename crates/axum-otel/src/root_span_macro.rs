#[macro_export]
/// `root_span!` creates a new [`tracing::Span`].
macro_rules! root_span {
    // Vanilla root span, with no additional fields
    ($request:ident) => {
        $crate::root_span!($request,)
    };
    // Vanilla root span, with a level but no additional fields
    (level = $lvl:expr, $request:ident) => {
        $crate::root_span!(level = $lvl, $request,)
    };
    // One or more additional fields, comma separated, without a level
    ($request:ident, $($field:tt)*) => {
        $crate::root_span!(level = $crate::Level::INFO, $request, $($field)*)
    };
    // One or more additional fields, comma separated
    (level = $lvl:expr, $request:ident, $($field:tt)*) => {
        {
            let user_agent = $request
                .headers()
                .get("User-Agent")
                .map(|h| h.to_str().unwrap_or(""))
                .unwrap_or("");
            let http_route: std::borrow::Cow<'static, str> = $request
                .match_pattern()
                .map(Into::into)
                .unwrap_or_else(|| "default".into());
            let http_method = $crate::root_span_macro::private::http_method_str($request.method());
            let connection_info = $request.connection_info();
            let request_id = $crate::root_span_macro::private::get_request_id($request);

            macro_rules! inner_span {
                ($level:expr) => {
                    $crate::root_span_macro::private::tracing::span!(
                        $level,
                        "HTTP request",
                        http.method = %http_method,
                        http.route = %http_route,
                        http.flavor = %$crate::root_span_macro::private::http_flavor($request.version()),
                        http.scheme = %$crate::root_span_macro::private::http_scheme(connection_info.scheme()),
                        http.host = %connection_info.host(),
                        http.client_ip = %$request.connection_info().realip_remote_addr().unwrap_or(""),
                        http.user_agent = %user_agent,
                        http.target = %$request.uri().path_and_query().map(|p| p.as_str()).unwrap_or(""),
                        http.status_code = $crate::root_span_macro::private::tracing::field::Empty,
                        otel.name = %format!("{} {}", http_method, http_route),
                        otel.kind = "server",
                        otel.status_code = $crate::root_span_macro::private::tracing::field::Empty,
                        trace_id = $crate::root_span_macro::private::tracing::field::Empty,
                        request_id = %request_id,
                        exception.message = $crate::root_span_macro::private::tracing::field::Empty,
                        // Not proper OpenTelemetry, but their terminology is fairly exception-centric
                        exception.stacktrace = $crate::root_span_macro::private::tracing::field::Empty,
                        $($field)*
                    )
                };
            }
            let span = match $lvl {
                $crate::Level::TRACE => inner_span!($crate::Level::TRACE),
                $crate::Level::DEBUG => inner_span!($crate::Level::DEBUG),
                $crate::Level::INFO => inner_span!($crate::Level::INFO),
                $crate::Level::WARN => inner_span!($crate::Level::WARN),
                $crate::Level::ERROR => inner_span!($crate::Level::ERROR),
            };
            std::mem::drop(connection_info);

            // Previously, this line was instrumented with an opentelemetry-specific feature
            // flag check. However, this resulted in the feature flags being resolved in the crate
            // which called `root_span!` as opposed to being resolved by this crate as expected.
            // Therefore, this function simply wraps an internal function with the feature flags
            // to ensure that the flags are resolved against this crate.
            $crate::root_span_macro::private::set_otel_parent(&$request, &span);

            span
        }
    };
}

#[doc(hidden)]
pub mod private {
    use crate::RequestId;
    use axum::http::{Method, Version};
    use std::borrow::Cow;

    pub use tracing;

    #[doc(hidden)]
    pub fn set_otel_parent(req: &Request, span: &tracing::Span) {
        crate::otel::set_otel_parent(req, span);
    }

    #[doc(hidden)]
    #[inline]
    pub fn http_method_str(method: &Method) -> Cow<'static, str> {
        match method {
            &Method::OPTIONS => "OPTIONS".into(),
            &Method::GET => "GET".into(),
            &Method::POST => "POST".into(),
            &Method::PUT => "PUT".into(),
            &Method::DELETE => "DELETE".into(),
            &Method::HEAD => "HEAD".into(),
            &Method::TRACE => "TRACE".into(),
            &Method::CONNECT => "CONNECT".into(),
            &Method::PATCH => "PATCH".into(),
            other => other.to_string().into(),
        }
    }

    #[doc(hidden)]
    #[inline]
    pub fn http_flavor(version: Version) -> Cow<'static, str> {
        match version {
            Version::HTTP_09 => "0.9".into(),
            Version::HTTP_10 => "1.0".into(),
            Version::HTTP_11 => "1.1".into(),
            Version::HTTP_2 => "2.0".into(),
            Version::HTTP_3 => "3.0".into(),
            other => format!("{other:?}").into(),
        }
    }

    #[doc(hidden)]
    #[inline]
    pub fn http_scheme(scheme: &str) -> Cow<'static, str> {
        match scheme {
            "http" => "http".into(),
            "https" => "https".into(),
            other => other.to_string().into(),
        }
    }

    #[doc(hidden)]
    pub fn generate_request_id() -> RequestId {
        RequestId::generate()
    }

    #[doc(hidden)]
    pub fn get_request_id(request: &Request) -> RequestId {
        request.extensions().get::<RequestId>().cloned().unwrap()
    }
}
