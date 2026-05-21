use super::Engine;
use openloom_inference::CompletionRequest;
use openloom_models::{ChatMessage, EngineEvent};
use std::sync::atomic::Ordering;
use std::time::Instant;

use super::memory_thread;
use super::token_store::TokenUsageRecord;

impl Engine {
    pub async fn stream_complete(
        &self,
        req: CompletionRequest,
        tx: tokio::sync::mpsc::Sender<String>,
    ) -> anyhow::Result<()> {
        if let Some(ref cloud) = self.cloud {
            cloud.complete_stream(req, tx).await
        } else if let Some(ref local) = self.local_client {
            local.complete_stream(req, tx).await
        } else {
            self.inference.complete_stream(req, tx).await
        }
    }

    /// Full pipeline streaming: routes through context weaver, memory extraction,
    /// and message persistence while streaming the response token-by-token.
    pub async fn handle_message_streaming(
        &self,
        msg: ChatMessage,
        session_id: &str,
        tx: tokio::sync::mpsc::Sender<String>,
    ) -> anyhow::Result<()> {
        // Rate limiting
        {
            let mut limiter = self.rate_limiter.lock().unwrap();
            limiter.check()?;
        }
        *self.last_user_message.lock().unwrap() = Instant::now();

        if self.draining.load(Ordering::SeqCst) {
            let _ = tx.send("[server shutting down]".into()).await;
            return Ok(());
        }

        // Classify intent via router
        let out = self.router.classify_sync(&msg.content);

        // Feed memory pipeline (non-blocking)
        let _ = self.memory_tx.send(memory_thread::ProcessRequest {
            session_id: session_id.to_string(),
            text: msg.content.clone(),
            context: out.intent.to_string(),
        });

        // Complex intent, skill match, or cloud model available -> agent loop with streaming markers
        let has_cloud = self.cloud.is_some();
        if out.complexity >= 0.5 || out.skill_match.is_some() || has_cloud {
            let tx_clone = tx.clone();
            match self.agent_loop_streaming(&msg, session_id, tx_clone).await {
                Ok(resp) => {
                    let _ = tx.send(resp.response).await;
                }
                Err(e) => {
                    let _ = tx.send(format!("[agent error: {}]", e)).await;
                }
            }
            return Ok(());
        }

        // Assemble context (persona + skill context + working memory + system prompt)
        let skill_ctx = out.skill_match.as_ref().and_then(|name| {
            self.skills
                .find_by_name(name)
                .map(|s| s.context_md().to_string())
        });
        let working_memory = self.get_working_memory(session_id).unwrap_or_default();
        let persona_summary = self.persona.summarize().await.unwrap_or_default();
        let system = crate::system_instruction().replace("[tools]", "None");
        let assembled = self.weaver.assemble(
            &system,
            &msg.content,
            &persona_summary,
            skill_ctx.as_deref(),
            &working_memory,
        );

        let start = Instant::now();

        // Collect response while streaming
        let (collector_tx, mut collector_rx) = tokio::sync::mpsc::channel::<String>(64);
        let user_tx = tx.clone();

        // Spawn a forwarder that both sends to user AND collects full response
        let collector_handle = tokio::spawn(async move {
            let mut full_response = String::new();
            let mut usage_info: Option<(usize, usize, usize)> = None;
            while let Some(token) = collector_rx.recv().await {
                // Intercept usage signal from streaming — don't forward to user
                if let Some(usage_str) = token.strip_prefix("\x00USAGE:") {
                    let parts: Vec<&str> = usage_str.split(':').collect();
                    if parts.len() == 3 {
                        let p = parts[0].parse().unwrap_or(0);
                        let c = parts[1].parse().unwrap_or(0);
                        let cached = parts[2].parse().unwrap_or(0);
                        usage_info = Some((p, c, cached));
                    }
                    continue;
                }
                full_response.push_str(&token);
                let _ = user_tx.send(token).await;
            }
            (full_response, usage_info)
        });

        // Stream the completion
        let req = CompletionRequest {
            prompt: assembled.prompt.clone(),
            max_tokens: self.max_output_tokens,
            temperature: 0.7,
            top_p: 1.0,
            stop: Vec::new(),
            stream: true,
        };

        let stream_result = if let Some(ref cloud) = self.cloud {
            cloud.complete_stream(req, collector_tx).await
        } else if let Some(ref local) = self.local_client {
            local.complete_stream(req, collector_tx).await
        } else {
            self.inference.complete_stream(req, collector_tx).await
        };

        // Wait for collector to finish
        let (full_response, stream_usage) = collector_handle.await.unwrap_or_default();
        let latency_ms = start.elapsed().as_millis() as u64;

        if let Err(e) = stream_result {
            let _ = tx.send(format!("\n[error: {}]", e)).await;
        }

        // Post-streaming: persist messages, emit token usage
        // Prefer API-reported usage over local estimation
        let (prompt_tokens, completion_tokens, cached_tokens) = stream_usage.unwrap_or_else(|| {
            let p = self.inference.token_count(&assembled.prompt);
            let c = self.inference.token_count(&full_response);
            (p, c, 0)
        });

        let _ = self.save_messages(session_id, &msg, &full_response);

        let model_name = self.model_display_name();
        let _ = self.event_bus.send(EngineEvent::TokenUsage {
            session_id: session_id.to_string(),
            model: model_name.clone(),
            prompt_tokens,
            completion_tokens,
            cached_tokens,
            latency_ms,
        });
        let _ = self.token_store_tx.send(TokenUsageRecord {
            session_id: session_id.to_string(),
            model: model_name,
            prompt_tokens,
            completion_tokens,
            cached_tokens,
            latency_ms,
        });

        Ok(())
    }
}
