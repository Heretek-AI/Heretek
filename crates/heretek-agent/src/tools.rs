use std::path::{Component, Path, PathBuf};

use heretek_model::{ToolCall, ToolDefinition};
use serde_json::json;

pub struct ToolBox {
    root: PathBuf,
    canonical_root: PathBuf,
}

pub struct ToolOutcome {
    pub content: String,
    pub is_error: bool,
    pub mutated: bool,
    pub finish: Option<String>,
}

const SKIP_DIRS: &[&str] = &[
    ".git",
    ".heretek",
    "node_modules",
    "target",
    "dist",
    "build",
    "vendor",
    "coverage",
];

impl ToolBox {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        let root = root.into();
        let canonical_root = root.canonicalize().unwrap_or_else(|_| root.clone());
        Self {
            root,
            canonical_root,
        }
    }

    pub fn definitions() -> Vec<ToolDefinition> {
        vec![
            ToolDefinition {
                name: "read_file".to_string(),
                description: "Read a file from the workspace. Returns numbered lines.".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": { "path": { "type": "string" } },
                    "required": ["path"]
                }),
            },
            ToolDefinition {
                name: "write_file".to_string(),
                description: "Create or replace a file with the given content.".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "path": { "type": "string" },
                        "content": { "type": "string" }
                    },
                    "required": ["path", "content"]
                }),
            },
            ToolDefinition {
                name: "edit_file".to_string(),
                description: "Replace an exact search string with a replacement. The search must match exactly once.".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "path": { "type": "string" },
                        "search": { "type": "string" },
                        "replace": { "type": "string" }
                    },
                    "required": ["path", "search", "replace"]
                }),
            },
            ToolDefinition {
                name: "list_dir".to_string(),
                description: "List directory entries, two levels deep.".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": { "path": { "type": "string" } },
                    "required": []
                }),
            },
            ToolDefinition {
                name: "search".to_string(),
                description: "Search file contents for a case-insensitive substring. Returns file:line matches.".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": { "query": { "type": "string" } },
                    "required": ["query"]
                }),
            },
            ToolDefinition {
                name: "finish".to_string(),
                description: "Declare the task complete with a one-paragraph summary.".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": { "summary": { "type": "string" } },
                    "required": ["summary"]
                }),
            },
        ]
    }

    pub fn dispatch(&self, call: &ToolCall) -> ToolOutcome {
        let arguments: serde_json::Value = match serde_json::from_str(&call.arguments) {
            Ok(value) => value,
            Err(error) => {
                return ToolOutcome {
                    content: format!("invalid arguments JSON: {error}"),
                    is_error: true,
                    mutated: false,
                    finish: None,
                };
            }
        };
        match call.name.as_str() {
            "read_file" => self.read_file(&arguments),
            "write_file" => self.write_file(&arguments),
            "edit_file" => self.edit_file(&arguments),
            "list_dir" => self.list_dir(&arguments),
            "search" => self.search(&arguments),
            "finish" => {
                let summary = arguments
                    .get("summary")
                    .and_then(|value| value.as_str())
                    .unwrap_or("finished")
                    .to_string();
                ToolOutcome {
                    content: "finished".to_string(),
                    is_error: false,
                    mutated: false,
                    finish: Some(summary),
                }
            }
            other => ToolOutcome {
                content: format!("unknown tool '{other}'"),
                is_error: true,
                mutated: false,
                finish: None,
            },
        }
    }

    fn read_file(&self, arguments: &serde_json::Value) -> ToolOutcome {
        let Some(path) = arguments.get("path").and_then(|value| value.as_str()) else {
            return error("missing 'path'");
        };
        let path = match self.safe_path(path) {
            Ok(path) => path,
            Err(message) => return error(&message),
        };
        match std::fs::read_to_string(&path) {
            Ok(content) => {
                let numbered: Vec<String> = content
                    .lines()
                    .enumerate()
                    .map(|(index, line)| format!("{:>5} | {line}", index + 1))
                    .collect();
                ToolOutcome {
                    content: numbered.join("\n"),
                    is_error: false,
                    mutated: false,
                    finish: None,
                }
            }
            Err(io_error) => error(&format!("cannot read file: {io_error}")),
        }
    }

    fn write_file(&self, arguments: &serde_json::Value) -> ToolOutcome {
        let Some(path) = arguments.get("path").and_then(|value| value.as_str()) else {
            return error("missing 'path'");
        };
        let Some(content) = arguments.get("content").and_then(|value| value.as_str()) else {
            return error("missing 'content'");
        };
        let path = match self.safe_path(path) {
            Ok(path) => path,
            Err(message) => return error(&message),
        };
        if let Some(parent) = path.parent()
            && let Err(io_error) = std::fs::create_dir_all(parent)
        {
            return error(&format!("cannot create directory: {io_error}"));
        }
        match std::fs::write(&path, content) {
            Ok(()) => ToolOutcome {
                content: format!("wrote {} bytes", content.len()),
                is_error: false,
                mutated: true,
                finish: None,
            },
            Err(io_error) => error(&format!("cannot write file: {io_error}")),
        }
    }

    fn edit_file(&self, arguments: &serde_json::Value) -> ToolOutcome {
        let Some(path) = arguments.get("path").and_then(|value| value.as_str()) else {
            return error("missing 'path'");
        };
        let Some(search) = arguments.get("search").and_then(|value| value.as_str()) else {
            return error("missing 'search'");
        };
        let Some(replace) = arguments.get("replace").and_then(|value| value.as_str()) else {
            return error("missing 'replace'");
        };
        if search.is_empty() {
            return error("'search' must not be empty");
        }
        let path = match self.safe_path(path) {
            Ok(path) => path,
            Err(message) => return error(&message),
        };
        let content = match std::fs::read_to_string(&path) {
            Ok(content) => content,
            Err(io_error) => return error(&format!("cannot read file: {io_error}")),
        };
        let matches = content.matches(search).count();
        if matches != 1 {
            return error(&format!(
                "'search' must match exactly once, found {matches} occurrence(s)"
            ));
        }
        let updated = content.replacen(search, replace, 1);
        match std::fs::write(&path, updated) {
            Ok(()) => ToolOutcome {
                content: "edited file".to_string(),
                is_error: false,
                mutated: true,
                finish: None,
            },
            Err(io_error) => error(&format!("cannot write file: {io_error}")),
        }
    }

    fn list_dir(&self, arguments: &serde_json::Value) -> ToolOutcome {
        let relative = arguments
            .get("path")
            .and_then(|value| value.as_str())
            .unwrap_or(".");
        let path = match self.safe_path(relative) {
            Ok(path) => path,
            Err(message) => return error(&message),
        };
        let mut lines = Vec::new();
        collect_entries(&path, &self.root, 0, 2, &mut lines);
        if lines.is_empty() {
            return error("directory is empty or unreadable");
        }
        lines.sort();
        lines.truncate(400);
        ToolOutcome {
            content: lines.join("\n"),
            is_error: false,
            mutated: false,
            finish: None,
        }
    }

    fn search(&self, arguments: &serde_json::Value) -> ToolOutcome {
        let Some(query) = arguments.get("query").and_then(|value| value.as_str()) else {
            return error("missing 'query'");
        };
        if query.is_empty() {
            return error("'query' must not be empty");
        }
        let needle = query.to_lowercase();
        let mut matches = Vec::new();
        search_dir(&self.root, &needle, &mut matches, 12);
        if matches.is_empty() {
            return ToolOutcome {
                content: format!("no matches for '{query}'"),
                is_error: false,
                mutated: false,
                finish: None,
            };
        }
        matches.truncate(200);
        ToolOutcome {
            content: matches.join("\n"),
            is_error: false,
            mutated: false,
            finish: None,
        }
    }

    fn safe_path(&self, user: &str) -> Result<PathBuf, String> {
        let candidate = Path::new(user);
        if candidate.is_absolute() {
            return Err("absolute paths are not allowed".to_string());
        }
        let mut normalized = PathBuf::new();
        for component in candidate.components() {
            match component {
                Component::Normal(part) => normalized.push(part),
                Component::CurDir => {}
                _ => return Err("path must not escape the workspace".to_string()),
            }
        }
        let joined = self.root.join(&normalized);
        if !joined.starts_with(&self.root) {
            return Err("path escapes the workspace".to_string());
        }
        let existing = if joined.exists() {
            joined.canonicalize().ok()
        } else {
            joined
                .parent()
                .and_then(|parent| parent.canonicalize().ok())
        };
        if let Some(existing) = existing
            && !existing.starts_with(&self.canonical_root)
        {
            return Err("path resolves outside the workspace".to_string());
        }
        Ok(joined)
    }
}

