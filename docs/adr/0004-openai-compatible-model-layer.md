# ADR-0004: OpenAI-compatible model interface

**Status:** accepted
**Date:** 2026-09-20

## Context

Local inference engines in 2026 (llama.cpp/llama-server, vLLM, SGLang, Ollama, LM Studio)
all expose OpenAI-compatible HTTP endpoints. They differ in prefix-cache behavior,
structured-output backends, and speculative decoding support, but not in the wire
protocol. Native SDK integration for each engine would multiply maintenance for no
capability gain.

## Decision

The model layer speaks OpenAI-compatible HTTP only. Configuration declares endpoints and
lanes (`fast`, `deep`, `micro`) as data. Engine-specific behavior (cache reuse, grammar
backends) is handled by profiles and verified per engine, never by engine-specific client
code. The reference profile targets `llama-server`; vLLM and SGLang profiles are
supported alternatives.

## Consequences

- Swapping engines or adopting a new model generation is a config change.
- The harness cannot rely on engine-specific quirks (for example, DeepSeek-only prefix
  caching) without a profile that verifies them.
- Prefix-cache fidelity must be tested per engine, since the cache-aligned context
  design assumes the engine has a prefix cache at all.
- Any OpenAI-compatible cloud endpoint can be attached for comparison, subject to
  ADR-0007.
