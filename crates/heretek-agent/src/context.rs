use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::Path;

pub struct Zones {
    pub system: String,
    pub topology: String,
}

pub fn zone_hash(text: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    text.hash(&mut hasher);
    hasher.finish()
}

pub fn build_zones(repo_root: &Path) -> Zones {
    Zones {
        system: system_prompt(),
        topology: topology(repo_root),
    }
}

pub fn system_prompt() -> String {
    [
        "You are Heretek, a local coding agent operating inside a workspace.",
        "Every edit you make is verified by deterministic gates: syntax, formatting, types, tests, structural rules, and security scans.",
        "Rules:",
        "1. Read a file before editing it. Never guess file contents.",
        "2. Prefer small, surgical edits over rewrites.",
        "3. Do not narrate. At most one short status line, then act with tools.",
        "4. After you edit, the harness runs the gates and returns errors only if something fails. Fix the reported problems exactly; do not argue with the gates.",
        "5. Do not claim the task is complete unless the gates pass.",
        "6. Work only within the workspace. Absolute paths and parent-directory traversal are rejected.",
        "7. When the task is done, call the finish tool with a short summary.",
    ]
    .join("\n")
}

fn topology(repo_root: &Path) -> String {
    let mut lines = Vec::new();
    lines.push("Workspace topology:".to_string());
    collect(repo_root, repo_root, 0, 2, &mut lines);
    lines.sort();
    lines.truncate(250);

    let manifest = repo_root.join("package.json");
    if let Ok(content) = std::fs::read_to_string(&manifest)
        && let Ok(value) = serde_json::from_str::<serde_json::Value>(&content)
        && let Some(scripts) = value.get("scripts").and_then(|value| value.as_object())
    {
        lines.push("package.json scripts:".to_string());
        for (name, command) in scripts.iter().take(20) {
            let command = command
                .as_str()
                .unwrap_or("")
                .chars()
                .take(120)
                .collect::<String>();
            lines.push(format!("  {name}: {command}"));
        }
    }

    for name in ["AGENTS.md", "CLAUDE.md"] {
        let path = repo_root.join(name);
        if let Ok(content) = std::fs::read_to_string(&path) {
            lines.push(format!("{name} (excerpt):"));
            for line in content.lines().take(60) {
                lines.push(format!("  {line}"));
            }
            break;
        }
    }

    lines.join("\n")
}

fn collect(path: &Path, root: &Path, depth: usize, max_depth: usize, lines: &mut Vec<String>) {
    if depth > max_depth || lines.len() > 400 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(path) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if matches!(
            name.as_str(),
            ".git" | ".heretek" | "node_modules" | "target" | "dist" | "build" | "coverage"
        ) {
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
            format!("  {relative}/")
        } else {
            format!("  {relative}")
        });
        if is_dir {
            collect(&entry_path, root, depth + 1, max_depth, lines);
        }
    }
}

pub fn compact_tool_result(content: &str, budget_chars: usize) -> String {
    if content.chars().count() <= budget_chars {
        return content.to_string();
    }
    let head_budget = budget_chars * 3 / 5;
    let tail_budget = budget_chars.saturating_sub(head_budget);
    let head: String = content.chars().take(head_budget).collect();
    let tail: String = content
        .chars()
        .rev()
        .take(tail_budget)
        .collect::<Vec<char>>()
        .into_iter()
        .rev()
        .collect();
    let omitted = content.chars().count() - budget_chars;
    format!("{head}\n... [{omitted} chars omitted; re-read the file if needed] ...\n{tail}")
}
