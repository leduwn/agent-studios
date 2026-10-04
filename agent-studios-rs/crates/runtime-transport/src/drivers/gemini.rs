use async_trait::async_trait;
use codex_api::{ApiError, ResponseEvent, ResponseStream, ResponsesApiRequest};
use codex_model_provider::ModelInferenceContext;
use futures::StreamExt;
use tokio::sync::{mpsc, oneshot};

use agent_studios_protocol_adapters::gemini::{
    GeminiAdapter, GeminiAdapterOptions, GeminiGenerateContentResponse, GeminiThinkingPolicy,
};
use agent_studios_provider::instance::ProviderInstance;
use agent_studios_provider::model::ModelDescriptor;

use crate::auth::ResolvedAuth;
use crate::drivers::ProtocolDriver;
use crate::error::TransportError;
use crate::options::{RuntimeTransportOptions, read_bounded_error_body};
use crate::sse::SseStream;
use crate::state::ContinuationTransaction;

/// Driver for Google Gemini `streamGenerateContent` protocol.
#[derive(Debug, Default)]
pub struct GeminiDriver;

impl GeminiDriver {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl ProtocolDriver for GeminiDriver {
    #[allow(clippy::too_many_arguments)]
    async fn stream(
        &self,
        client: &reqwest::Client,
        instance: &ProviderInstance,
        _descriptor: &ModelDescriptor,
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

        let mut gemini_options = GeminiAdapterOptions::new();
        if request.reasoning.is_some() {
            gemini_options =
                gemini_options.with_thinking_policy(GeminiThinkingPolicy::ExactReasoningEffort);
        }

        let translation = GeminiAdapter::translate_request(
            &request,
            &gemini_options,
            Some(&continuation_tx.staged().gemini),
        )?;

        for (k, v) in validated_url.query_pairs() {
            if k.eq_ignore_ascii_case("alt") && v != "sse" {
                return Err(TransportError::ReservedQueryParameterCollision {
                    parameter: "alt".to_string(),
                });
            }
        }
        for (k, v) in &instance.endpoint.query_params {
            if k.eq_ignore_ascii_case("alt") && v != "sse" {
                return Err(TransportError::ReservedQueryParameterCollision {
                    parameter: "alt".to_string(),
                });
            }
        }

        let base_alt_count = validated_url
            .query_pairs()
            .filter(|(k, _)| k.eq_ignore_ascii_case("alt"))
            .count();
        let endpoint_alt_count = instance
            .endpoint
            .query_params
            .iter()
            .filter(|(k, _)| k.eq_ignore_ascii_case("alt"))
            .count();

        // Reject duplicate configured alt across configuration sources or within either source
        if (base_alt_count > 0 && endpoint_alt_count > 0)
            || base_alt_count > 1
            || endpoint_alt_count > 1
        {
            return Err(TransportError::ReservedQueryParameterCollision {
                parameter: "alt".to_string(),
            });
        }

        let encoded_model: String = url::form_urlencoded::byte_serialize(request.model.as_bytes())
            .collect::<String>()
            .replace('+', "%20");
        let base_path = validated_url.path().trim_end_matches('/');
        let new_path = format!("{base_path}/models/{encoded_model}:streamGenerateContent");
        validated_url.set_path(&new_path);

        let has_alt_sse = base_alt_count > 0 || endpoint_alt_count > 0;
        if !has_alt_sse {
            validated_url.query_pairs_mut().append_pair("alt", "sse");
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
                "Gemini generate content request failed with status {status}: {body}"
            )));
        }

        let upstream_request_id = resp
            .headers()
            .get("x-goog-request-id")
            .or_else(|| resp.headers().get("x-request-id"))
            .or_else(|| resp.headers().get("request-id"))
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());

        let (tx_event, rx_event) = mpsc::channel(128);
        let (interrupt_tx, mut interrupt_rx) = oneshot::channel();

        let bytes_stream = resp.bytes_stream();
        let mut sse_stream = SseStream::with_max_bytes(bytes_stream, options.max_sse_event_bytes);
        let mut translator = GeminiAdapter::new_stream_translator();

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

                                let chunk: GeminiGenerateContentResponse = match serde_json::from_str(data) {
                                    Ok(c) => c,
                                    Err(e) => {
                                        let _ = tx_event.send(Err(ApiError::Stream(format!(
                                            "Failed to parse Gemini stream chunk (length {} bytes): {e}",
                                            data.len()
                                        )))).await;
                                        if let Some(tx) = tx_opt.take() {
                                            tx.rollback();
                                        }
                                        return;
                                    }
                                };

                                match translator.feed_response(&chunk) {
                                    Ok(events) => {
                                        for ev in events {
                                            if matches!(ev, ResponseEvent::Completed { .. }) {
                                                if let Some(tx) = tx_opt.as_mut() {
                                                    *tx.gemini_mut() = translator.continuation_state().clone();
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
                                                    if let Some(tx) = tx_opt.take() {
                                                        tx.rollback();
                                                    }
                                                    return;
                                                }
                                                break;
                                            } else if tx_event.send(Ok(ev)).await.is_err() {
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
                                    match translator.finish_stream() {
                                        Ok(events) => {
                                            for ev in events {
                                                if matches!(ev, ResponseEvent::Completed { .. }) {
                                                    if let Some(tx) = tx_opt.as_mut() {
                                                        *tx.gemini_mut() = translator.continuation_state().clone();
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
                                                        if let Some(tx) = tx_opt.take() {
                                                            tx.rollback();
                                                        }
                                                        return;
                                                    }
                                                    break;
                                                } else if tx_event.send(Ok(ev)).await.is_err() {
                                                    if let Some(tx) = tx_opt.take() {
                                                        tx.rollback();
                                                    }
                                                    return;
                                                }
                                            }
                                        }
                                        Err(e) => {
                                            let _ = tx_event.send(Err(ApiError::Stream(format!(
                                                "Gemini finish_stream failed: {e}"
                                            )))).await;
                                            if let Some(tx) = tx_opt.take() {
                                                tx.rollback();
                                            }
                                            return;
                                        }
                                    }
                                }
                                if !completed && !translator.is_completed() {
                                    let _ = tx_event.send(Err(ApiError::Stream(
                                        "Gemini stream ended before receiving completion event".to_string()
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
