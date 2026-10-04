use async_trait::async_trait;
use codex_api::{ApiError, ResponseEvent, ResponseStream, ResponsesApiRequest};
use codex_model_provider::ModelInferenceContext;
use futures::StreamExt;
use tokio::sync::{mpsc, oneshot};

use agent_studios_protocol_adapters::AnthropicMessagesAdapter;
use agent_studios_protocol_adapters::anthropic::{
    AnthropicRequestOptions, AnthropicStreamEvent, AnthropicThinkingPolicy,
};
use agent_studios_provider::instance::ProviderInstance;
use agent_studios_provider::model::ModelDescriptor;

use crate::auth::ResolvedAuth;
use crate::drivers::ProtocolDriver;
use crate::error::TransportError;
use crate::options::{RuntimeTransportOptions, read_bounded_error_body};
use crate::sse::SseStream;
use crate::state::ContinuationTransaction;

/// Driver for Anthropic Messages protocol (`POST /v1/messages`).
#[derive(Debug, Default)]
pub struct AnthropicDriver;

impl AnthropicDriver {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl ProtocolDriver for AnthropicDriver {
    #[allow(clippy::too_many_arguments)]
    async fn stream(
        &self,
        client: &reqwest::Client,
        instance: &ProviderInstance,
        descriptor: &ModelDescriptor,
        auth: &ResolvedAuth,
        request: ResponsesApiRequest,
        _context: ModelInferenceContext,
        continuation_tx: ContinuationTransaction,
        options: &RuntimeTransportOptions,
    ) -> Result<ResponseStream, TransportError> {
        let mut validated_url = options.validate_url(&instance.endpoint.base_url)?;
        auth.check_collisions(
            instance.endpoint.static_headers.keys(),
            instance.endpoint.query_params.keys(),
            validated_url.query_pairs().map(|(k, _)| k),
        )?;

        let max_tokens = descriptor.limits.max_output_tokens.ok_or_else(|| {
            TransportError::MissingRequiredModelLimit {
                model: request.model.clone(),
                limit: "max_output_tokens",
            }
        })?;

        let mut anthropic_options = AnthropicRequestOptions::new(max_tokens);

        if request
            .reasoning
            .as_ref()
            .and_then(|r| r.effort.as_ref())
            .is_some()
        {
            anthropic_options.thinking_policy = AnthropicThinkingPolicy::Adaptive;
        }

        let translation = AnthropicMessagesAdapter::translate_request(
            &request,
            &anthropic_options,
            Some(&continuation_tx.staged().anthropic),
        )?;

        let mut path = validated_url.path().trim_end_matches('/').to_string();
        if !path.ends_with("/messages") {
            path.push_str("/messages");
            validated_url.set_path(&path);
        }

        let mut req = client.post(validated_url.as_str());
        req = auth.apply(req);
        for (k, v) in &instance.endpoint.static_headers {
            req = req.header(k, v);
        }
        for (k, v) in &instance.endpoint.query_params {
            req = req.query(&[(k, v)]);
        }
        req = req.header(reqwest::header::CONTENT_TYPE, "application/json");

        let has_version_header = instance
            .endpoint
            .static_headers
            .keys()
            .any(|k| k.eq_ignore_ascii_case("anthropic-version"));

        if !has_version_header {
            req = req.header("anthropic-version", "2023-06-01");
        }
        req = req.json(&translation.request);

        let resp = tokio::time::timeout(options.request_headers_timeout, req.send())
            .await
            .map_err(|_| TransportError::RequestHeadersTimeout)?
            .map_err(TransportError::from)?;
        let status = resp.status();
        if !status.is_success() {
            let body =
                read_bounded_error_body(resp, options.max_error_body_bytes, Some(auth)).await?;
            return Err(TransportError::Http(format!(
                "Anthropic messages request failed with status {status}: {body}"
            )));
        }

        let upstream_request_id = resp
            .headers()
            .get("request-id")
            .or_else(|| resp.headers().get("x-request-id"))
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());

