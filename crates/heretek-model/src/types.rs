use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    System,
    User,
    Assistant,
    Tool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Message {
    pub role: Role,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tool_calls: Vec<ToolCall>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

impl Message {
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: Role::System,
            content: Some(content.into()),
            tool_calls: Vec::new(),
            tool_call_id: None,
        }
    }

    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: Role::User,
            content: Some(content.into()),
            tool_calls: Vec::new(),
            tool_call_id: None,
        }
    }

    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: Role::Assistant,
            content: Some(content.into()),
            tool_calls: Vec::new(),
            tool_call_id: None,
        }
    }

    pub fn tool_call_response(id: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            role: Role::Tool,
            content: Some(content.into()),
            tool_calls: Vec::new(),
            tool_call_id: Some(id.into()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: String,
}

impl Serialize for ToolCall {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        #[derive(Serialize)]
        struct Function<'a> {
            name: &'a str,
            arguments: &'a str,
        }

        #[derive(Serialize)]
        struct Wire<'a> {
            id: &'a str,
            #[serde(rename = "type")]
            kind: &'static str,
            function: Function<'a>,
        }

        Wire {
            id: &self.id,
            kind: "function",
            function: Function {
                name: &self.name,
                arguments: &self.arguments,
            },
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for ToolCall {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct Wire {
            id: String,
            function: Function,
        }

        #[derive(Deserialize)]
        struct Function {
            name: String,
            #[serde(default, deserialize_with = "deserialize_arguments")]
            arguments: String,
        }

        let wire = Wire::deserialize(deserializer)?;
        Ok(Self {
            id: wire.id,
            name: wire.function.name,
            arguments: wire.function.arguments,
        })
    }
}

fn deserialize_arguments<'de, D: Deserializer<'de>>(deserializer: D) -> Result<String, D::Error> {
    let value = Value::deserialize(deserializer)?;
    Ok(match value {
        Value::String(text) => text,
        Value::Null => String::new(),
        other => other.to_string(),
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub parameters: Value,
}

impl ToolDefinition {
    pub fn as_openai(&self) -> Value {
        serde_json::json!({
            "type": "function",
            "function": {
                "name": self.name,
                "description": self.description,
                "parameters": self.parameters,
            }
        })
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ChatRequest {
    #[serde(default)]
    pub messages: Vec<Message>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tools: Vec<ToolDefinition>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
}

impl ChatRequest {
    pub fn to_body(&self, model: &str) -> Value {
        let mut body = serde_json::json!({
            "model": model,
            "messages": self.messages,
        });
        if !self.tools.is_empty() {
            body["tools"] =
                Value::Array(self.tools.iter().map(ToolDefinition::as_openai).collect());
        }
        if let Some(max_tokens) = self.max_tokens {
            body["max_tokens"] = Value::from(max_tokens);
        }
        if let Some(temperature) = self.temperature {
            body["temperature"] = Value::from(temperature);
        }
        body
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Usage {
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub total_tokens: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cached_tokens: Option<u64>,
}

impl Usage {
    pub fn from_openai(value: &Value) -> Self {
        Self {
            prompt_tokens: value
                .get("prompt_tokens")
                .and_then(Value::as_u64)
                .unwrap_or(0),
            completion_tokens: value
                .get("completion_tokens")
                .and_then(Value::as_u64)
                .unwrap_or(0),
            total_tokens: value
                .get("total_tokens")
                .and_then(Value::as_u64)
                .unwrap_or(0),
            cached_tokens: value
                .pointer("/prompt_tokens_details/cached_tokens")
                .and_then(Value::as_u64),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatResponse {
    #[serde(default)]
    pub content: Option<String>,
    #[serde(default)]
    pub tool_calls: Vec<ToolCall>,
    #[serde(default)]
    pub usage: Usage,
    pub model: String,
    #[serde(default)]
    pub finish_reason: Option<String>,
    pub raw: Value,
}

impl ChatResponse {
    pub fn tool_call_by_name(&self, name: &str) -> Option<&ToolCall> {
        self.tool_calls.iter().find(|call| call.name == name)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelHealth {
    pub reachable: bool,
    pub models: Vec<String>,
    pub latency_ms: u64,
    pub error: Option<String>,
}
