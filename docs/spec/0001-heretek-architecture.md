# Spec 0001: Heretek architecture

**Status:** implemented (alpha); tracked by issues #1-#13
**Date:** 2026-09-20
**Supersedes:** `docs/research/inputs/ai-coding-harness-architecture-design.pdf`
(see the fact-check in `docs/research/2026-09-ai-coding-harness-landscape.md`, section 15)

---

## 1. Problem and positioning

Frontier coding agents cost $20-$200 per seat per month and require sending source code
to third parties. Local open-weight models in the 30-35B class are now capable enough to
write plausible code, but they fail at long-horizon autonomy: instruction drift, context
pollution, malformed tool calls, and an inability to reliably catch their own logic
errors.

Heretek is a local-first coding harness that closes that gap with deterministic
verification. The model proposes; syntax, type, test, lint, structural, and security
gates dispose. Correctness is enforced by the harness, not by model self-reflection.

Two consequences shape every decision below:

1. **Local models are the prime LLM.** Cloud endpoints may be connected (any
   OpenAI-compatible provider), but nothing in the critical path may require one.
2. **False rejections are the primary failure mode.** A gate that blocks a valid patch
   destroys more value than a gate that misses a bug. The evaluation contract measures
   false rejects explicitly.

## 2. Settled decisions

| # | Decision | ADR |
| --- | --- | --- |
| 1 | Local models as prime LLM; hardware targets: 24GB GPU and Strix Halo 128GB | [0001](../adr/0001-local-models-as-prime-llm.md) |
| 2 | Hybrid sequencing: gate engine first, own agent loop second | [0002](../adr/0002-gate-engine-first.md) |
| 3 | Rust workspace; single static binary | [0003](../adr/0003-rust-core.md) |
| 4 | OpenAI-compatible model interface with named lanes and a router | [0004](../adr/0004-openai-compatible-model-layer.md) |
| 5 | Deterministic `Decide` heuristics in v1; Jev-style adapters optional later | [0005](../adr/0005-deterministic-decide.md) |
| 6 | Dual MIT/Apache-2.0; consume tools, reimplement patterns, no whole-harness forks | [0006](../adr/0006-oss-strategy-and-donors.md) |
| 7 | No cloud LLM in the critical path; network limited to downloads and OSV | [0007](../adr/0007-no-cloud-critical-path.md) |
| 8 | Git worktrees for shadow state; copy fallback for non-git trees | [0008](../adr/0008-git-worktree-shadow-state.md) |

## 3. Goals

- G1: Gate a proposed patch deterministically and return a structured, bounded report.
- G2: Run the same gates as a git hook (lefthook-compatible) and as an MCP tool.
- G3: Drive a local model through a cache-disciplined loop that respects a VRAM budget.
- G4: Route tasks across model lanes (`fast`, `deep`, `micro`) by measured heuristics.
- G5: Prove or falsify the thesis on a pre-registered evaluation contract.
- G6: Stay integrable: MCP, Agent Skills, `AGENTS.md`, OpenAI-compatible HTTP.

## 4. Non-goals

- TUI parity with OpenCode/Claude Code in v0. CLI plus machine-readable output comes first.
- Provider-matrix parity. Any OpenAI-compatible endpoint works; native SDKs do not.
- Cloud hosting, hosted inference, or telemetry of any kind.
- Desktop GUI automation and browser automation in v0 (these are MCP surfaces later).
- Plugin marketplaces and private extension formats. Use MCP and Agent Plugins.
- Forking an entire upstream harness. Borrow patterns with attribution; depend on tools.

## 5. System overview

```
                       +------------------+
   user / CI / hook -->|  heretek-cli     |
                       |  gate|run|mcp|init
                       +--------+---------+
                                |
              +-----------------+------------------+
              |                                    |
      +-------v--------+                  +--------v--------+
      |  heretek-gate  |                  |  agent loop     |
      |  pipeline      |                  |  (phase 2)      |
      |  stages        |                  +--------+--------+
      |  shadow state  |                           |
      +-------+--------+                  +--------v--------+
              |                           |  model router   |
      subprocesses:                       | fast|deep|micro |
      tsgo, biome, vitest,                +--------+--------+
      ast-grep, semgrep, knip,                     |
      osv-scanner, git                    OpenAI-compatible HTTP
                                          (llama-server, vLLM, SGLang, Ollama)
```

The gate engine has no dependency on the loop and the loop has no dependency on the
gate engine's internals. Both are exposed by the CLI. The loop calls gates; the gates
never call the model.

## 6. Gate engine

### 6.1 Stage contract

Every stage implements:

```rust
trait Stage {
    fn id(&self) -> StageId;
    fn kind(&self) -> StageKind;          // Blocking | Advisory
    fn applies_to(&self, target: &Target) -> bool;
    fn run(&self, ctx: &GateContext) -> Result<StageOutcome, StageError>;
}
```

