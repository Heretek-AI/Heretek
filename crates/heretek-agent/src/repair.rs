use std::collections::VecDeque;

use heretek_model::ToolCall;

pub struct ToolCallRepair {
    recent: VecDeque<String>,
}

impl Default for ToolCallRepair {
    fn default() -> Self {
        Self::new()
    }
}

impl ToolCallRepair {
    pub fn new() -> Self {
        Self {
            recent: VecDeque::new(),
        }
    }

    pub fn repair(
        &mut self,
        content: Option<&str>,
        mut calls: Vec<ToolCall>,
    ) -> (Vec<ToolCall>, Vec<(&'static str, String)>) {
        let mut notes = Vec::new();

        if calls.is_empty()
            && let Some(content) = content
        {
            let scavenged = scavenge(content);
            if !scavenged.is_empty() {
                notes.push((
                    "scavenge",
                    format!("recovered {} tool call(s) from content", scavenged.len()),
                ));
                calls = scavenged;
            }
        }

        for call in &mut calls {
            if serde_json::from_str::<serde_json::Value>(&call.arguments).is_err()
                && let Some(fixed) = repair_truncated(&call.arguments)
            {
                notes.push((
                    "truncation",
                    format!("repaired arguments for '{}'", call.name),
                ));
                call.arguments = fixed;
            }
        }

        (calls, notes)
    }

    pub fn is_storm(&mut self, call: &ToolCall) -> bool {
        if matches!(call.name.as_str(), "read_file" | "list_dir" | "search") {
            return false;
        }
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        std::hash::Hash::hash(&call.name, &mut hasher);
        std::hash::Hash::hash(&call.arguments, &mut hasher);
        let signature = format!("{:x}", std::hash::Hasher::finish(&hasher));
        self.recent.push_back(signature.clone());
        while self.recent.len() > 10 {
            self.recent.pop_front();
        }
        self.recent
            .iter()
            .filter(|entry| **entry == signature)
            .count()
            >= 3
    }
}

pub fn repair_truncated(arguments: &str) -> Option<String> {
    let trimmed = arguments.trim();
    if trimmed.is_empty() {
        return Some("{}".to_string());
    }
    let mut stack: Vec<char> = Vec::new();
    let mut in_string = false;
    let mut escaped = false;
    for character in trimmed.chars() {
        if in_string {
            if escaped {
                escaped = false;
            } else if character == '\\' {
                escaped = true;
            } else if character == '"' {
                in_string = false;
            }
            continue;
        }
        match character {
            '"' => in_string = true,
            '{' => stack.push('}'),
            '[' => stack.push(']'),
            '}' | ']' if stack.last() == Some(&character) => {
                stack.pop();
            }
            '}' | ']' => {}
            _ => {}
        }
    }
    let mut candidate = trimmed.to_string();
    if in_string {
        candidate.push('"');
    }
    while let Some(closing) = stack.pop() {
        candidate.push(closing);
    }
    serde_json::from_str::<serde_json::Value>(&candidate)
        .ok()
        .map(|_| candidate)
}

pub fn scavenge(content: &str) -> Vec<ToolCall> {
    let mut calls = Vec::new();
    for candidate in json_candidates(content) {
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(&candidate) {
            if let Some(call) = call_from_value(&value, calls.len()) {
                calls.push(call);
            }
        }
    }
    calls
}

fn json_candidates(content: &str) -> Vec<String> {
    let mut candidates = Vec::new();
    let mut depth = 0usize;
    let mut start: Option<usize> = None;
    let mut in_string = false;
    let mut escaped = false;
    for (index, character) in content.char_indices() {
        if in_string {
            if escaped {
                escaped = false;
            } else if character == '\\' {
                escaped = true;
            } else if character == '"' {
                in_string = false;
            }
            continue;
        }
        match character {
            '"' => in_string = true,
            '{' => {
                if depth == 0 {
                    start = Some(index);
                }
                depth += 1;
            }
            '}' if depth > 0 => {
                depth -= 1;
                if depth == 0
                    && let Some(begin) = start.take()
                {
                    candidates.push(content[begin..=index].to_string());
                }
            }
            _ => {}
        }
    }
    candidates
}

fn call_from_value(value: &serde_json::Value, index: usize) -> Option<ToolCall> {
    let name = value
        .get("name")
        .or_else(|| value.get("tool"))
        .and_then(|value| value.as_str())
        .or_else(|| {
            value
                .get("function")
                .and_then(|function| function.get("name"))
                .and_then(|value| value.as_str())
        })?;
    let arguments = value
        .get("arguments")
        .or_else(|| value.get("args"))
        .or_else(|| value.get("parameters"))
        .or_else(|| {
            value
                .get("function")
                .and_then(|function| function.get("arguments"))
        })
        .map(|value| match value {
            serde_json::Value::String(text) => text.clone(),
            other => other.to_string(),
        })
        .unwrap_or_else(|| "{}".to_string());
    Some(ToolCall {
        id: format!("scavenged-{index}"),
        name: name.to_string(),
        arguments,
    })
}
