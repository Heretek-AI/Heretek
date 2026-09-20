# ADR-0005: Deterministic Decide in v1

**Status:** accepted
**Date:** 2026-09-20

## Context

Confidence-gated routing (low-risk changes proceed, high-risk changes escalate) is a
sound architecture, and the source design document proposed implementing it with
TypeSafe's Jev. Independent analysis of Jev found: the schema guarantee is real but
says nothing about correctness; calibration is unverified with no published reliability
diagram or ECE; the advertised speedups were not reproduced by independent testers
(about 5x, not 193x); the returned confidence is a rescaling of the top probability; and
documented failure modes include arithmetic, counting, dates, and prompt injection.

## Decision

The v1 `Decider` is deterministic heuristics over change metadata (paths, size, tests
touched, dependency changes, failure history). `Decision<T>` carries a reason code so
routing is auditable. The trait is the extension point: logit-readout models,
cross-encoders, or Jev can be evaluated behind it, but they may only gate once
calibration is measured on Heretek's own labeled tasks.

## Consequences

- Routing is fully explainable and testable from day one.
- No external API dependency in the critical path.
- The interesting research (calibrated decision models) remains available as an adapter
  experiment with a clear promotion criterion.
- Some nuance is lost compared with a trained classifier; the evaluation contract
  measures whether it matters.
