mod axum;
mod header_extractor;
mod middleware;
mod root_span;
mod root_span_builder;

pub use middleware::{StreamSpan, TracingLogger};
pub use root_span::RootSpan;
pub use root_span_builder::{RootSpanBuilder, RootSpanBuilder};
// Re-exporting the `Level` enum since it's used in our `root_span!` macro
pub use tracing::Level;

mod otel;
mod otel_span;
#[doc(hidden)]
pub mod root_span_macro;
