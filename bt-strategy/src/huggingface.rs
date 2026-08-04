use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, CONTENT_TYPE};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tracing::{info, warn};

use crate::sentiment::{SentimentClass, SentimentScorer};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum InferenceSource {
    /// Live Hugging Face cloud inference (ProsusAI/finbert or Llama-3 endpoints)
    HuggingFaceCloudApi,
    /// Zero-latency internal Rust n-gram sentiment fallback engine
    LocalOfflineFallback,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HuggingFaceInferenceResult {
    pub model_used: String,
    pub source: InferenceSource,
    pub conviction_score: f64,
    pub classification: SentimentClass,
    pub latency_ms: u64,
    pub summary_note: String,
}

#[derive(Debug, Deserialize)]
struct HfClassificationScore {
    label: String,
    score: f64,
}

pub struct HuggingFaceEngine {
    client: reqwest::Client,
    default_model: String,
}

impl Default for HuggingFaceEngine {
    fn default() -> Self {
        Self::new("ProsusAI/finbert")
    }
}

impl HuggingFaceEngine {
    pub fn new(model_name: &str) -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(8))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());

        Self {
            client,
            default_model: model_name.to_string(),
        }
    }

    /// Analyzes an OCR-extracted document snippet using Hugging Face Serverless Inference APIs.
    /// If api_key is None or endpoint connection fails, automatically falls back to internal zero-latency engine.
    pub async fn analyze_document(
        &self,
        doc_text: &str,
        api_key: Option<&str>,
    ) -> HuggingFaceInferenceResult {
        let start_time = std::time::Instant::now();

        if let Some(key) = api_key {
            if !key.is_empty() {
                info!("Attempting Hugging Face cloud inference via model {}...", self.default_model);
                if let Ok(res) = self.query_hf_api(doc_text, key).await {
                    return res;
                }
                warn!("Hugging Face API unreachable or rate-limited. Engaging offline fallback engine.");
            }
        }

        // --- Local Zero-Latency Fallback ---
        let local_res = SentimentScorer::score(doc_text);
        let elapsed = start_time.elapsed().as_millis() as u64;

        let summary_note = match local_res.classification {
            SentimentClass::StrongBullish | SentimentClass::Bullish => {
                "Strong bullish fundamental indicators and positive earnings momentum detected in OCR tables."
            }
            SentimentClass::StrongBearish | SentimentClass::Bearish => {
                "Cautionary language, outflow alerts, or liability risks detected in filing."
            }
            SentimentClass::Neutral => {
                "Balanced corporate disclosure with stable guidance and minimal volatility triggers."
            }
        };

        HuggingFaceInferenceResult {
            model_used: "B-Terminal Local n-gram Quant Engine (Offline Fallback)".to_string(),
            source: InferenceSource::LocalOfflineFallback,
            conviction_score: local_res.score,
            classification: local_res.classification,
            latency_ms: elapsed.max(1), // Sub-millisecond execution
            summary_note: summary_note.to_string(),
        }
    }

    async fn query_hf_api(&self, text: &str, api_key: &str) -> anyhow::Result<HuggingFaceInferenceResult> {
        let start_time = std::time::Instant::now();
        let url = format!("https://api-inference.huggingface.co/models/{}", self.default_model);

        let mut headers = HeaderMap::new();
        let auth_val = format!("Bearer {}", api_key);
        headers.insert(AUTHORIZATION, HeaderValue::from_str(&auth_val)?);
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));

        // Truncate text to fit typical model input token windows (e.g., 512 words)
        let truncated: String = text.chars().take(1500).collect();
        let payload = serde_json::json!({
            "inputs": truncated,
            "parameters": { "wait_for_model": false }
        });

        let resp = self.client.post(&url)
            .headers(headers)
            .json(&payload)
            .send()
            .await?;

        if !resp.status().is_success() {
            anyhow::bail!("HF returned non-200 status: {}", resp.status());
        }

        let body: Vec<Vec<HfClassificationScore>> = resp.json().await?;
        let scores = body.first().ok_or_else(|| anyhow::anyhow!("Empty classification array"))?;

        let mut pos_weight = 0.0;
        let mut neg_weight = 0.0;
        for item in scores {
            let lbl = item.label.to_lowercase();
            if lbl == "positive" || lbl == "bullish" {
                pos_weight = item.score;
            } else if lbl == "negative" || lbl == "bearish" {
                neg_weight = item.score;
            }
        }

        let conviction = (pos_weight - neg_weight).clamp(-1.0, 1.0);
        let classification = if conviction > 0.4 {
            SentimentClass::StrongBullish
        } else if conviction > 0.15 {
            SentimentClass::Bullish
        } else if conviction < -0.4 {
            SentimentClass::StrongBearish
        } else if conviction < -0.15 {
            SentimentClass::Bearish
        } else {
            SentimentClass::Neutral
        };

        let elapsed = start_time.elapsed().as_millis() as u64;

        Ok(HuggingFaceInferenceResult {
            model_used: format!("HuggingFace Cloud ({})", self.default_model),
            source: InferenceSource::HuggingFaceCloudApi,
            conviction_score: conviction,
            classification,
            latency_ms: elapsed,
            summary_note: "Successfully ingested and scored via Hugging Face Serverless Inference API.".to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_hf_engine_offline_fallback() {
        let engine = HuggingFaceEngine::default();
        let sample_text = "FII inflow surge and profit growth makes me bullish on nifty OUTPERFORM";
        
        // Passing None as api_key should instantly run offline fallback without failing
        let res = engine.analyze_document(sample_text, None).await;
        
        assert_eq!(res.source, InferenceSource::LocalOfflineFallback);
        assert!(res.conviction_score > 0.5);
        assert_eq!(res.classification, SentimentClass::StrongBullish);
        assert!(res.latency_ms < 50, "Local offline execution must be extremely fast");
    }
}
