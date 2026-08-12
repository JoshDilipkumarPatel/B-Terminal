use anyhow::Result;
use candle_core::{Device, Tensor};
use candle_transformers::models::quantized_llama::ModelWeights;
use std::path::PathBuf;
use tokenizers::Tokenizer;
use serde::{Deserialize, Serialize};
use zeroize::{Zeroize, ZeroizeOnDrop};

/// Enforces the Structured Output requirement for the Sentiment Agent.
#[derive(Debug, Serialize, Deserialize)]
pub struct SentimentScore {
    pub ticker: String,
    pub score: f32,
    pub confidence: f32,
}

/// Secure buffer that zeroes its memory when dropped
#[derive(Zeroize, ZeroizeOnDrop)]
pub struct SecurePrompt {
    pub content: String,
}

pub struct CandleLlamaEngine {
    model: ModelWeights,
    tokenizer: Tokenizer,
    device: Device,
}

impl CandleLlamaEngine {
    pub fn new(api_key: Option<&str>) -> Result<Self> {
        // Use a quantized GGUF model for hardware-accelerated Windows inference
        let model_id = "TheBloke/Llama-3-8B-Instruct-GGUF";
        let tokenizer_filename = "tokenizer.json";
        let model_filename = "llama-3-8b-instruct.Q4_K_M.gguf";

        let cache_dir = dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".cache")
            .join("b-terminal")
            .join("models");

        std::fs::create_dir_all(&cache_dir)?;

        let tokenizer_path = cache_dir.join(tokenizer_filename);
        let model_path = cache_dir.join(model_filename);

        // Blocking downloader using our SChannel-backed reqwest instance
        let download_file = |filename: &str, out_path: &PathBuf| -> Result<()> {
            if out_path.exists() {
                return Ok(());
            }
            let url = format!("https://huggingface.co/{}/resolve/main/{}", model_id, filename);
            tracing::info!("Downloading model file {} via SChannel...", url);
            let mut client_builder = reqwest::blocking::Client::builder();
            if let Some(token) = api_key {
                let mut headers = reqwest::header::HeaderMap::new();
                let auth_value = reqwest::header::HeaderValue::from_str(&format!("Bearer {}", token))?;
                headers.insert(reqwest::header::AUTHORIZATION, auth_value);
                client_builder = client_builder.default_headers(headers);
            }
            let client = client_builder.build()?;
            let mut response = client.get(&url).send()?.error_for_status()?;
            let mut file = std::fs::File::create(out_path)?;
            std::io::copy(&mut response, &mut file)?;
            Ok(())
        };

        std::thread::scope(|s| {
            s.spawn(|| -> Result<()> {
                download_file(tokenizer_filename, &tokenizer_path)?;
                download_file(model_filename, &model_path)?;
                Ok(())
            }).join().expect("Thread panicked")
        }).map_err(|e| anyhow::anyhow!("Download failed: {}", e))?;

        // Use CPU by default for GGUF since DirectML isn't natively in this version's core flags
        let device = Device::Cpu; 
        
        let tokenizer = Tokenizer::from_file(&tokenizer_path)
            .map_err(|e| anyhow::anyhow!("Tokenizer error: {}", e))?;

        // Load the Quantized model (Q4_K_M)
        let mut file = std::fs::File::open(&model_path)?;
        let content = candle_core::quantized::gguf_file::Content::read(&mut file)?;
        let model = ModelWeights::from_gguf(content, &mut file, &device)?;

        Ok(Self {
            model,
            tokenizer,
            device,
        })
    }

    pub fn infer_sentiment(&mut self, prompt: &str) -> Result<SentimentScore> {
        // Enforce memory zeroization for sensitive OCR/financial data
        let mut secure_prompt = SecurePrompt {
            content: prompt.to_string(),
        };

        let tokens = self.tokenizer.encode(secure_prompt.content.as_str(), true)
            .map_err(|e| anyhow::anyhow!("{}", e))?;
        
        let tokens = tokens.get_ids().to_vec();
        let max_tokens = 50; 
        
        let mut generated_text = String::new();
        let mut current_token = tokens.clone();
        
        for _ in 0..max_tokens {
            let input = Tensor::new(current_token.as_slice(), &self.device)?.unsqueeze(0)?;
            
            // ModelWeights in quantized_llama natively manages its KV cache internally on forward passes
            let logits = self.model.forward(&input, 0)?;
            let logits = logits.squeeze(0)?.to_vec1::<f32>()?;
            
            // Argmax sampling (greedy)
            let (max_idx, _) = logits.iter().enumerate()
                .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap())
                .unwrap();
                
            if max_idx as u32 == 2 { // EOS token
                break;
            }
            
            if let Ok(text) = self.tokenizer.decode(&[max_idx as u32], false) {
                generated_text.push_str(&text);
            }
            current_token = vec![max_idx as u32];
        }

        // Wipe prompt explicitly before drop just in case
        secure_prompt.content.zeroize();

        // Attempt strict structured decoding
        match serde_json::from_str::<SentimentScore>(&generated_text) {
            Ok(score) => Ok(score),
            Err(_) => {
                // If the LLM failed to output exact JSON, fallback gracefully
                Ok(SentimentScore {
                    ticker: "UNKNOWN".to_string(),
                    score: 0.0,
                    confidence: 0.0,
                })
            }
        }
    }

    pub fn explain_decision(&mut self, prompt: &str) -> Result<String> {
        let mut secure_prompt = SecurePrompt {
            content: prompt.to_string(),
        };

        let tokens = self.tokenizer.encode(secure_prompt.content.as_str(), true)
            .map_err(|e| anyhow::anyhow!("{}", e))?;
        
        let tokens = tokens.get_ids().to_vec();
        let max_tokens = 200; // Longer generation for summaries
        
        let mut generated_text = String::new();
        let mut current_token = tokens.clone();
        
        for _ in 0..max_tokens {
            let input = Tensor::new(current_token.as_slice(), &self.device)?.unsqueeze(0)?;
            let logits = self.model.forward(&input, 0)?;
            let logits = logits.squeeze(0)?.to_vec1::<f32>()?;
            
            let (max_idx, _) = logits.iter().enumerate()
                .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap())
                .unwrap();
                
            if max_idx as u32 == 2 { break; }
            
            if let Ok(text) = self.tokenizer.decode(&[max_idx as u32], false) {
                generated_text.push_str(&text);
            }
            current_token = vec![max_idx as u32];
        }

        secure_prompt.content.zeroize();
        Ok(generated_text.trim().to_string())
    }
}
