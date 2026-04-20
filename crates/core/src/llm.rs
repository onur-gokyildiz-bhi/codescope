//! Dream-D — minimal LLM client.
//!
//! Speaks to an Ollama-compatible `/api/generate` endpoint by
//! default. The same shape also works against other inference
//! gateways that accept `{model, prompt, stream}` (LM Studio,
//! llama.cpp server with the Ollama compat layer, etc.). If the
//! user wants Anthropic / OpenAI they can reverse-proxy.
//!
//! Two env vars gate every call:
//! * `CODESCOPE_LLM_URL`   — default `http://127.0.0.1:11434`.
//! * `CODESCOPE_LLM_MODEL` — default `qwen2.5:7b-instruct`.
//!
//! When the LLM is unreachable or returns an error, callers fall
//! back to their existing template output — we never hard-fail
//! a render path on infra issues.

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::time::Duration;

const DEFAULT_URL: &str = "http://127.0.0.1:11434";
const DEFAULT_MODEL: &str = "qwen2.5:7b-instruct";

#[derive(Debug, Clone)]
pub struct LlmConfig {
    pub url: String,
    pub model: String,
    /// Hard upper bound. Ollama `keep_alive` can hold a model hot,
    /// but a cold model on a laptop CPU can take 60+ s.
    pub timeout: Duration,
}

impl LlmConfig {
    /// Read the env vars; `None` when the user didn't opt in.
    /// We treat "empty string" the same as "unset" so
    /// `CODESCOPE_LLM_URL=` disables LLM cleanly.
    pub fn from_env() -> Option<Self> {
        let url = std::env::var("CODESCOPE_LLM_URL")
            .ok()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| DEFAULT_URL.to_string());
        let model = std::env::var("CODESCOPE_LLM_MODEL")
            .ok()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| DEFAULT_MODEL.to_string());
        // Opt-in: either var must be explicitly set to something.
        if std::env::var("CODESCOPE_LLM_URL").is_err()
            && std::env::var("CODESCOPE_LLM_MODEL").is_err()
        {
            return None;
        }
        Some(LlmConfig {
            url,
            model,
            timeout: Duration::from_secs(90),
        })
    }
}

#[derive(Serialize)]
struct OllamaReq<'a> {
    model: &'a str,
    prompt: &'a str,
    stream: bool,
}

#[derive(Deserialize)]
struct OllamaResp {
    response: String,
}

/// One-shot completion. Returns the model's text verbatim — the
/// caller trims / post-processes as they see fit.
pub async fn complete(cfg: &LlmConfig, prompt: &str) -> Result<String> {
    let url = format!("{}/api/generate", cfg.url.trim_end_matches('/'));
    let client = reqwest::Client::builder()
        .timeout(cfg.timeout)
        .build()
        .context("build reqwest client")?;
    let body = OllamaReq {
        model: &cfg.model,
        prompt,
        stream: false,
    };
    let resp = client
        .post(&url)
        .json(&body)
        .send()
        .await
        .with_context(|| format!("POST {url}"))?;
    if !resp.status().is_success() {
        bail!("LLM returned {}", resp.status());
    }
    let parsed: OllamaResp = resp.json().await.context("parse Ollama response")?;
    Ok(parsed.response)
}
