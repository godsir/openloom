use axum::extract::{Path, Query, State};
use axum::response::sse::{Event, Sse};
use futures::stream::{self, Stream};
use openloom_engine::Engine;
use openloom_inference::CompletionRequest;
use serde::Deserialize;
use std::convert::Infallible;
use std::sync::Arc;
use tokio::sync::mpsc;

#[derive(Deserialize)]
pub struct SseParams {
    prompt: String,
    max_tokens: Option<usize>,
}

pub async fn sse_handler(
    Path(session_id): Path<String>,
    Query(params): Query<SseParams>,
    State(engine): State<Arc<Engine>>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    tracing::debug!(%session_id, prompt_len = params.prompt.len(), "SSE streaming started");

    let (tx, rx) = mpsc::channel::<String>(64);

    let engine = engine.clone();
    tokio::spawn(async move {
        let req = CompletionRequest {
            prompt: params.prompt,
            max_tokens: params.max_tokens.unwrap_or(2048),
            ..Default::default()
        };
        if let Err(e) = engine.stream_complete(req, tx).await {
            tracing::warn!(%session_id, error = %e, "SSE stream_complete failed");
        }
    });

    let stream = stream::unfold(rx, |mut rx| async move {
        rx.recv().await.map(|token| (Ok(Event::default().data(token)), rx))
    });

    Sse::new(stream)
}
