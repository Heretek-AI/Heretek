# Heretek

**A local-first coding harness that makes mid-size open-weight models produce mergeable work by wrapping them in deterministic verification gates.**

Heretek exists because the model is not the only variable. A 35B-class local model that is weak at long-horizon agentic work can still ship correct, reviewable patches when the runtime refuses to accept anything that fails syntax, type, lint, test, and security gates. The harness inverts control: the model proposes, the gates dispose.

The name is deliberate. In the 40k sense, a heretek turns forbidden tools against the orthodoxy. In the 2026 sense, that orthodoxy is the $200/month subscription in the critical path of your own codebase.

## Why

- **Local models are the prime LLM.** Your hardware, your weights, your data. Cloud endpoints are optional adapters, never a requirement.
- **Deterministic gates, not vibes.** Compilers, type checkers, tests, formatters, and static analysis run as hard gates. No LLM decides whether code is correct in the critical path.
- **Harness quality is the leverage.** Independent 2026 measurements put the same model 5–40 points apart depending on the harness around it. That gap is engineering, not model weights.

## Status

Pre-alpha. This repository contains the research, architecture spec, and a Rust workspace skeleton. See:

- [`docs/research/2026-09-ai-coding-harness-landscape.md`](docs/research/2026-09-ai-coding-harness-landscape.md) — OSINT on the 2026 coding-agent ecosystem, and a fact-check of the source design document (`docs/research/inputs/`).
- [`docs/spec/0001-heretek-architecture.md`](docs/spec/0001-heretek-architecture.md) — the architecture spec: gates, shadow workspaces, model lanes, evaluation contract.
- [`docs/adr/`](docs/adr/) — decision records for the load-bearing choices.

## Target platforms

| Tier | Hardware | Default model profile |
| --- | --- | --- |
| Reference | 24GB GPU | `fast`: Qwen3.6-35B-A3B (Q4, ~22GB) |
| Extended | Strix Halo 128GB unified | `deep`: Qwen3.6-122B-A10B (Q4) |
| Micro | CPU or shared GPU | `micro` (4B–9B) for triage and routing |

Any OpenAI-compatible endpoint works: `llama-server`, vLLM, SGLang, Ollama, or a remote host.

## Repository layout

```
crates/
  heretek-core   Domain types: diagnostics, error frames, gate reports, model profiles
  heretek-gate   Gate pipeline, stage implementations, shadow workspace
  heretek-cli    `heretek` binary: gate, run, mcp, init
docs/
  research/      Landscape research and source material
  spec/          Architecture specification
  adr/           Architecture decision records
examples/        Integration snippets (lefthook, MCP config)
```

## Build

```sh
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## License

Dual-licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

at your option. See [ADR-0006](docs/adr/0006-oss-strategy-and-donors.md) for the reasoning.
