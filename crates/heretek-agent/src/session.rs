use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use heretek_core::{GateKind, GateReport, HereConfig, Severity, StageReport, StageStatus};
use heretek_gate::{Baseline, GateContext, Pipeline, ShadowWorkspace, Target, build_pipeline};
use heretek_model::{ChatRequest, Message, ModelClient, Router, Usage};

use crate::context::{auditor_prompt, build_zones, compact_tool_result, zone_hash};
use crate::error::{AgentError, blocking_frames};
use crate::event::{Event, EventWriter};
use crate::repair::ToolCallRepair;
use crate::tools::ToolBox;

#[derive(Debug, Clone, Default)]
pub struct SessionOptions {
    pub lane: Option<String>,
    pub max_turns: Option<u32>,
    pub apply: bool,
    pub audit: bool,
}

#[derive(Debug, Clone)]
pub struct SessionOutcome {
    pub finished: bool,
    pub passed: bool,
    pub verified: bool,
    pub audit_unresolved: bool,
    pub turns: u32,
    pub summary: String,
    pub escalated: bool,
    pub shadow_path: PathBuf,
    pub events_path: PathBuf,
    pub usage: Usage,
}

struct SessionState {
    client: ModelClient,
    lane: String,
    escalated: bool,
    tail: Vec<Message>,
    repair: ToolCallRepair,
    consecutive_failures: u32,
    usage: Usage,
}

const PROTECTED_CONFIG_PREFIXES: &[&str] = &[
    ".heretek.toml",
    "tsconfig",
    "biome.json",
    "biome.jsonc",
    "sgconfig.yml",
    "semgrep.yml",
    ".semgrep.yml",
    "vitest.config.",
    "jest.config.",
];

