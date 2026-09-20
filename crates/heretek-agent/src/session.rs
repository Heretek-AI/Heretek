use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use heretek_core::HereConfig;
use heretek_gate::{Baseline, GateContext, Pipeline, ShadowWorkspace, Target, build_pipeline};
use heretek_model::{ChatRequest, Message, ModelClient, Router, Usage};

use crate::context::{build_zones, compact_tool_result, zone_hash};
use crate::error::{AgentError, blocking_frames};
use crate::event::{Event, EventWriter};
use crate::repair::ToolCallRepair;
use crate::tools::ToolBox;

#[derive(Debug, Clone, Default)]
pub struct SessionOptions {
    pub lane: Option<String>,
    pub max_turns: Option<u32>,
    pub apply: bool,
}

#[derive(Debug, Clone)]
pub struct SessionOutcome {
    pub finished: bool,
    pub passed: bool,
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
    let tool_budget = config.agent.tool_result_token_budget.max(200) * 4;

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
    let mut summary = String::new();
    let mut turn: u32 = 0;

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

        compact_tail(&mut state.tail, tool_budget);

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

        if mutated {
            let report = run_gate(repo_root, &pipeline, &shadow_root, &config);
            let report = match &baseline {
                Some(baseline) => baseline.match_new(report),
                None => report,
            };
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
            break;
        }
    }

    let final_report = run_gate(repo_root, &pipeline, &shadow_root, &config);
    let final_report = match &baseline {
        Some(baseline) => baseline.match_new(final_report),
        None => final_report,
    };
    let passed = final_report.passed;

    events.write(&Event::Finished {
        summary: summary.clone(),
        passed,
        turns: turn,
    });

    if options.apply && passed {
        shadow.apply_to(repo_root)?;
        shadow.cleanup();
    } else {
        shadow.persist();
    }

    Ok(SessionOutcome {
        finished,
        passed,
        turns: turn,
        summary,
        escalated: state.escalated,
        shadow_path: shadow_root,
        events_path: events.path().to_path_buf(),
        usage: state.usage,
    })
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
) -> heretek_core::GateReport {
    let ctx = GateContext::new(repo_root, Target::Worktree(shadow_root.to_path_buf()))
        .with_config(Arc::new(config.gate.clone()))
        .with_baseline(Some("HEAD".to_string()))
        .with_fix(true);
    let files = heretek_gate::files::changed_files(&ctx).unwrap_or_default();
    let ctx = ctx.with_files(files);
    pipeline.run(&ctx)
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
    if tail.len() < 3 {
        return;
    }
    let last_index = tail.len() - 1;
    for message in tail.iter_mut().take(last_index) {
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
