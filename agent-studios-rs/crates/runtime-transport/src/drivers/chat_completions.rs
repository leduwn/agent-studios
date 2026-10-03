use async_trait::async_trait;
use codex_api::{ApiError, ResponseEvent, ResponseStream, ResponsesApiRequest};
use codex_model_provider::ModelInferenceContext;
use futures::StreamExt;
use tokio::sync::{mpsc, oneshot};

use agent_studios_protocol_adapters::{ChatCompletionChunk, ChatCompletionsAdapter};
use agent_studios_provider::instance::ProviderInstance;

use crate::auth::ResolvedAuth;
use crate::drivers::ProtocolDriver;
use crate::error::TransportError;
use crate::sse::SseStream;
use crate::state::ContinuationTransaction;

/// Driver for OpenAI Chat Completions protocol (`POST /v1/chat/completions`).
#[derive(Debug, Default)]
pub struct ChatCompletionsDriver;

impl ChatCompletionsDriver {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl ProtocolDriver for ChatCompletionsDriver {
    async fn stream(
        &self,
        client: &reqwest::Client,
        instance: &ProviderInstance,
        auth: &ResolvedAuth,
        request: ResponsesApiRequest,
        _context: ModelInferenceContext,
        continuation_tx: ContinuationTransaction,
    ) -> Result<ResponseStream, TransportError> {
        let translation = ChatCompletionsAdapter::translate_request(&request)?;

        let base = instance.endpoint.base_url.trim_end_matches('/');
        let url = if base.ends_with("/chat/completions") {
            base.to_string()
        } else {
            format!("{base}/chat/completions")
        };

        let mut req = client.post(&url);
        req = auth.apply(req);
        for (k, v) in &instance.endpoint.static_headers {
            req = req.header(k, v);
        }
        for (k, v) in &instance.endpoint.query_params {
            req = req.query(&[(k, v)]);
        }
        req = req.header(reqwest::header::CONTENT_TYPE, "application/json");
        req = req.json(&translation.request);

        let resp = req.send().await?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(TransportError::Http(format!(
                "Chat completions request failed with status {status}: {body}"
            )));
        }

        let upstream_request_id = resp
            .headers()
            .get("x-request-id")
            .or_else(|| resp.headers().get("request-id"))
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());

        let (tx_event, rx_event) = mpsc::channel(128);
        let (interrupt_tx, mut interrupt_rx) = oneshot::channel();

        let bytes_stream = resp.bytes_stream();
        let mut sse_stream = SseStream::new(bytes_stream);
        let mut translator = ChatCompletionsAdapter::new_stream_translator();

        tokio::spawn(async move {
            let mut tx_opt = Some(continuation_tx);
            loop {
                tokio::select! {
                    _ = &mut interrupt_rx => {
                        let _ = tx_event.send(Err(ApiError::Stream("Stream was interrupted".to_string()))).await;
                        break;
                    }
                    item = sse_stream.next() => {
                        match item {
                            Some(Ok(sse_event)) => {
                                let data = sse_event.data.trim();
                                if data == "[DONE]" {
                                    if !translator.is_completed() {
                                        match translator.finish() {
                                            Ok(events) => {
                                                for ev in events {
                                                    if matches!(ev, ResponseEvent::Completed { .. }) {
                                                        let _ = tx_opt.take().map(|tx| tx.commit());
                                                    }
                                                    if tx_event.send(Ok(ev)).await.is_err() {
                                                        return;
                                                    }
                                                }
                                            }
                                            Err(e) => {
                                                let _ = tx_event.send(Err(ApiError::Stream(e.to_string()))).await;
                                                return;
                                            }
                                        }
                                    }
                                    break;
                                }

                                let chunk: ChatCompletionChunk = match serde_json::from_str(data) {
                                    Ok(c) => c,
                                    Err(e) => {
                                        let _ = tx_event.send(Err(ApiError::Stream(format!("Failed to parse chat chunk: {e}")))).await;
                                        return;
                                    }
                                };

                                match translator.feed_chunk(&chunk) {
                                    Ok(events) => {
                                        for ev in events {
                                            if matches!(ev, ResponseEvent::Completed { .. }) {
                                                let _ = tx_opt.take().map(|tx| tx.commit());
                                            }
                                            if tx_event.send(Ok(ev)).await.is_err() {
                                                return;
                                            }
                                        }
                                    }
                                    Err(e) => {
                                        let _ = tx_event.send(Err(ApiError::Stream(e.to_string()))).await;
                                        return;
                                    }
                                }
                            }
                            Some(Err(e)) => {
                                let _ = tx_event.send(Err(ApiError::Stream(e.to_string()))).await;
                                return;
                            }
                            None => {
                                if !translator.is_completed()
                                    && let Ok(events) = translator.finish()
                                {
                                    for ev in events {
                                        if matches!(ev, ResponseEvent::Completed { .. }) {
                                            let _ = tx_opt.take().map(|tx| tx.commit());
                                        }
                                        let _ = tx_event.send(Ok(ev)).await;
                                    }
                                }
                                break;
                            }
                        }
                    }
                }
            }
        });

        Ok(ResponseStream {
            rx_event,
            upstream_request_id,
            interrupt: Some(interrupt_tx),
        })
    }
}