pub fn run_session(
    repo_root: &std::path::Path,
    task: &str,
    options: &SessionOptions,
) -> Result<SessionOutcome, AgentError> {
    let config = HereConfig::load(repo_root)?;
    let router = Router::new(config.clone());
    let lane = match &options.lane {
        Some(lane) => lane.clone(),
        None => router.default_lane()?.to_string(),
    };
    let client = router.client_for(&lane)?;

    let session_id = format!(
        "session-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_secs())
            .unwrap_or_default()
    );
    let events_dir = repo_root.join(".heretek").join("sessions");
    let mut events = EventWriter::create(&events_dir, &session_id)?;

    let shadow = ShadowWorkspace::create(repo_root, "HEAD", &session_id)?;
    let shadow_root = shadow.path().to_path_buf();
    let pipeline = build_pipeline(repo_root, &config.gate);
    let baseline = capture_baseline(repo_root, &pipeline, &config, &session_id);

    let zones = build_zones(repo_root);
    let zone1_hash = zone_hash(&zones.system);
    let zone2_hash = zone_hash(&zones.topology);
    let system_message = Message::system(format!("{}\n\n{}", zones.system, zones.topology));

    events.write(&Event::SessionStart {
        task: task.to_string(),
        lane: lane.clone(),
        model: client.model().to_string(),
    });

    let max_turns = options.max_turns.unwrap_or(config.agent.max_turns).max(1);
    let deadline = Instant::now() + std::time::Duration::from_secs(config.agent.max_wall_secs);
    let tools = ToolBox::definitions();
    let toolbox = ToolBox::new(&shadow_root);
    let context_cap = config
        .models
        .get(&lane)
        .and_then(|profile| profile.context_tokens)
        .map(|tokens| tokens as usize * 4 / 12)
        .unwrap_or(usize::MAX);
    let tool_budget = (config.agent.tool_result_token_budget.max(200) * 4).min(context_cap);

    let mut state = SessionState {
        client,
        lane,
        escalated: false,
        tail: vec![Message::user(task.to_string())],
        repair: ToolCallRepair::new(),
        consecutive_failures: 0,
        usage: Usage::default(),
    };

    let mut finished = false;
    let mut last_gate_passed = true;
    let mut summary = String::new();
    let mut turn: u32 = 0;
    let mut audit_cycles: u32 = 0;
    let mut pending_objection = false;

    while turn < max_turns {
        if Instant::now() >= deadline {
            events.write(&Event::Error {
                message: "wall-clock budget exhausted".to_string(),
            });
            break;
        }
        turn += 1;
        events.write(&Event::TurnStart {
            turn,
            max_turns,
            zone1_hash,
            zone2_hash,
        });

        let cutoff = state.tail.len();
        compact_tail(&mut state.tail[..cutoff], tool_budget);

        let mut messages = Vec::with_capacity(state.tail.len() + 1);
        messages.push(system_message.clone());
        messages.extend(state.tail.iter().cloned());
        let request = ChatRequest {
            messages,
            tools: tools.clone(),
            max_tokens: None,
            temperature: None,
        };

        let response = match state.client.chat(&request) {
            Ok(response) => response,
            Err(error) => {
                events.write(&Event::Error {
                    message: error.to_string(),
                });
                return Err(error.into());
            }
        };
        accumulate(&mut state.usage, &response.usage);
        events.write(&Event::ModelResponse {
            content_chars: response.content.as_deref().map(str::len).unwrap_or(0),
            tool_calls: response.tool_calls.len(),
            prompt_tokens: response.usage.prompt_tokens,
            completion_tokens: response.usage.completion_tokens,
            cached_tokens: response.usage.cached_tokens,
        });

        let (calls, notes) = state
            .repair
            .repair(response.content.as_deref(), response.tool_calls.clone());
        for (pass, detail) in notes {
            events.write(&Event::Repair {
                pass: pass.to_string(),
                detail,
            });
        }

        state.tail.push(Message {
            role: heretek_model::Role::Assistant,
            content: response.content.clone(),
            tool_calls: calls.clone(),
            tool_call_id: None,
        });

        if calls.is_empty() {
            state.tail.push(Message::user(
                "Use the provided tools to make progress, or call finish when the task is complete.",
            ));
            continue;
        }

        let mut mutated = false;
        for call in &calls {
            if state.repair.is_storm(call) {
                events.write(&Event::StormBreak {
                    name: call.name.clone(),
                });
                state.tail.push(Message::tool_call_response(
                    &call.id,
                    "repeated identical call suppressed; choose a different action",
                ));
                continue;
            }
            events.write(&Event::ToolCall {
                name: call.name.clone(),
                arguments_chars: call.arguments.len(),
            });
            let outcome = toolbox.dispatch(call);
            events.write(&Event::ToolResult {
                name: call.name.clone(),
                is_error: outcome.is_error,
                chars: outcome.content.len(),
            });
            mutated |= outcome.mutated;
            state.tail.push(Message::tool_call_response(
                &call.id,
                outcome.content.clone(),
            ));
            if let Some(summary_text) = outcome.finish {
                finished = true;
                summary = summary_text;
            }
        }

        if Instant::now() >= deadline {
            events.write(&Event::Error {
                message: "wall-clock budget exhausted before verification".to_string(),
            });
            break;
        }

        if mutated {
            let reverted = protect_configs(&shadow_root);
            if !reverted.is_empty() {
                events.write(&Event::ConfigReverted {
                    paths: reverted.clone(),
                });
                state.tail.push(Message::user(format!(
                    "These configuration files are harness-owned and were restored to their original contents: {}. Do not edit them again.",
                    reverted.join(", ")
                )));
            }
            let report = run_gate(repo_root, &pipeline, &shadow_root, &config);
            let report = match &baseline {
                Some(baseline) => baseline.match_new(report),
                None => report,
            };
            last_gate_passed = report.passed;
            events.write(&Event::GateRun {
                passed: report.passed,
                new_diagnostics: report.new_diagnostic_count(),
                blocking_failures: report.blocking_failures,
                warnings: report.warnings.clone(),
            });
            if report.passed {
                state.consecutive_failures = 0;
            } else {
                state.consecutive_failures += 1;
                let frames = blocking_frames(&report);
                events.write(&Event::GateFeedback {
                    frame: frames.clone(),
                });
                let warnings = if report.warnings.is_empty() {
                    String::new()
                } else {
                    format!("\nUnverified stages: {}", report.warnings.join("; "))
                };
                state.tail.push(Message::user(format!(
                    "The gate rejected this change. Fix exactly these problems:\n{frames}{warnings}"
                )));
                maybe_escalate(&mut state, &config, &router, &mut events);
            }
        }

        if finished {
            if mutated && !last_gate_passed {
                finished = false;
                state.tail.push(Message::user(
                    "finish was not accepted because the gate is failing. Fix the reported problems, then call finish again.",
                ));
            } else if (config.agent.auditor || options.audit)
                && audit_cycles < config.agent.auditor_max_cycles
            {
                match run_auditor(&shadow_root, &config, &state.client, task, &mut events) {
                    AuditResult::Objection(frame) => {
                        audit_cycles += 1;
                        finished = false;
                        pending_objection = true;
                        events.write(&Event::AuditObjection {
                            frame: frame.clone(),
                        });
                        state.tail.push(Message::user(format!(
                            "The auditor proved a defect with a failing test. Fix the code so the test passes; do not delete, weaken, or move the test:\n{frame}"
                        )));
                    }
                    AuditResult::Cleared => {
                        pending_objection = false;
                        events.write(&Event::AuditCleared);
                        break;
                    }
                    AuditResult::Unavailable(reason) => {
                        pending_objection = true;
                        events.write(&Event::AuditUnavailable {
                            reason: reason.clone(),
                        });
                        break;
                    }
                }
            } else {
                if pending_objection {
                    events.write(&Event::AuditUnavailable {
                        reason: format!("objection unresolved after {audit_cycles} audit cycle(s)"),
                    });
                }
                break;
            }
        }
    }

    let final_report = run_gate(repo_root, &pipeline, &shadow_root, &config);
    let final_report = match &baseline {
        Some(baseline) => baseline.match_new(final_report),
        None => final_report,
    };
    let changed = heretek_gate::files::changed_files(&GateContext::new(
        repo_root,
        Target::Worktree(shadow_root.clone()),
    ))
    .unwrap_or_default();
    let verification = heretek_gate::verification(&final_report, &changed);
    let passed = final_report.passed
        && verification != heretek_gate::Verification::Unverified
        && !pending_objection;
    let verified = final_report
        .stages
        .iter()
        .any(|stage| stage.kind == GateKind::Blocking && stage.status == StageStatus::Passed);

    events.write(&Event::Finished {
        summary: summary.clone(),
        passed,
        turns: turn,
    });

    if let Some(error) = events.take_error() {
        eprintln!("heretek: event stream write failed: {error}");
    }

    let shadow_path;
    if options.apply && passed && finished {
        match shadow.apply_to(repo_root) {
            Ok(()) => {
                shadow_path = shadow.persist();
                heretek_gate::discard(repo_root, &shadow_path).ok();
            }
            Err(error) => {
                let path = shadow.persist();
                return Err(AgentError::Session(format!(
                    "apply failed: {error}; the shadow was preserved at {}",
                    path.display()
                )));
            }
        }
    } else {
        shadow_path = shadow.persist();
    }

    Ok(SessionOutcome {
        finished,
        passed,
        verified,
        audit_unresolved: pending_objection,
        turns: turn,
        summary,
        escalated: state.escalated,
        shadow_path,
        events_path: events.path().to_path_buf(),
        usage: state.usage,
    })
}

