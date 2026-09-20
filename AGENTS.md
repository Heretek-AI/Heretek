# AGENTS.md

Instructions for coding agents working in this repository.

## Commands

- Format: `cargo fmt --all`
- Lint: `cargo clippy --workspace --all-targets -- -D warnings`
- Test: `cargo test --workspace`
- Run CLI: `cargo run -p heretek-cli -- --help`

All three must pass before a commit.

## Conventions

- Rust 2024 edition. Crates are workspace members under `crates/`.
- No comments unless they explain a non-obvious constraint. Prefer self-describing names.
- Every gate must be deterministic. No LLM calls inside `heretek-gate`.
- Network access is limited to dependency downloads and OSV lookups (see ADR-0007).
- New architectural decisions get an ADR in `docs/adr/`; specs live in `docs/spec/`.

## Issue tracker

GitHub Issues on `Heretek-AI/Heretek`. One issue per tracer bullet, with acceptance
criteria and explicit dependencies. Reference issues by number in commits.

## Working agreements

- Local models are the prime LLM; cloud adapters are optional and off by default.
- The gate pipeline blocks only on new diagnostics (diff-aware), never on pre-existing debt.
- False rejections are the primary failure mode to guard against. Measure them.