`StageOutcome` carries zero or more `Diagnostic`s. A blocking stage with diagnostics
fails the pipeline; an advisory stage reports without failing.

### 6.2 Stage set (v0, JS/TS first)

| Order | Stage | Tool | Kind | Notes |
| --- | --- | --- | --- | --- |
| 1 | Syntax | tree-sitter grammars | Blocking | ERROR/MISSING node detection per changed file |
| 2 | Format | biome (oxfmt later) | Auto-fix | Rewrites on the shadow tree only |
| 3 | Typecheck | `tsgo --noEmit` (tsc fallback) | Blocking | Diff-aware: only new diagnostics block |
| 4 | Tests | vitest/jest affected | Blocking | Skipped (labeled "untested") when the repo has no suite |
| 5 | Structural | ast-grep rules | Blocking | Repo-defined anti-patterns and boundaries |
| 6 | Secrets | semgrep secrets / gitleaks | Advisory | Promoted to blocking after false-reject measurement |
| 7 | SAST | semgrep OSS | Advisory | Same promotion path |
| 8 | Dead code + deps | knip + osv-scanner | Advisory | Unused exports/deps; known-malicious and vulnerable versions |

Python and Rust stages arrive after the JS/TS pipeline passes the evaluation contract.

### 6.3 Diff-aware blocking

Gates compare diagnostics against a baseline captured from HEAD before the patch. Only
new findings block. Pre-existing debt is reported in a separate `baseline` section and
never fails a run. This is a product invariant (see AGENTS.md).

### 6.4 Error frames

Every diagnostic is normalized into a bounded frame:

```json
{
  "schema": "heretek.diagnostic/1",
  "gate": "typecheck",
  "severity": "error",
  "file": "src/auth/session.ts",
  "line": 84,
  "column": 22,
  "code": "TS2345",
  "message": "Argument of type 'string | null' is not assignable to parameter of type 'string'.",
  "hint": "Narrow before the call or widen the parameter.",
  "context": ["82 | ...", "83 | ...", "84 | ..."],
  "is_new": true
}
```

Rules: no terminal escape codes; at most 3 lines of context per side; messages
truncated at a fixed budget; deterministic ordering. The frame is the only diagnostic
shape the loop ever sees.

## 7. Shadow workspace

- The loop edits a **git worktree** at the repo's HEAD, created per task under
  `.heretek/worktrees/<id>` (detached HEAD).
- Before each turn, the harness commits a snapshot on a private ref under
  `refs/heretek/turns/<id>/<n>` so rollback is a `reset --hard` on the shadow ref.
- A turn passes when the gate pipeline reports no new blocking diagnostics. Only then
  does the harness apply the shadow diff to the developer's working tree.
- Non-git trees fall back to a full copy under `.heretek/shadow/`; rollback is a copy
  restore. No VCS features are assumed beyond the presence of a git repository.
- In hook mode (`heretek gate --staged`) there is no loop and no shadow; gates run
  against the index and the developer's worktree is never modified.

## 8. Model layer

### 8.1 Interface

OpenAI-compatible HTTP is the only model interface. Configuration names a base URL,
model id, optional API key env var, and generation parameters per lane.

### 8.2 Lanes

| Lane | Target | Default | Use |
| --- | --- | --- | --- |
| `fast` | 24GB GPU / Strix Halo | Qwen3.6-35B-A3B (Q4_K_XL or Q8) | Edit loop, routine turns |
| `deep` | Strix Halo 128GB | Qwen3.6-122B-A10B (Q4) | Failed-turn escalation, review, hard tasks |
| `micro` | CPU/shared GPU | 4B-9B class | `Decide` primitives, classification |

Profiles are data, not code. The exact model ids live in a config file so new weights
can be adopted without a release.

### 8.3 Context tiers

| Host | Working set | KV |
| --- | --- | --- |
| 24GB GPU | 16-32k | Q4/Q8 quantized |
| Strix Halo | 128-262k | Q8 K / Q4 V |

The prompt is laid out in three zones: immutable system core, semi-static topology
(file tree, manifests, rules), and a mutable working tail (task, diffs, gate frames).
Zone stability is verified per engine because prefix-cache behavior differs between
llama.cpp, vLLM, and SGLang. Cache hit rate is a first-class metric.

---

## 9. Agent loop (phase 2)

The loop is deliberately boring:

1. Assemble context from the three zones.
2. Ask the model for the next action with a JSON-schema-constrained tool call
   (grammar where the engine supports it; strict parse plus repair otherwise).
3. Execute the action in the shadow workspace (read tools run freely; write tools are
   gated by read-before-write and path sandboxing).
