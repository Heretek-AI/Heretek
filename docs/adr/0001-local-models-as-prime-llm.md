# ADR-0001: Local models as the prime LLM

**Status:** accepted
**Date:** 2026-09-20

## Context

Frontier coding subscriptions cost $20-$200/month, require sending source to third
parties, and bind the workflow to a single vendor. Open-weight models in the 30-35B
class now ship agentic capabilities that were frontier-only eighteen months ago
(Qwen3.6-35B-A3B: 51.5% on Terminal-Bench 2.0; Qwen3.8-27B: 73.0% on Terminal-Bench
2.1). Consumer hardware can run them: a 35B-A3B at 4-bit fits a 24GB GPU; the same model
at Q8 runs ~50 tok/s on a 128GB Strix Halo with 262K context available.

## Decision

Local models are the primary LLM for every critical-path function. Cloud endpoints are
optional adapters and off by default. Hardware targets are a 24GB GPU (`fast` lane) and
a Strix Halo 128GB box (`deep` lane).

## Consequences

- The harness must treat VRAM and context as scarce resources, which drives the
  three-zone prompt layout and cache discipline.
- Model quality is bounded by what fits on the target hardware; gate coverage
  compensates.
- Evaluation runs are offline and cost electricity, not dollars; results are
  reproducible by anyone with comparable hardware.
- Any OpenAI-compatible endpoint can be connected, so a cloud spike is a config change,
  not an architecture change.
