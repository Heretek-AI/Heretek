# ADR-0002: Gate engine first, own loop second

**Status:** accepted
**Date:** 2026-09-20

## Context

Two candidate products: (a) a deterministic gate engine that any harness can call, and
(b) a full coding harness with its own agent loop. The gate engine is the differentiator
and is independently valuable; the loop is crowded territory (OpenCode, Codex CLI,
Aider, and others) and only pays off once the gates are proven. Wrappers cannot deliver
cache-aligned context or tool-call repair, so the loop cannot be permanently delegated
either.

## Decision

Sequence the work. Phase 1 delivers `heretek-gate` as a standalone binary plus CLI,
git-hook, and MCP surfaces. Phase 2 builds the own loop around the proven gate engine.
The two components share no internals: the loop depends on the gate engine's public
report types, never the reverse.

## Consequences

- Value ships in weeks rather than quarters, and early adopters can use the gates with
  their existing harness.
- The evaluation contract (spec section 13) measures gate value independently of loop
  value, which keeps the thesis falsifiable.
- Some loop-specific concerns (context zones, repair passes) are deferred but their
  interfaces are reserved in the spec.
