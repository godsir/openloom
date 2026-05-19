use axum::extract::Path;
use axum::response::sse::{Event, Sse};
use futures::stream;
use std::convert::Infallible;

pub async fn sse_handler(
    Path(session_id): Path<String>,
) -> Sse<impl futures::Stream<Item = Result<Event, Infallible>>> {
    tracing::debug!(%session_id, "SSE connection opened");

    let stream = stream::once(async move {
        Ok(Event::default()
            .data(format!("SSE stream ready for session {}", session_id))
            .event("ready"))
    });

    Sse::new(stream)
}
