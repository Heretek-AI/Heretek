# ADR-0003: Rust core

**Status:** accepted
**Date:** 2026-09-20

## Context

The harness supervises long-lived subprocesses (type checkers, test runners, scanners),
manages filesystem snapshots, must ship as a single binary for easy adoption, and needs
to be fast enough that gates do not dominate a turn. Candidate languages: TypeScript
(best ecosystem fit with OpenCode and MCP tooling), Python (fastest prototyping, Aider
donor code), Go (simple concurrency), Rust (performance, sandboxing crates, native
tree-sitter, Codex CLI donor code).

## Decision

Rust 2024, one cargo workspace, three crates (`heretek-core`, `heretek-gate`,
`heretek-cli`). Edition 2024, stable toolchain. No async runtime in the gate crate
unless a concrete need appears; the CLI may use tokio.

## Consequences

- Single static binary distribution; no Node or Python runtime required to gate a repo.
- Process supervision and future sandboxing (Landlock, seccomp) have first-class
  crates.
- MCP server support comes from the official `rmcp` SDK.
- Contributors need Rust fluency; the plugin surface (MCP, skills, config) is the
  extension path instead of a scripting layer.