enum AuditResult {
    Cleared,
    Objection(String),
    Unavailable(String),
}

fn run_auditor(
    shadow_root: &std::path::Path,
    config: &HereConfig,
    client: &ModelClient,
    task: &str,
    events: &mut EventWriter,
) -> AuditResult {
    let audit_dir = shadow_root.join(".heretek-audit");
    let diff =
        heretek_gate::files::git_output(shadow_root, &["diff".to_string(), "HEAD".to_string()])
            .unwrap_or_default();
    let diff: String = diff.chars().take(12_000).collect();

    let request = ChatRequest {
        messages: vec![
            Message::system(auditor_prompt()),
            Message::user(format!(
                "Original task:\n{task}\n\nCandidate diff (truncated to 12k chars):\n{diff}"
            )),
        ],
        tools: ToolBox::definitions(),
        max_tokens: None,
        temperature: None,
    };
    let response = match client.chat(&request) {
        Ok(response) => response,
        Err(error) => {
            events.write(&Event::AuditUnavailable {
                reason: error.to_string(),
            });
            return AuditResult::Unavailable(error.to_string());
        }
    };
    events.write(&Event::ModelResponse {
        content_chars: response.content.as_deref().map(str::len).unwrap_or(0),
        tool_calls: response.tool_calls.len(),
        prompt_tokens: response.usage.prompt_tokens,
        completion_tokens: response.usage.completion_tokens,
        cached_tokens: response.usage.cached_tokens,
    });

    let toolbox = ToolBox::auditor(shadow_root);
    let (calls, _) =
        ToolCallRepair::new().repair(response.content.as_deref(), response.tool_calls.clone());
    for call in calls {
        match call.name.as_str() {
            "read_file" | "search" | "list_dir" | "finish" => {
                let _ = toolbox.dispatch(&call);
            }
            "write_file" => {
                let allowed = serde_json::from_str::<serde_json::Value>(&call.arguments)
                    .ok()
                    .and_then(|value| {
                        value
                            .get("path")
                            .and_then(|path| path.as_str())
                            .map(str::to_string)
                    })
                    .map(|path| path.starts_with(".heretek-audit/"))
                    .unwrap_or(false);
                if !allowed {
                    events.write(&Event::AuditUnavailable {
                        reason: "auditor attempted to write outside .heretek-audit/".to_string(),
                    });
                    continue;
                }
                let _ = toolbox.dispatch(&call);
            }
            other => {
                events.write(&Event::AuditUnavailable {
                    reason: format!("auditor called disallowed tool '{other}'"),
                });
            }
        }
    }

    if !audit_dir.exists() {
        return AuditResult::Cleared;
    }
    let mut files: Vec<PathBuf> = std::fs::read_dir(&audit_dir)
        .map(|entries| entries.flatten().map(|entry| entry.path()).collect())
        .unwrap_or_default();
    files.retain(|path| path.extension().map(|ext| ext == "mjs").unwrap_or(false));
    if files.is_empty() {
        return AuditResult::Cleared;
    }
    files.sort();

    let spec = heretek_gate::process::ProcessSpec::new("node", shadow_root)
        .args({
            let mut args = vec![
                "--permission".to_string(),
                format!("--allow-fs-read={}", shadow_root.display()),
                format!("--allow-fs-write={}", shadow_root.display()),
                "--test".to_string(),
            ];
            args.extend(files.iter().map(|file| file.display().to_string()));
            args
        })
        .timeout(std::time::Duration::from_secs(
            config.gate.stage_timeout_secs.min(300),
        ))
        .allow_network(false);
    let output = match heretek_gate::process::run(&spec) {
        Ok(output) => output,
        Err(error) => {
            let reason = format!("cannot run node --test: {error}");
            events.write(&Event::AuditUnavailable {
                reason: reason.clone(),
            });
            return AuditResult::Unavailable(reason);
        }
    };
    if output.success() {
        let _ = std::fs::remove_dir_all(&audit_dir);
        return AuditResult::Cleared;
    }
    let frame = output.combined();
    AuditResult::Objection(crate::context::compact_tool_result(&frame, 1_200))
}

