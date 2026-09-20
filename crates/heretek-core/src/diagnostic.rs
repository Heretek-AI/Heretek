use serde::{Deserialize, Serialize};

pub const DIAGNOSTIC_SCHEMA: &str = "heretek.diagnostic/1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Error,
    Warning,
    Note,
}

impl Severity {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Error => "error",
            Self::Warning => "warning",
            Self::Note => "note",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GateKind {
    Blocking,
    Advisory,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Diagnostic {
    pub schema: String,
    pub gate: String,
    pub severity: Severity,
    pub file: String,
    pub line: u32,
    pub column: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub context: Vec<String>,
    pub is_new: bool,
}

pub fn sanitize(text: &str) -> String {
    text.chars()
        .map(|character| {
            if character.is_control() && character != '\n' && character != '\t' {
                ' '
            } else {
                character
            }
        })
        .collect()
}

impl Diagnostic {
    pub fn new(
        gate: impl Into<String>,
        severity: Severity,
        file: impl Into<String>,
        line: u32,
        column: u32,
        message: impl Into<String>,
    ) -> Self {
        Self {
            schema: DIAGNOSTIC_SCHEMA.to_string(),
            gate: sanitize(&gate.into()),
            severity,
            file: sanitize(&file.into()),
            line,
            column,
            code: None,
            message: sanitize(&message.into()),
            hint: None,
            context: Vec::new(),
            is_new: true,
        }
    }

    pub fn with_hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = Some(sanitize(&hint.into()));
        self
    }

    pub fn with_context(mut self, context: Vec<String>) -> Self {
        self.context = context.into_iter().map(|line| sanitize(&line)).collect();
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diagnostic_round_trips_through_json() {
        let diagnostic = Diagnostic::new(
            "typecheck",
            Severity::Error,
            "src/auth/session.ts",
            84,
            22,
            "Argument of type 'string | null' is not assignable to 'string'.",
        );
        let json = serde_json::to_string(&diagnostic).expect("serialize");
        let back: Diagnostic = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(diagnostic, back);
        assert_eq!(back.schema, DIAGNOSTIC_SCHEMA);
    }
}
