use axum::extract::Request;
use axum::middleware::{self, Next};
use axum::response::Response;
use axum::Router;
use tracing::{Instrument, Span};

/// The client's name for itself, if it is safe to put in a log.
fn client_name(request: &Request) -> String {
    request
        .headers()
        .get("x-client")
        .and_then(|v| v.to_str().ok())
        .filter(|name| (1..=32).contains(&name.len()))
        .filter(|name| name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-'))
        .unwrap_or("unknown")
        .to_string()
}

/// Logs if dropped while armed: the request's future was thrown away before there was a response.
struct Abandoned {
    span: Span,
    armed: bool,
}

impl Drop for Abandoned {
    fn drop(&mut self) {
        if self.armed {
            tracing::warn!(parent: &self.span, "request abandoned");
        }
    }
}

async fn log_request(request: Request, next: Next) -> Response {
    let span = tracing::info_span!(
        "http",
        method = %request.method(),
        path = %request.uri().path(),
        client = %client_name(&request),
    );
    let mut abandoned = Abandoned {
        span: span.clone(),
        armed: true,
    };
    // `instrument` enters the span on every poll and leaves it again, so it never leaks across an await.
    let response = next.run(request).instrument(span.clone()).await;
    abandoned.armed = false;
    tracing::info!(parent: &span, status = response.status().as_u16(), "request done");
    response
}

pub fn observe(app: Router) -> Router {
    app.layer(middleware::from_fn(log_request))
}
