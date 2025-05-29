use crate::{RequestId, RootSpan, RootSpanBuilderTrait};
// Removed: use axum::body::{BodySize, MessageBody};
// Removed: use axum::dev::{Service as AxumService, ServiceRequest, ServiceResponse, Transform};
use opentelemetry::KeyValue; // Added for _custom_attributes
// Removed: use axum::http::StatusCode;
// Removed: use axum::web::Bytes;
use tower_layer::Layer;
use axum::Error; // Keep for OtelMiddleware's Error type if it's derived from S::Error: Into<Error>
// Removed: use axum::{HttpMessage, ResponseError};
// Removed: use std::future::{Ready, ready};
use std::future::Future; // Keep for ResponseFuture
use std::pin::Pin;
use std::task::{Context, Poll};
use tracing::Span;
use tower_service::Service; // New import
use http::{Request, Response}; // New import
// Removed: use futures_util::future::BoxFuture; // Not used
use pin_project::pin_project; // For ResponseFuture


// New OtelLayer struct
// R is now RootSpanBuilderTrait
/// A Tower [`Layer`] for OpenTelemetry tracing.
///
/// This layer creates a new root span for each incoming request and handles
/// context propagation. It uses a [`RootSpanBuilderTrait`] implementation
/// to customize how spans are created.
///
/// # Example
///
/// ```rust,no_run
/// use axum::{Router, routing::get};
/// use axum_otel::{OtelLayer, DefaultRootSpanBuilder};
///
/// async fn hello() -> &'static str {
///     "Hello, world!"
/// }
///
/// #[tokio::main]
/// async fn main() {
///     // You would typically set up your tracer provider here
///
///     let app = Router::new()
///         .route("/hello", get(hello))
///         // Create a new OtelLayer with the DefaultRootSpanBuilder
///         .layer(OtelLayer::<DefaultRootSpanBuilder>::new());
///
///     // Run the server
///     // let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
///     // axum::serve(listener, app).await.unwrap();
/// }
/// ```
///
/// # Generic Parameters
///
/// *   `R`: The type of the [`RootSpanBuilderTrait`] used to create root spans.
///     This allows for customization of span attributes and behavior.
///     [`DefaultRootSpanBuilder`] is provided as a sensible default.
///
pub struct OtelLayer<R: RootSpanBuilderTrait> {
    root_span_builder: std::marker::PhantomData<R>,
}

impl<R: RootSpanBuilderTrait> OtelLayer<R> {
    pub fn new() -> Self {
        OtelLayer {
            root_span_builder: std::marker::PhantomData,
        }
    }
}

impl<R: RootSpanBuilderTrait> Clone for OtelLayer<R> {
    fn clone(&self) -> Self {
        Self {
            root_span_builder: std::marker::PhantomData,
        }
    }
}

// Implement tower::Layer for OtelLayer
// R is now RootSpanBuilderTrait
impl<Svc, R> Layer<Svc> for OtelLayer<R>
where
    R: RootSpanBuilderTrait + Clone + Send + Sync + 'static + Default,
    Svc: Service<Request<axum::body::BoxBody>> + Clone + Send + 'static, // axum::body::BoxBody is a common body type for Axum
    Svc::Future: Send + 'static,
    Svc::Response: Send + 'static,
    Svc::Error: Into<Error> + Send + Sync + 'static,
{
    type Service = OtelMiddleware<Svc, R>;

    fn layer(&self, inner: Svc) -> Self::Service {
        OtelMiddleware {
            inner,
            _root_span_builder: std::marker::PhantomData,
        }
    }
}

// OtelMiddleware implements tower::Service
// R is now RootSpanBuilderTrait
#[derive(Clone)]
pub struct OtelMiddleware<S, R> {
    inner: S,
    _root_span_builder: std::marker::PhantomData<R>,
}

// Define the ResponseFuture to handle the lifecycle of the span
// R is now RootSpanBuilderTrait
#[pin_project]
pub struct ResponseFuture<F, R, ResBody, SErr>
where
    R: RootSpanBuilderTrait, 
    F: Future<Output = Result<Response<ResBody>, SErr>>,
    SErr: Into<Error>,
{
    #[pin]
    inner_future: F,
    span: Span,
    root_span_builder: R, 
    _phantom_resp: std::marker::PhantomData<ResBody>,
    _phantom_err: std::marker::PhantomData<SErr>,
}

// R is now RootSpanBuilderTrait
impl<F, R, ResBody, SErr> Future for ResponseFuture<F, R, ResBody, SErr>
where
    R: RootSpanBuilderTrait, 
    F: Future<Output = Result<Response<ResBody>, SErr>>,
    SErr: Into<Error>, 
{
    type Output = Result<Response<ResBody>, Error>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.project();
        let span = this.span.clone();
        let root_span_builder = this.root_span_builder;

        let _enter = span.enter();

        match this.inner_future.poll(cx) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(outcome_result) => {
                let outcome_for_otel = outcome_result.map_err(Into::into);
                root_span_builder.on_request_end(span, &outcome_for_otel, &[]); 
                Poll::Ready(outcome_for_otel)
            }
        }
    }
}

// Tower Service implementation for OtelMiddleware
// R is now RootSpanBuilderTrait
impl<S, R, ReqBody, ResBody> Service<Request<ReqBody>> for OtelMiddleware<S, R>
where
    R: RootSpanBuilderTrait + Clone + Send + Sync + 'static + Default, 
    S: Service<Request<ReqBody>, Response = Response<ResBody>> + Clone + Send + 'static,
    S::Future: Send + 'static,
    S::Error: Into<Error> + Send + Sync + 'static,
    ReqBody: Send + 'static,
    ResBody: Send + 'static,
{
    type Response = Response<ResBody>;
    type Error = Error;
    type Future = ResponseFuture<S::Future, R, ResBody, S::Error>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx).map_err(Into::into)
    }

    fn call(&mut self, mut req: Request<ReqBody>) -> Self::Future {
        let root_span_builder_instance = R::default();
        let span = root_span_builder_instance.on_request_start(&req, &[]);
        let _enter = span.enter();

        let extensions = req.extensions_mut();
        extensions.insert(RequestId::generate());
        extensions.insert(RootSpan::new(span.clone()));

        let response_future = self.inner.call(req);

        ResponseFuture {
            inner_future: response_future,
            span: span.clone(),
            root_span_builder: root_span_builder_instance,
            _phantom_resp: std::marker::PhantomData,
            _phantom_err: std::marker::PhantomData,
        }
    }
}
