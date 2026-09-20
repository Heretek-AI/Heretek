use std::time::{Duration, Instant};

use heretek_core::ModelProfile;
use serde_json::Value;

use crate::error::ModelError;
use crate::types::{ChatRequest, ChatResponse, ModelHealth, ToolCall, Usage};

const MAX_ATTEMPTS: u32 = 3;
const BACKOFF_MS: [u64; 3] = [300, 900, 2700];
const MAX_ERROR_BODY_CHARS: usize = 600;

pub struct ModelClient {
    http: reqwest::blocking::Client,
    base_url: String,
    model: String,
    api_key: Option<String>,
}

impl ModelClient {
    pub fn new(profile: &ModelProfile) -> Result<Self, ModelError> {
        let base_url = profile.base_url.trim_end_matches('/').to_string();
        let api_key = match &profile.api_key_env {
            Some(var) => Some(
                std::env::var(var)
                    .ok()
                    .filter(|key| !key.is_empty())
                    .ok_or_else(|| ModelError::MissingApiKey(var.clone()))?,
            ),
            None => None,
        };
        let http = reqwest::blocking::Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(600))
            .user_agent(format!("heretek/{}", env!("CARGO_PKG_VERSION")))
            .build()?;
        Ok(Self {
            http,
            base_url,
            model: profile.model.clone(),
            api_key,
        })
    }

    pub fn model(&self) -> &str {
        &self.model
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    pub fn chat(&self, request: &ChatRequest) -> Result<ChatResponse, ModelError> {
        let url = format!("{}/chat/completions", self.base_url);
        let body = request.to_body(&self.model);
        for attempt in 0..MAX_ATTEMPTS {
            if attempt > 0 {
                std::thread::sleep(retry_delay(attempt));
            }
            match self.post_chat(&url, &body) {
                Ok(response) => return Ok(response),
                Err(error) => {
                    if !error.is_retryable() || attempt + 1 == MAX_ATTEMPTS {
                        return Err(error);
                    }
                }
            }
        }
        Err(ModelError::Transport(
            "chat request exhausted retries".to_string(),
        ))
    }

    pub fn list_models(&self) -> Result<Vec<String>, ModelError> {
        let url = format!("{}/models", self.base_url);
        let mut request = self.http.get(url);
        if let Some(key) = &self.api_key {
            request = request.bearer_auth(key);
        }
        let response = request.send()?;
        let value = self.decode(response)?;
        Ok(value
            .get("data")
            .and_then(Value::as_array)
            .map(|entries| {
                entries
                    .iter()
                    .filter_map(|entry| entry.get("id").and_then(Value::as_str).map(str::to_string))
                    .collect()
            })
            .unwrap_or_default())
    }

    pub fn health(&self) -> ModelHealth {
        let start = Instant::now();
        match self.list_models() {
            Ok(models) => ModelHealth {
                reachable: true,
                models,
                latency_ms: elapsed_ms(start),
                error: None,
            },
            Err(error) => ModelHealth {
                reachable: false,
                models: Vec::new(),
                latency_ms: elapsed_ms(start),
                error: Some(error.to_string()),
            },
        }
    }

    fn post_chat(&self, url: &str, body: &Value) -> Result<ChatResponse, ModelError> {
        let mut request = self.http.post(url).json(body);
        if let Some(key) = &self.api_key {
            request = request.bearer_auth(key);
        }
        let response = request.send()?;
        let value = self.decode(response)?;
        self.parse_chat(value)
    }

    fn decode(&self, response: reqwest::blocking::Response) -> Result<Value, ModelError> {
        let status = response.status();
        let body = response.text()?;
        if !status.is_success() {
            return Err(ModelError::Status {
                status: status.as_u16(),
                body: truncate(&body, MAX_ERROR_BODY_CHARS),
            });
        }
        serde_json::from_str(&body).map_err(|error| {
            ModelError::Parse(format!("invalid JSON from model endpoint: {error}"))
        })
    }

    fn parse_chat(&self, value: Value) -> Result<ChatResponse, ModelError> {
        let choice = value
            .get("choices")
            .and_then(Value::as_array)
            .and_then(|choices| choices.first())
            .ok_or_else(|| ModelError::Parse("response contained no choices".to_string()))?;
        let message = choice
            .get("message")
            .ok_or_else(|| ModelError::Parse("response choice had no message".to_string()))?;
        let content = parse_content(message.get("content"));
        let tool_calls = parse_tool_calls(message.get("tool_calls"));
        let usage = value
            .get("usage")
            .map(Usage::from_openai)
            .unwrap_or_default();
        let model = value
            .get("model")
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or_else(|| self.model.clone());
        let finish_reason = choice
            .get("finish_reason")
            .and_then(Value::as_str)
            .map(str::to_string);
        Ok(ChatResponse {
            content,
            tool_calls,
            usage,
            model,
            finish_reason,
            raw: value,
        })
    }
}

fn retry_delay(attempt: u32) -> Duration {
    let index = (attempt as usize)
        .saturating_sub(1)
        .min(BACKOFF_MS.len() - 1);
    Duration::from_millis(BACKOFF_MS[index])
}

fn elapsed_ms(start: Instant) -> u64 {
    start.elapsed().as_millis() as u64
}

fn truncate(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        text.to_string()
    } else {
        text.chars().take(max).collect()
    }
}

fn parse_content(value: Option<&Value>) -> Option<String> {
    match value? {
        Value::String(text) => Some(text.clone()),
        Value::Array(parts) => {
            let text: String = parts
                .iter()
                .filter_map(|part| match part {
                    Value::String(text) => Some(text.as_str()),
                    Value::Object(_) => part.get("text").and_then(Value::as_str),
                    _ => None,
                })
                .collect();
            if text.is_empty() { None } else { Some(text) }
        }
        _ => None,
    }
}

fn parse_tool_calls(value: Option<&Value>) -> Vec<ToolCall> {
    value
        .and_then(Value::as_array)
        .map(|calls| {
            calls
                .iter()
                .filter_map(|call| {
                    let id = call.get("id").and_then(Value::as_str)?.to_string();
                    let function = call.get("function")?;
                    let name = function.get("name").and_then(Value::as_str)?.to_string();
                    let arguments = match function.get("arguments") {
                        Some(Value::String(text)) => text.clone(),
                        Some(other) => other.to_string(),
                        None => String::new(),
                    };
                    Some(ToolCall {
                        id,
                        name,
                        arguments,
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}