fn error(message: &str) -> ToolOutcome {
    ToolOutcome {
        content: message.to_string(),
        is_error: true,
        mutated: false,
        finish: None,
    }
}

fn collect_entries(
    path: &Path,
    root: &Path,
    depth: usize,
    max_depth: usize,
    lines: &mut Vec<String>,
) {
    if depth > max_depth {
        return;
    }
    let Ok(entries) = std::fs::read_dir(path) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if SKIP_DIRS.contains(&name.as_str()) {
            continue;
        }
        let entry_path = entry.path();
        let relative = entry_path
            .strip_prefix(root)
            .unwrap_or(&entry_path)
            .display()
            .to_string();
        let is_dir = entry.file_type().map(|kind| kind.is_dir()).unwrap_or(false);
        lines.push(if is_dir {
            format!("{relative}/")
        } else {
            relative
        });
        if is_dir {
            collect_entries(&entry_path, root, depth + 1, max_depth, lines);
        }
    }
}

fn search_dir(path: &Path, needle: &str, matches: &mut Vec<String>, max_depth: usize) {
    if matches.len() >= 400 || max_depth == 0 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(path) else {
        return;
    };
    for entry in entries.flatten() {
        if matches.len() >= 400 {
            return;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        if SKIP_DIRS.contains(&name.as_str()) {
            continue;
        }
        let entry_path = entry.path();
        let is_dir = entry.file_type().map(|kind| kind.is_dir()).unwrap_or(false);
        if is_dir {
            search_dir(&entry_path, needle, matches, max_depth - 1);
            continue;
        }
        let Ok(metadata) = entry.metadata() else {
            continue;
        };
        if metadata.len() > 2_000_000 {
            continue;
        }
        let Ok(content) = std::fs::read_to_string(&entry_path) else {
            continue;
        };
        for (index, line) in content.lines().enumerate() {
            if line.to_lowercase().contains(needle) {
                let clipped: String = line.chars().take(200).collect();
                matches.push(format!(
                    "{}:{}: {}",
                    entry_path.display(),
                    index + 1,
                    clipped
                ));
                if matches.len() >= 400 {
                    return;
                }
            }
        }
    }
}
