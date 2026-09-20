use std::path::Path;

use heretek_core::{Diagnostic, Severity};

pub const TS_EXTENSIONS: &[&str] = &[".ts", ".tsx", ".js", ".jsx", ".mjs", ".cjs", ".mts", ".cts"];

pub fn is_ts(path: &str) -> bool {
    crate::files::is_source_file(path, TS_EXTENSIONS)
}

pub fn diagnostic(
    gate: &str,
    severity: Severity,
    file: impl Into<String>,
    line: u32,
    column: u32,
    message: impl Into<String>,
) -> Diagnostic {
    Diagnostic::new(gate, severity, file, line, column, message)
}

pub fn truncate(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let head: String = text.chars().take(max.saturating_sub(3)).collect();
    format!("{head}...")
}

pub fn parse_line_column(text: &str) -> (u32, u32) {
    let mut parts = text.split(':');
    let line = parts
        .next()
        .and_then(|v| v.trim().parse().ok())
        .unwrap_or(0);
    let column = parts
        .next()
        .and_then(|v| v.trim().parse().ok())
        .unwrap_or(0);
    (line, column)
}

pub fn ensure_within(root: &Path, candidate: &Path) -> bool {
    candidate.starts_with(root)
}