4. After each write batch, run the gate pipeline. Feed back only error frames.
5. Repair passes on the model output before dispatch: flatten complex tool schemas,
   scavenge tool calls emitted inside reasoning text, repair truncated JSON, detect
   repeated identical calls and inject a reflection turn.
6. Turn budget and wall-clock budget are hard caps. Escalation from `fast` to `deep` is
   announced, never silent.

Edit formats in v0: whole-file writes and SEARCH/REPLACE blocks with byte-exact match
enforcement. Formatting is a gate, not a model responsibility.

## 10. Decide interface

```rust
trait Decider {
    fn risky_change(&self, change: &ChangeSummary) -> Decision<RiskTier>;
    fn lane_for(&self, task: &TaskSpec, risk: RiskTier) -> Decision<Lane>;
    fn gate_strictness(&self, change: &ChangeSummary) -> Decision<Strictness>;
}
```

`Decision<T>` carries the value plus a reason code. The v1 implementation is
deterministic heuristics: paths touched (auth, migrations, CI, lockfiles), change
size, test coverage delta, dependency additions, and history of gate failures. The
trait exists so a logit-readout or Jev-style model can be evaluated behind it later,
with calibration measured on Heretek's own labeled tasks before it gates anything.

## 11. Auditor (v2, deferred)

Adversarial review returns only after the gate engine passes the evaluation contract.

- Read-only access to the shadow diff and repository.
- An objection is recognized only with executable evidence: a failing test, a gate
  diagnostic, or a static-analysis finding.
- The auditor may author tests; the harness runs them in the shadow workspace and
  retains them as regression tests on a fail.
- Maximum two correction cycles, then rollback and escalation to the human with a
  failure summary.
- Known failure mode to measure: auditor tests that encode the same misunderstanding as
  the patch. A passing auditor is not evidence of correctness; the gates remain the
  evidence.

## 12. Surfaces

### 12.1 CLI

```
heretek gate [--staged|--worktree PATH] [--format json|text] [--baseline REF]
heretek run "task" [--lane fast|deep] [--max-turns N]
heretek mcp                 # stdio MCP server
heretek init [--lefthook]   # write config and hook snippet
heretek doctor              # toolchain and endpoint health
```

Exit codes: `0` pass, `1` blocking diagnostics, `2` usage/config error, `3` internal
error. `--format json` emits one report object on stdout; logs go to stderr.

### 12.2 Git hooks

`heretek init --lefthook` writes a snippet for `lefthook.yml`:

```yaml
pre-commit:
  commands:
    heretek:
      run: heretek gate --staged --format text
pre-push:
  commands:
    heretek:
      run: heretek gate --baseline origin/main --format text
```

The same binary gates agent commits and human commits. Hook mode never mutates the
worktree and never auto-fixes; it only reports.

### 12.3 MCP server

Tools: `gate_run` (path or staged), `gate_list`, `report_get`, `doctor`. Read-only with
respect to the repository unless `gate_run` is given a fix flag. Built on the official
Rust SDK (`rmcp`), stdio transport first, streamable HTTP later.

### 12.4 Skills and AGENTS.md

Heretek ships a skill that teaches any MCP-capable harness how to run gates before
declaring work complete, and reads `AGENTS.md` for repo conventions.

## 13. Evaluation contract

Pre-registered before implementation. Results are reported even when they falsify the
thesis.

**Baseline.** The same model, same tasks, run through an unmodified upstream harness
(OpenCode or Codex CLI) with `AGENTS.md` and no Heretek.

**Task sets.**

| Set | Content | Purpose |
| --- | --- | --- |
| Local | 10-20 tasks from maintainer-owned repositories (TBD, see open questions) | Real-world relevance |
| Public JS/TS | A fixed slice of SWE-bench Multilingual JS/TS plus Multi-SWE-bench mini | Comparability |
| Terminal agentic | Terminal-Bench 2.0 subset | Long-horizon signal |

Runs are offline: local model endpoints only.

**Metrics.** Task pass rate; false-reject rate (blocking diagnostics on patches that
tests later prove valid); human interventions per task; wall-clock per task; tokens
in/out; prefix-cache hit rate; cost in electricity-equivalent hours, not dollars.

**Proceed gate.** Ship the loop only if Heretek achieves at least +5 percentage points
pass rate OR at least 30% fewer human interventions, while false rejects stay at or
below 10%. Otherwise the gate engine ships as a standalone tool and the loop thesis is
re-examined.

## 14. Security principles

- Repo configuration (hooks, devcontainers, MCP server entries, task files) is untrusted
  input and is never executed by the gate pipeline.
- Gate subprocesses run with a scrubbed environment; network is disabled for SAST and
  test stages by default.
- Writes are confined to the shadow workspace; path traversal and symlink escapes are
  rejected before execution.