fn protect_configs(shadow_root: &std::path::Path) -> Vec<String> {
    let output = std::process::Command::new("git")
        .args(["diff", "--name-only", "HEAD"])
        .current_dir(shadow_root)
        .output();
    let Ok(output) = output else {
        return Vec::new();
    };
    let changed = String::from_utf8_lossy(&output.stdout);
    let mut reverted = Vec::new();
    for file in changed.lines() {
        let name = file.rsplit('/').next().unwrap_or(file);
        let protected = PROTECTED_CONFIG_PREFIXES
            .iter()
            .any(|prefix| name.starts_with(prefix));
        if !protected {
            continue;
        }
        let checkout = std::process::Command::new("git")
            .args(["checkout", "HEAD", "--", file])
            .current_dir(shadow_root)
            .output();
        if checkout
            .map(|result| result.status.success())
            .unwrap_or(false)
        {
            reverted.push(file.to_string());
        }
    }
    reverted
}

fn maybe_escalate(
    state: &mut SessionState,
    config: &HereConfig,
    router: &Router,
    events: &mut EventWriter,
) {
    if !config.agent.escalate || state.escalated || state.consecutive_failures < 2 {
        return;
    }
    if state.lane == "deep" || !config.models.contains_key("deep") {
        return;
    }
    match router.client_for("deep") {
        Ok(client) => {
            let from = state.lane.clone();
            state.client = client;
            state.lane = "deep".to_string();
            state.escalated = true;
            events.write(&Event::Escalation {
                from,
                to: "deep".to_string(),
                reason: format!("{} consecutive gate failures", state.consecutive_failures),
            });
        }
        Err(error) => {
            events.write(&Event::Error {
                message: format!("escalation to deep lane failed: {error}"),
            });
        }
    }
}

