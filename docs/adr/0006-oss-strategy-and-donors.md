# ADR-0006: OSS strategy and donor sources

**Status:** accepted
**Date:** 2026-09-20

## Context

The harness should be an open-source project. The surrounding ecosystem (OpenCode MIT,
Codex CLI Apache-2.0, Aider Apache-2.0, Reasonix MIT, tree-sitter, rmcp, and the
scanner/formatter tools) is licensable and borrowable. Two failure modes to avoid:
license contamination from vendoring code without notices, and inheriting the roadmap
and architecture of a forked harness.

## Decision

- **License:** dual MIT/Apache-2.0, the Rust-ecosystem convention and compatible with
  the Apache-2.0 donor projects.
- **Consume tools as subprocesses or libraries** (biome, tsgo, vitest, ast-grep,
  semgrep, knip, osv-scanner, tree-sitter, rmcp).
- **Reimplement architecture patterns with attribution** (cache-first loop, repair
  passes, read-before-write, structured compaction, evidence supremacy).
- **Vendor code only for small, license-clean units** with notices preserved.
- **Never fork an entire harness.** Pattern borrowing is cheap; roadmap inheritance is
  not.

## Consequences

- Attribution belongs in commit messages and, where required, a NOTICE file.
- Tool versions become part of the compatibility surface; `heretek doctor` reports them.
- The project stays small enough to audit and reimplement where necessary.
