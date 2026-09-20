# ADR-0007: No cloud LLM in the critical path

**Status:** accepted
**Date:** 2026-09-20

## Context

The project exists to remove recurring subscription and per-token costs from agentic
coding and to keep source code on local hardware. Optional cloud features (a second-opinion
auditor, hosted arbiters, cloud embeddings) would reintroduce both the cost structure and
the data-egress surface, and would make the harness unusable in the air-gapped and
regulated environments where local inference matters most.

## Decision

No cloud LLM may sit in the critical path of gating, routing, or code generation.
Network access is limited to dependency and model downloads and OSV lookups. Any cloud
model adapter is off by default and must be explicitly configured. No telemetry is
collected or sent.

## Consequences

- Evaluation runs are offline and reproducible without accounts.
- The `Decide` and auditor interfaces may accept remote adapters later, but only behind
  explicit configuration and with the offline path preserved.
- Features that would normally rely on a hosted service (semantic search, embeddings)
  must run locally or be optional.
