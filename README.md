# Heretek

**A local-first coding harness that makes mid-size open-weight models produce mergeable work by wrapping them in deterministic verification gates.**

Heretek exists because the model is not the only variable. A 35B-class local model that is weak at long-horizon agentic work can still ship correct, reviewable patches when the runtime refuses to accept anything that fails syntax, type, lint, test, and security gates. The harness inverts control: the model proposes, the gates dispose.

## Quick start

```sh
# Build (Rust stable; edition 2024)
cargo build --release -p heretek-cli

# Check tooling and endpoints
heretek doctor
heretek doctor --models

# Write starter configuration and optional lefthook hooks
heretek init --lefthook

# Gate what you are about to commit (exit 1 on blocking findings)
heretek gate --staged
heretek gate --staged --baseline origin/main --format json

# Run an agent session against a local OpenAI-compatible model
heretek run "fix the failing auth test" --max-turns 20 --apply

# Expose gates to any MCP-capable harness (Claude Code, OpenCode, Codex)
heretek mcp

# Inspect or clean up agent workspaces
heretek apply .heretek/worktrees/<session>
heretek clean
```

Point `.heretek.toml` at any OpenAI-compatible endpoint (`llama-server`, vLLM, SGLang, Ollama, or a remote host). See [`docs/config.md`](docs/config.md).

### Install and provision

```sh
cargo install --path crates/heretek-cli     # puts `heretek` on PATH
```

The gate engine shells out to project tooling. Missing tools are reported
loudly and cause unverified runs to fail, so install what your repositories
use:

| Stage | Tool | Install |
| --- | --- | --- |
| syntax | tree-sitter | bundled (built into the binary) |
| format | Biome | `pnpm add -D @biomejs/biome` in the repo |
| typecheck | tsgo or tsc | `pnpm add -D @typescript/native-preview` or `typescript` |
| tests | vitest or jest | `pnpm add -D vitest` (or jest) |
| ast-grep | ast-grep | `cargo install ast-grep` and add `sgconfig.yml` |
| secrets + SAST | Semgrep | `pipx install semgrep` (or `brew install semgrep`) |
| dead code | knip | `pnpm add -D knip` |
| deps | osv-scanner | `go install github.com/google/osv-scanner/v2/cmd/osv-scanner@latest` |

`heretek doctor` lists what is present and what is missing. Run it before the
first gate; a green `heretek gate` on a machine with no tools installed is
reported as unverified and exits non-zero.

## What it does

| Surface | Behavior |
| --- | --- |
| `heretek gate` | Deterministic pipeline over staged changes or a worktree: tree-sitter syntax, biome formatting, tsgo/tsc typecheck, affected tests, ast-grep structural rules, semgrep secrets/SAST, knip dead code, osv-scanner dependencies. Only new diagnostics block. |
| `heretek run` | Agent loop in a git-worktree shadow: path-sandboxed tools, tool-call repair, storm detection, context compaction, turn and wall-clock budgets, announced escalation, gate feedback after every write, and an opt-in evidence-only auditor (`--audit`) whose objections are executable tests, validated under `node --permission`. Emits a JSONL event stream. |
| `heretek mcp` | Read-only MCP server exposing `gate_run`, `gate_list`, `report_get`, and `doctor`. |
| Hooks | `heretek init --lefthook` gates agent and human commits with the same pipeline. |

## Why gates

Independent 2026 measurements put the same model 5-40 points apart depending on the harness around it. Heretek treats that as an engineering surface: deterministic tools decide correctness, the model only proposes. Pre-existing debt never blocks; only new findings do, with false rejections measured as the primary failure mode.

## Evaluation

`eval/` contains the task runner and fixtures for measuring pass rate, wall time, turns, and false rejections against a baseline harness on the same model. See [`eval/README.md`](eval/README.md).

## Documentation

- [`docs/spec/0001-heretek-architecture.md`](docs/spec/0001-heretek-architecture.md) - architecture spec and evaluation contract
- [`docs/research/2026-09-ai-coding-harness-landscape.md`](docs/research/2026-09-ai-coding-harness-landscape.md) - 2026 ecosystem research and fact-check of the original design document
- [`docs/adr/`](docs/adr/) - decision records
- [`docs/config.md`](docs/config.md) - configuration reference
- [`SECURITY.md`](SECURITY.md) - threat model and reporting
- [`CONTRIBUTING.md`](CONTRIBUTING.md) - build, conventions, PR expectations

## Target platforms

| Tier | Hardware | Default lane |
| --- | --- | --- |
| Reference | 24GB GPU | `fast`: Qwen3.6-35B-A3B (Q4, ~22GB) |
| Extended | Strix Halo 128GB unified | `deep`: Qwen3.6-122B-A10B (Q4) |
| Micro | CPU or shared GPU | `micro`: 4B-9B for triage |

## Status

Alpha. All surfaces above work end to end and are exercised against a scripted model endpoint; the gate engine is the most mature part. The public evaluation run and the maintainer task suite (issue #1) are the next evidence milestone. CI runs `cargo fmt --check`, `cargo clippy -D warnings`, and `cargo test`.

## License

Dual-licensed under either of Apache-2.0 ([LICENSE-APACHE](LICENSE-APACHE)) or MIT ([LICENSE-MIT](LICENSE-MIT)) at your option. See [NOTICE](NOTICE) for attribution.