        let (tx_event, rx_event) = mpsc::channel(128);
        let (interrupt_tx, mut interrupt_rx) = oneshot::channel();

        let bytes_stream = resp.bytes_stream();
        let mut sse_stream = SseStream::with_max_bytes(bytes_stream, options.max_sse_event_bytes);
        let mut translator = AnthropicMessagesAdapter::new_stream_translator();

        let stream_idle_timeout = options.stream_idle_timeout;

        tokio::spawn(async move {
            let mut tx_opt = Some(continuation_tx);
            let mut completed = false;

            loop {
                tokio::select! {
                    _ = &mut interrupt_rx => {
                        let _ = tx_event.send(Err(ApiError::Stream("Stream was interrupted".to_string()))).await;
                        if let Some(tx) = tx_opt.take() {
                            tx.rollback();
                        }
                        break;
                    }
                    item = tokio::time::timeout(stream_idle_timeout, sse_stream.next()) => {
                        match item {
                            Ok(Some(Ok(sse_event))) => {
                                let data = sse_event.data.trim();
                                if data.is_empty() {
                                    continue;
                                }

                                let event: AnthropicStreamEvent = match serde_json::from_str(data) {
                                    Ok(ev) => ev,
                                    Err(e) => {
                                        let _ = tx_event.send(Err(ApiError::Stream(format!(
                                            "Failed to parse Anthropic stream event (length {} bytes): {e}",
                                            data.len()
                                        )))).await;
                                        if let Some(tx) = tx_opt.take() {
                                            tx.rollback();
                                        }
                                        return;
                                    }
                                };

                                match translator.feed_event(&event) {
                                    Ok(events) => {
                                        for ev in events {
                                            if matches!(ev, ResponseEvent::Completed { .. }) {
                                                if let Some(tx) = tx_opt.as_mut() {
                                                    *tx.anthropic_mut() = translator.continuation_state().clone();
                                                }
                                                if tx_event.send(Ok(ev)).await.is_ok() {
                                                    completed = true;
                                                    if let Some(tx) = tx_opt.take() {
                                                        match tx.commit() {
                                                            Ok(()) => {}
                                                            Err(e) => {
                                                                let _ = tx_event.send(Err(ApiError::Stream(format!(
                                                                    "Failed to commit continuation state: {e}"
                                                                )))).await;
                                                                return;
                                                            }
                                                        }
                                                    }
                                                } else {
                                                    // Receiver dropped before Completed delivery
                                                    if let Some(tx) = tx_opt.take() {
                                                        tx.rollback();
                                                    }
                                                    return;
                                                }
                                                break;
                                            } else if tx_event.send(Ok(ev)).await.is_err() {
                                                // Receiver dropped mid-stream
                                                if let Some(tx) = tx_opt.take() {
                                                    tx.rollback();
                                                }
                                                return;
                                            }
                                        }
                                    }
                                    Err(e) => {
                                        let _ = tx_event.send(Err(ApiError::Stream(e.to_string()))).await;
                                        if let Some(tx) = tx_opt.take() {
                                            tx.rollback();
                                        }
                                        return;
                                    }
                                }

                                if completed {
                                    break;
                                }
                            }
                            Ok(Some(Err(e))) => {
                                let _ = tx_event.send(Err(ApiError::Stream(e.to_string()))).await;
                                if let Some(tx) = tx_opt.take() {
                                    tx.rollback();
                                }
                                return;
                            }
                            Ok(None) => {
                                if !completed && !translator.is_completed() {
                                    let _ = tx_event.send(Err(ApiError::Stream(
                                        "Anthropic stream terminated before receiving completion event".to_string()
                                    ))).await;
                                    if let Some(tx) = tx_opt.take() {
                                        tx.rollback();
                                    }
                                }
                                break;
                            }
                            Err(_elapsed) => {
                                let _ = tx_event.send(Err(ApiError::Stream(
                                    TransportError::StreamIdleTimeout.to_string()
                                ))).await;
                                if let Some(tx) = tx_opt.take() {
                                    tx.rollback();
                                }
                                return;
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