fn capture_baseline(
    repo_root: &std::path::Path,
    pipeline: &Pipeline,
    config: &HereConfig,
    session_id: &str,
) -> Option<Baseline> {
    let all_files = all_files(repo_root).ok()?;
    if all_files.is_empty() {
        return None;
    }
    let ctx = GateContext::new(repo_root, Target::Staged)
        .with_config(Arc::new(config.gate.clone()))
        .with_files(all_files)
        .with_fix(false);
    Baseline::capture(pipeline, &ctx, session_id).ok()
}

fn all_files(repo_root: &std::path::Path) -> Result<Vec<String>, heretek_gate::GateError> {
    let ctx = GateContext::new(repo_root, Target::Worktree(repo_root.to_path_buf()));
    heretek_gate::files::changed_files(&ctx)
}

fn run_gate(
    repo_root: &std::path::Path,
    pipeline: &Pipeline,
    shadow_root: &std::path::Path,
    config: &HereConfig,
) -> GateReport {
    let ctx = GateContext::new(repo_root, Target::Worktree(shadow_root.to_path_buf()))
        .with_config(Arc::new(config.gate.clone()))
        .with_baseline(Some("HEAD".to_string()))
        .with_fix(true);
    let files = match heretek_gate::files::changed_files(&ctx) {
        Ok(files) => files,
        Err(error) => return setup_failure(&ctx, &error.to_string()),
    };
    let ctx = ctx.with_files(files);
    pipeline.run(&ctx)
}

fn setup_failure(ctx: &GateContext, message: &str) -> GateReport {
    use heretek_core::Diagnostic;
    heretek_core::GateReport::from_stages(
        ctx.target.describe(),
        vec![StageReport::failed(
            "setup",
            GateKind::Blocking,
            0,
            vec![Diagnostic::new(
                "setup",
                Severity::Error,
                "",
                0,
                0,
                format!("cannot determine changed files: {message}"),
            )],
        )],
    )
}

fn accumulate(total: &mut Usage, usage: &Usage) {
    total.prompt_tokens += usage.prompt_tokens;
    total.completion_tokens += usage.completion_tokens;
    total.total_tokens += usage.total_tokens;
    if let Some(cached) = usage.cached_tokens {
        *total.cached_tokens.get_or_insert(0) += cached;
    }
}

fn compact_tail(tail: &mut [Message], budget: usize) {
    for message in tail.iter_mut() {
        if message.role != heretek_model::Role::Tool {
            continue;
        }
        if let Some(content) = &message.content
            && content.chars().count() > budget
        {
            message.content = Some(compact_tool_result(content, budget));
        }
    }
}