- Secrets are never logged; diagnostic frames redact environment variable values.
- No telemetry. Network calls are limited to dependency/model downloads and OSV lookups.

## 15. Repository layout

```
crates/
  heretek-core    Domain types: diagnostics, reports, profiles, config, Decider
  heretek-gate    Stage trait, pipeline, subprocess stages, shadow workspace
  heretek-cli     `heretek` binary: gate, run, mcp, init, doctor
docs/
  research/       Landscape research and source material
  spec/           Architecture specifications
  adr/            Architecture decision records
examples/         lefthook snippet, MCP config, sample .heretek.toml
```

## 16. Phases

| Phase | Deliverable | Estimate | Tracking |
| --- | --- | --- | --- |
| 0 | Baseline evaluation of the local model through an upstream harness on the public slice | 1 week | #1, #11 |
| 1 | Gate engine v0 (JS/TS stages), CLI, shadow worktree, lefthook integration | 3 weeks | #3, #4, #5, #6, #8 |
| 2 | Own loop v0, model router, context zones, repair passes, MCP server | 4 weeks | #2, #7, #9, #10 |
| 3 | Evaluation contract run, Rust/Python stages, auditor prototype, OSS release polish | 4 weeks | #12, #13 |

## 17. Open questions

1. **Local task-suite repositories.** The maintainer owns the only repositories that
   satisfy the realism requirement. Blocking for the evaluation contract (tracking
   issue filed).
2. **24GB KV budget.** Exact context ceiling for Qwen3.6-35B-A3B at Q4 with quantized KV
   must be measured on the actual GPU, not inferred.
3. **Prefix-cache fidelity per engine.** Whether zone-3 microcompaction preserves cache
   alignment on llama.cpp the way it does on vLLM/SGLang determines the compaction
   design. Spike required.
4. **LSP escalation.** CLI typecheckers are the v0 plan; if cross-file diagnostics prove
   insufficient, a persistent LSP client (lsp-types + JSON-RPC) enters as a stage.
5. **Windows support.** Out of scope for v0; sandboxing primitives differ and the target
   hardware is Linux.

## 18. Glossary

| Term | Meaning |
| --- | --- |
| Gate | A deterministic check with a pass/fail outcome |
| Stage | One gate implementation in the pipeline |
| Blocking / advisory | Whether stage findings fail the run or report only |
| Frame | The normalized, bounded diagnostic payload fed to the model |
| Shadow | The worktree where the model's edits live until they pass |
| Baseline | Diagnostics captured from HEAD before a patch, used for diff-aware blocking |
| Lane | A named model profile (`fast`, `deep`, `micro`) |
| Decide | The interface that produces routing and strictness decisions |
| False reject | A gate failure on a patch that is actually valid |

## 19. Implementation status (2026-09-20)

What exists in the repository today:

| Section | Status |
| --- | --- |
| 6 Gate engine | Implemented for JS/TS: tree-sitter syntax, biome format, tsgo/tsc typecheck, vitest/jest tests, ast-grep, semgrep secrets and SAST, knip, osv-scanner. Diagnostics use `heretek.diagnostic/1`. Python and Rust stages are not started (#3, #13). |
| 7 Shadow workspace | Implemented with git worktrees, per-turn snapshot refs, apply-on-pass, copy fallback for non-git trees, persistence and cleanup commands. |
| 8 Model layer | Implemented: OpenAI-compatible blocking client with retries, lanes, endpoint probing, token and cache accounting. Context tiers are configurable; per-engine cache verification is not yet measured (#7). |
| 9 Agent loop | Implemented: sandboxed tools, read-before-write (write requires prior read is not enforced yet), repair passes, storm detection, compaction, budgets, escalation, JSONL events. |
| 10 Decide | Heuristics implemented with reason codes; learned-model promotion is documented in ADR-0005. |
| 11 Auditor | Implemented as an opt-in pass (`agent.auditor` / `heretek run --audit`): read-only review of the diff, objections only as executable `node --test` files under `.heretek-audit/`, two-cycle cap, and an unresolved objection fails the session. |
| 12 Surfaces | CLI, lefthook snippet, MCP server, and `AGENTS.md` consumption are implemented. |
| 13 Evaluation contract | Runner and fixtures implemented in `eval/`; the public run and the local task suite (#1) are pending. |
| 14 Security | Sandbox rules, network namespace isolation where available, config protection, and baseline ref validation implemented; see SECURITY.md. |

Known deviations from this spec: the typecheck stage runs CLI checkers
(`tsgo`/`tsc`) rather than an LSP daemon (section 6 allows either); auto-fix is
gated on `--fix`/harness-owned shadows rather than always-on; the auditor is
deferred until the gate engine's evaluation numbers exist.

