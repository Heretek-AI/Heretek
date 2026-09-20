# The 2026 coding-harness landscape

**Compiled:** 2026-09-20
**Method:** web search and primary-source retrieval (project docs, GitHub, vendor blogs,
independent benchmark write-ups) across the categories requested by the maintainer.
**Confidence:** directional. Vendor-published numbers are marked as such; independently
reproduced numbers are preferred. Anything here can be falsified by a benchmark run —
that is the point of the evaluation contract in the spec.

This document accompanies [`inputs/ai-coding-harness-architecture-design.pdf`](inputs/ai-coding-harness-architecture-design.pdf),
a design document produced by a hosted deep-research agent. Section 15 scores that
document against the findings here. The spec in `docs/spec/0001-heretek-architecture.md`
supersedes it.

---

## 1. The harness landscape

There are now more than a dozen serious terminal coding agents. They divide into three
archetypes:

| Archetype | Examples | Model coupling | Harness source |
| --- | --- | --- | --- |
| Lab-locked | Claude Code, Gemini CLI | One vendor's models | Closed (Gemini CLI is Apache-2.0) |
| Lab-open, model-anchored | Codex CLI (Apache-2.0, Rust) | OpenAI default; Ollama officially supported | Open |
| Model-agnostic | OpenCode (MIT, 75+ providers), Aider (Apache-2.0, 100+ via LiteLLM), Pi (MIT), Goose, OpenHands | Any provider | Open |

Also active: Amp (mode-based routing, no BYOK), Factory, Command Code, Cline, Crush,
Qwen Code, DeepSeek-Reasonix, Codewhale, and the research stacks (SWE-agent, OpenHands).

**Convergence.** By late 2026 the big three (Claude Code, Codex CLI, OpenCode) share:

- a cascading repo instruction file (`CLAUDE.md`, `AGENTS.md`; OpenCode reads both),
- a plugin/skill marketplace pattern,
- subagents and parallel sessions,
- persisted sessions and wakeup/scheduling primitives,
- MCP as the tool integration layer,
- a non-interactive JSON mode for CI (`claude -p --bare`, `codex exec --json`,
  `opencode run --format json`),
- sandbox defaults that differ mainly in strictness (Codex `exec` defaults read-only;
  Claude Code has permission modes; OpenCode ships read-only plan + full-access build).

**Harness quality is a first-class variable.** Reported numbers:

- Terminal-Bench 2.0 (agentic terminal tasks): a Claude Code harness configuration at
  92.1% vs Codex CLI at 77.3% (harness + model bundle).
- Same model, different harness: 77% to 93% on an Opus model (Claude Code vs Cursor).
- Multiple independent studies cited by practitioners: a 5-40 point spread attributable
  to harness quality alone.

**Implication for local models.** If harness quality moves frontier models by 16 points,
it is the only honest lever available to a 35B model. This is the thesis Heretek tests.

Sources: techstackups.com comparison (2026-05), warp.dev harness-selection guide
(2026-08), ai.rundatarun.io three-harness field report (2026-05), dev.to harness
comparison (2026-04).

---

## 2. Integration standards: MCP, Skills, Agent Plugins, AGENTS.md

A harness that integrates with nothing is a dead end in 2026. The portable surface is:

| Layer | Standard | Steward | State |
| --- | --- | --- | --- |
| Tool/data connections | MCP | Linux Foundation (Agentic AI Foundation) | De facto; `2026-07-28` spec removed protocol sessions (stateless) |
| Procedural knowledge | Agent Skills (`SKILL.md`) | agentskills.io / AAIF | Widely adopted |
| Packaging | Agent Plugins 1.0.0 (`plugin.json` + `skills/` + `mcp.json`) | AWS, Cursor, Microsoft, OpenAI, Vercel (+ Google) | Published 2026-08-06 |
| Repo instructions | `AGENTS.md` | AAIF | Claude Code added native support 2026-08 |
| Live skills over MCP | SEP-2640 (draft) | MCP working group | Shipped early by OpenAI before spec merge |

Scale data (arXiv 2603.23802; 177,436 public MCP tools, Nov 2024-Feb 2026):

- Software development accounts for 67% of all agent tools and 90% of MCP server downloads.
- "Action" tools (environment-modifying) grew from 27% to 65% of usage.
- 28% of MCP servers show AI assistance in authorship; 62% of new servers in Feb 2026.

**Implication.** Heretek should consume and expose MCP, ship skills, and read `AGENTS.md`.
Inventing a private extension format is not a strategy.

---

## 3. Language servers and type checking

| Ecosystem | Reference | Challenger / successor | Notes |
| --- | --- | --- | --- |
| Python | pyright | **ty** (Astral, Rust) | Beta Dec 2025; stable targeted 2026; 10-100x mypy/pyright; LSP included. Roadmap explicitly includes dead-code elimination, unused-dependency detection, CVE reachability, type-aware linting |
| TypeScript/JS | tsc / vtsls | **tsgo** (TypeScript 7, Go port) | `@typescript/native-preview`; `tsgo --noEmit` is a drop-in for `tsc`; parallel checkers (`--checkers N`); TS 7.0 RC+ renames the binary back to `tsc` |
| Rust | rust-analyzer | — | Strongest diagnostic fidelity of the set |
| Go | gopls | — | Stable |
| C/C++ | clangd | — | Stable |

Harness integration state:

- **OpenCode** ships ~30 built-in LSP servers, auto-installs them, and feeds diagnostics
  back to the agent. It disables them by default because language servers drift, consume
  memory, and vary by version; their docs recommend CLI typecheckers instead.
- **Codewhale** embeds LSP diagnostics in its loop.
- A proposed "type server protocol" separate from LSP is under discussion; not a
  standard yet.

**Implication.** LSP is not a differentiator. For gates, prefer deterministic CLI
typecheckers (`tsgo --noEmit`, `ty check`, `cargo check`, `gopls check`) over long-lived
LSP daemons; revisit LSP only if cross-file diagnostics prove necessary.

Sources: astral.sh/blog/ty (2025-12), docs.astral.sh/ty, microsoft/typescript-go,
opencode.ai/docs/lsp, zylos.ai LSP ecosystem review (2026-01).

---

## 4. Linters and formatters

| Ecosystem | 2026 default | Contenders |
| --- | --- | --- |
| Python | **ruff** (lint + format) | ty for type-aware linting (roadmap) |
| JS/TS | **Biome 2.x** (one binary, ~500 rules, type-aware "Biotype") vs **Oxc** (`oxlint` 700+ rules, `oxfmt` beta) | ESLint v10 for plugin long tail; Prettier 4 with Rust backend |
| Rust | rustfmt + clippy | — |
| Go | gofmt + golangci-lint | — |

Oxc specifics worth knowing: `oxlint` is ~2x faster than Biome and 50-100x ESLint;
JS plugin support (ESLint v9 API) reached alpha in March 2026; type-aware rules run via
`tsgolint` on typescript-go (59/61 typescript-eslint rules in alpha). Vite 8 ships
Rolldown, so every Vite project already runs Oxc under the hood.

The settled practice across production teams: **fast syntactic pass in the edit loop,
type-aware/deep pass in CI.** Heretek's gate v0 uses that split.

Sources: toolchew.com Oxc vs Biome (2026-06), youngju.dev formatters/linters 2026
review, blogs.abhipanseriya.dev typed-linting comparison (2026-05).

---

## 5. Dead code

| Ecosystem | Tool | Notes |
| --- | --- | --- |
| JS/TS | **knip** | Module-graph analysis; unused files, exports, dependencies; 150+ framework plugins; `--fix` |
| Python | vulture | Triage tool with whitelists; ty's roadmap will add dead-code elimination |
| Go | staticcheck U1000, `x/tools deadcode`, deadexports | U1000 is per-package and skips exported symbols; whole-program tools find the exported-but-unused tail |

The community consensus, stated bluntly by knip's own docs: do not spend agent tokens
looking for dead code; run the deterministic tool. This is a perfect gate-category
candidate and a cheap win for the advisory layer.

---

## 6. Dependency management and supply chain

**Managers.** uv (Python), pnpm/bun/npm (JS), cargo (Rust), Go modules.

**Cooldown is the control of the year.** Minimum release age blocks versions published
in the last N days, defeating fast-moving account-takeover and typosquat attacks:

| Manager | Key | Since |
| --- | --- | --- |
| pnpm | `minimumReleaseAge` | v10.16 (Sep 2025) |
| Yarn | `npmMinimalAgeGate` | v4.10 (Sep 2025) |
| Bun | `minimumReleaseAge` | v1.3 (Oct 2025) |
| uv | `exclude-newer` | v0.9.17 (Dec 2025) |
| npm | `min-release-age` | v11.10 (Feb 2026) |
| Cargo/Go/others | — | Use Renovate `minimumReleaseAge` or Dependabot cooldowns |

**Auditing.** `osv-scanner` (OSV, including the `MAL-` known-malicious feed),
govulncheck, cargo audit, npm audit, pip-audit. Socket.dev adds behavioral detection
of novel malware.

**The 2025-26 attack wave targets agent configuration.** Shai-Hulud and Miasma variants
achieved code execution through repository files that tools auto-run on open or install:
`.devcontainer`, `.vscode/tasks.json`, `.claude` hooks, `.mcp.json`, and build manifests
(`binding.gyp` command substitution). A harness that auto-loads project config and MCP
servers is a supply-chain attack surface.

**Implication for Heretek.** The gate pipeline needs an SCA stage (advisory in v0,
blocking later), a cooldown policy check, and an explicit rule: repo config and MCP
servers are untrusted input, never executed by the gates without sandboxing.

Sources: jordanconway/package-manager-hardening (2026-04), Flo5k5/supply-chain-scan,
magnusj/secure-supply, temporal-community dependency-scout, Astral uv+Renovate docs.

---

## 7. Web application testing

**Playwright MCP is the standard** (Microsoft):

- Operates on the accessibility tree, not pixels; no vision model needed; ~200-400
  tokens per snapshot vs thousands for screenshots.
- 40+ tools: navigation, clicking, typing, forms, tabs, dialogs, network mocking,
  console access, tracing, video, PDF export.
- Persistent profiles by default, isolated contexts available.
- Included in GitHub Copilot's coding agent.

**The token-efficient path has shifted to CLI + Skills.** Playwright's own docs now say
coding agents should prefer the Playwright CLI plus skills over MCP for high-throughput
work, because MCP tool schemas and accessibility trees are expensive in context. MCP
remains for persistent exploratory loops.

**The reliable workflow pattern:**

1. Agent explores the app through Playwright and observes real behavior.
2. Agent authors durable Playwright tests with verified role-based locators.
3. Test drafts are reviewed like junior-engineer code.
4. CI runs plain Playwright; the MCP/CLI session is never a runtime dependency.
5. Self-healing repairs locators but surfaces diffs, never silently rewrites assertions.

Accessibility names double as the agent's locator surface: if an agent cannot
disambiguate two "Edit" buttons, neither can a screen reader. Agent test authoring is a
de facto accessibility audit.

Sources: playwright.dev/docs/getting-started-mcp, playwright.dev/mcp/introduction,
qaskills.sh Playwright MCP analyses (2026-06/08), shiplight.ai MCP test workflow (2026-08).

---

## 8. Desktop GUI testing

The same accessibility-first pattern is arriving for native apps:

| Tool | Platform coverage | Approach |
| --- | --- | --- |
| guiport | macOS first, Windows/Linux partial | AX tree first, screenshots/OCR fallback; replayable flow scripts |
| agent-computer-use | macOS AX, Windows UIA, Linux AT-SPI2, Electron via CDP | Ref-based snapshot, act, verify loop |
| Tarsier | Cross-platform via UIA/AX/AT-SPI + web | YAML ARIA-snapshot IR; claims ~70% token reduction; MCP server |
| naturo | Windows | Multi-framework fusion (UIA + MSAA + IA2 + JAB + CDP + vision), visual regression |

Established guidance: hybrid accessibility-then-vision beats vision-only; computer-use
agents are for exploration and test generation, not production execution; OSWorld and
Windows Agent Arena are the readiness benchmarks to watch.

Heretek v0 treats desktop automation as a non-goal; it is a plug-in surface later.

---

## 9. Static analysis

| Tool | Strength | Weakness |
| --- | --- | --- |
| Semgrep | Fast (seconds), YAML rules anyone can write, 30+ languages, secrets, MCP server, Claude/Cursor plugins | Shallow dataflow in OSS edition (intraprocedural taint) |
| CodeQL | Deep interprocedural taint, best recall on real CVEs | Database builds take 15-45 min; QL is a specialist language; no true incremental |
| ast-grep | Tree-sitter structural search and rewrite, 31 languages, CLI + LSP + napi + Python + MCP server | No dataflow; rewrites are verbatim splices (format afterwards) |

Benchmark data points:

- 2026 benchmark, 164 known CVEs, default settings: CodeQL 33.6% recall vs Semgrep OSS
  23.2%; Semgrep is dramatically faster and noisier (28% vs 11% FP under defaults).
- A week of scanning AI-agent PRs: of ~40 PRs, 3 had findings. CodeQL caught a
  cross-function command injection Semgrep CE missed; Semgrep caught a hardcoded secret;
  neither caught a vulnerable dependency upgrade (Dependabot did).

The production pattern: Semgrep on every commit, CodeQL on merge/nightly, ast-grep for
agent-driven structural rewrites and architectural rules. SAST does not replace SCA or
secret scanning; it complements them.

Sources: safeguard.sh CodeQL vs Semgrep buyer comparison (2026-02), dev.to AI-code SAST
experiment (2026-09), hivebook ast-grep profile (2026-05), semgrep docs.

---

## 10. Constrained decoding and local inference

**Structured output is commoditized.** XGrammar is the default backend in vLLM, SGLang,
TensorRT-LLM, and MLC; it precomputes context-independent token masks so JSON-schema
masking costs tens of microseconds per token. llguidance builds masks dynamically and
wins when every request carries a new, complex schema. SGLang overlaps mask generation
with GPU inference, hiding most of the cost; vLLM's mask generation is more sequential.

Caveats that matter for an agent harness:

- Reasoning models need explicit configuration to combine thinking with structured
  output (`--structured-outputs-config.enable_in_reasoning=True` in vLLM).
- FSM-based backends flatten or reject recursive schemas; CFG backends (XGrammar,
  llguidance) handle recursion.
- Constraining the final answer is cheap; constraining an entire language-level patch
  is not. Practical split: JSON/grammar for tool calls and decision payloads,
  tree-sitter + retry for code bodies.
- Let the model reason in free text; constrain the artifact, not the thinking.

Sources: vLLM structured outputs docs, SGLang constrained decoding docs, squeezebits
guided-decoding benchmark (2025-09), dreaming.press constrained decoding guide (2026-07).

---

## 11. System 1 models: Jev and the open approximations

**Jev (TypeSafe AI)** launched 2026-09-15. It returns typed answers with probability
distributions instead of text: Choice (up to 255 options), Score, and Noul (binary
probability). Claims: 70-500 ms latency, $0.042/M input tokens, output free, schema
conformance guaranteed by construction.

What independent analysis established within days:

- The schema guarantee is real: a successful response cannot contain a malformed value.
  It says nothing about whether the value is correct.
- The "0% hallucination" chart is asserted by construction, and TypeSafe's own footnote
  says it is not empirical.
- **Calibration is unverified.** No paper, no reliability diagram, no expected
  calibration error. Independent tests measured roughly 5x speedup vs cheap LLMs, not
  the 193x headline against frontier reasoning models. One analysis showed the returned
  `confidence` for Choice is a rescaling of the top probability, not a measure of model
  uncertainty; near-tie option sets are unstable across identical calls.
- TypeSafe's own "jaggedness" page documents failure modes: unreliable arithmetic,
  counting, and date comparison; literal bias; prompt-injection susceptibility; context
  saturation; no text output.

**Open approximations** exist (multiple `openjev` reimplementations: direct logit
readout from a prefilled KV cache, NLI cross-encoder heads, masked-LM heads). They are
research previews, not load-bearing components.

**Implication.** Deterministic heuristics behind a `Decide` interface for v1. Any
Jev-style model enters as an optional adapter, and only after measuring calibration on
Heretek's own labeled tasks. Confidence-gated routing is a good pattern with an
unvalidated implementation.

Sources: docs.typesafe.ai, flaviocopes.com/jev (2026-09-18), bernoulli.app/is-jev-confident
(2026-09-18), jev.novcog.us.com (2026-09), mchromiak.github.io analysis (2026-09-17),
TheoLeeCJ/openjev, daseinlabs/open-jev.

---

## 12. Local models and hardware

### Qwen generations (as of 2026-09)

| Model | Architecture | Release | Notable scores |
| --- | --- | --- | --- |
| Qwen3.5-35B-A3B | 35B total / 3B active MoE, 262K native ctx | 2026-02 | SWE-bench Verified 72.0; Terminal-Bench 2 **31.9**; FullStackBench-en 30.6 |
| Qwen3.6-35B-A3B | 35B / 3B active MoE | 2026-03 | Terminal-Bench 2.0 **51.5**; SWE-bench Verified 73.4; SWE Multilingual 67.2 |
| Qwen3.8-27B | 27B dense | 2026-08 | Terminal-Bench 2.1 **73.0**; SWE-bench Pro 61.7; much higher intelligence index |

The generation delta on agentic benchmarks is the story: 31.9 to 51.5 to 73.0. Model
generation matters more than any harness trick available today, which is why the router
must be able to adopt new weights without a rewrite.

### Strix Halo (Ryzen AI Max+ 395, 128GB unified, ~215 GB/s real)

- **MoE or nothing.** 35B-A3B at Q8: ~44-53 tok/s empty-context decode; ~37.5 tok/s at
  76k context with MTP on ROCm, ~29 tok/s Vulkan non-MTP; 262K context viable with Q4 KV.
  Dense 27B: 6-11 tok/s, unusable for iteration.
- 122B-A10B at Q4 (~77-99GB): ~19-24 tok/s, viable as a quality lane.
- MTP speculative decoding is a large win on 35B (+50-100% at long context) and needs
  a matched draft head.
- Backend split: Vulkan/RADV is the stable generation path; ROCm/HIP wins prompt
  processing and degrades less predictably at full context; vLLM on Strix Halo is
  experimental as of mid-2026.

### 24GB GPU

- 35B-A3B at 4-bit is ~22GB, so it fits with little room for KV; expect a 16-32k working
  set with Q4/Q8 KV quantization.
- vLLM or llama.cpp both work; vLLM has the better automatic prefix cache.

### Serving engines

| Engine | Best for | Prefix cache |
| --- | --- | --- |
| llama.cpp / llama-server | Consumer hardware, GGUF, MTP, Strix Halo | Slot-based prompt cache, `--cache-reuse`; less automatic than vLLM |
| vLLM | NVIDIA GPUs, concurrency, structured output | Automatic prefix caching |
| SGLang | Throughput, RadixAttention, guided decoding overlap | RadixAttention (strongest) |
| Ollama | Easiest local setup | Weak controls |

All speak OpenAI-compatible HTTP, which is the interface Heretek targets.

Sources: HuggingFace Qwen model cards, unsloth Qwen3.5 guide, benchlm/llm-stats/artificial
analysis comparisons, kmarble.dev Strix Halo full-context study (2026-05), Level1Techs
Strix Halo notes (2026-04), datahardware.ai Strix Halo tok/s (2026-08), hogeheer499's
strix-halo-guide, Red Hat llama.cpp vs vLLM (2026-06).

---

## 13. Harness architecture patterns worth borrowing

These are shipped, documented mechanisms, not theory:

| Pattern | Source | What it does |
| --- | --- | --- |
| Cache-first loop | DeepSeek-Reasonix | Immutable prefix + append-only log + volatile scratch; reported 99.8% prefix-cache hit on a 435M-token workload ($12 vs $61 uncached) |
| Tool-call repair pipeline | Reasonix | Flatten complex schemas, scavenge tool calls from reasoning text, repair truncated JSON, storm-break repeated calls |
| Transparent cost escalation | Reasonix | Flash-first defaults; escalation to the expensive model is announced, never silent |
| Shadow git state | Codewhale | External side-git snapshots before/after every turn; `/restore` rollback |
| Constitutional priority | Codewhale | Live evidence > user instructions > stale memory |
| Read-before-write | Claude Code | Edits rejected unless the file was read this session |
| Structured compaction | Claude Code | Tool-result budgets, snipping, microcompaction, auto-compact |
| Repo map | Aider | Structural understanding of the codebase before edits |
| JSONL event stream | Codex CLI | Machine-readable run output for CI |
| Permission modes + sandbox defaults | Codex CLI, Claude Code, OpenCode | Read-only plan mode, workspace-write, full access |

Measured effects that justify the cache and repair work: DeepSeek bills cached input at
~10% of the miss rate, and most naive agent loops achieve <20% cache hits because they
inject timestamps and reorder context every turn.

---

## 14. Benchmarks relevant to a local TS-first harness

| Benchmark | Coverage | Notes |
| --- | --- | --- |
| SWE-bench Verified | Python, 500 tasks | The default headline; weak proxy for agentic work |
| SWE-bench Multilingual | 9 languages, 300 tasks; JS/TS = 43 | Claude 3.7 Sonnet + SWE-agent baseline: 43% overall, 34.9% JS/TS (2025 data) |
| Multi-SWE-bench | 7 languages, 1,632 tasks; TypeScript 382 + JavaScript 586 | Dockerized, human-verified; mini variant has 400 instances across 8 languages |
| SWE-Lancer | JS/TS freelance tasks, 1,400+ | Closest public proxy to real product work |
| Terminal-Bench 2.0 / 2.1 | Terminal-native agentic tasks | Better signal than SWE-bench for autonomous loops; Qwen3.6-35B = 51.5, Qwen3.8-27B = 73.0 |

**Implication.** The JS/TS slice is large enough to build a credible evaluation without
touching Python-only benches, and it aligns with the maintainer's TS-first target.

Sources: swebench.com/multilingual, microsoft/multi-swe-bench, llm-stats multilingual
leaderboard, artificialanalysis.ai model comparison pages.

---

## 15. Fact-check of the source design document

The input document (`inputs/ai-coding-harness-architecture-design.pdf`) is a hosted
deep-research agent's synthesis. Its architectural direction (deterministic gates,
shadow state, evidence over narration, cache-aligned context) matches shipped practice.
Its factual substrate is mixed.

### Verified or directionally correct

- Deterministic verification gates around a stochastic model: correct direction, and
  consistent with how Codex CLI, Claude Code, OpenCode, and Codewhale are built.
- Shadow-state rollback via external git snapshots: real (Codewhale's side-git).
- Evidence-supremacy priority (live output > user instruction > stale memory): real
  (Codewhale's constitutional hierarchy).
- Grammar-constrained decoding for tool calls: real, now table stakes (XGrammar).
- Context compaction and tool-result budgets: real (Claude Code's pipeline).
- Jev's schema guarantee: real by construction, as the document says.
- Codewhale and DeepSeek-Reasonix exist and roughly match the described architectures.

### Wrong or stale

1. **Qwen3.5-35B-A3B is not the frontier and is already superseded.** The document
   targets it as the reference model. Qwen3.6-35B-A3B (Mar 2026) more than doubled its
   Terminal-Bench score (31.9 to 51.5), and Qwen3.8-27B reaches 73.0 on Terminal-Bench
   2.1. The document also states a "32k or 128k" context window; the model card says
   262,144 native, extensible to ~1M.
2. **Jev-dependent arbitration is faith-based.** The document gates irreversible actions
   on Jev confidence thresholds (for example, gamma >= 0.90, p(defect) <= 0.05). TypeSafe
   has published no calibration evidence, independent tests measured ~5x rather than the
   advertised multiples, and the documented jaggedness list rules out several described
   uses (arithmetic, dates, counting, prompt-injection resistance).
3. **"OpenCode lacks compiler integration."** Outdated. OpenCode ships ~30 LSP servers
   with diagnostics fed back to the agent; it merely defaults them off and recommends
   CLI typecheckers instead.
4. **Grammar-constrained *diffs* are oversold.** Constraining tool-call JSON is trivial
   with modern engines. Constraining an arbitrary language patch to a grammar is a
   per-language research problem. The document treats the two as the same feature.
5. **Missing the 2026 integration surface.** No mention of MCP, Agent Skills, Agent
   Plugins, or `AGENTS.md`. A harness in 2026 is expected to speak these; the document
   describes a closed system.
6. **Missing supply-chain security.** The document's tool table has SAST but no SCA, no
   dependency cooldown, no secrets scanning, and no treatment of repo config as
   untrusted input, despite the 2025-26 attacks that target agent configuration files.
7. **Missing test-authoring tooling.** No reference to Playwright MCP/CLI or desktop
   accessibility-tree tooling, which is how agents verify UI behavior in 2026.
8. **Sandboxing is Linux/macOS-only.** Landlock, Seatbelt, seccomp, and Bubblewrap do
   not cover Windows; the document does not mention an alternative.
9. **"Two-pass adversarial review" depends on the audit writing tests.** That is a sound
   evidence mechanism, but the document does not address auditor tests encoding the same
   misunderstanding as the patch, which is the classic failure mode.
10. **No evaluation methodology.** There is no benchmark, no baseline, no metric, and no
    false-reject budget. An architecture whose thesis is "harness quality closes the
    model gap" must be measured against a baseline.

### Verdict

Keep the architectural spine: deterministic gates, shadow state, evidence-only
blocking, cache-aligned context, bounded repair loops. Discard the specific model
targets, the Jev thresholds, the grammar-of-diffs framing, and the closed-system
assumption. Replace the missing evaluation and supply-chain sections with the ones in
this document.

---

## 16. What this means for Heretek

1. **Gates are the product, and they are measurable.** The 5-40 point harness spread is
   the opportunity; the evaluation contract in the spec is how we prove we captured it.
2. **Target JS/TS first.** It has the largest non-Python public benchmark surface
   (SWE-bench Multilingual + Multi-SWE-bench), the largest agent-tool ecosystem, and
   first-class local tooling (tsgo, Biome/Oxc, vitest, knip).
3. **The model layer must be swappable.** The Qwen generation delta shows model
   progress outpaces harness engineering; the router adopts new weights without a
   rewrite.
4. **Cache discipline is an economics feature, not a micro-optimization.** At local
   token costs the cache hit rate determines whether an agent can run all day.
5. **Deterministic Decide before model-based Decide.** The confidence-gated routing
   pattern is right; the available model implementations are not yet trustworthy.
6. **Expose MCP and skills; consume AGENTS.md.** Portability is the adoption surface.

---

## Source index

Harnesses and standards: techstackups.com (2026-05), warp.dev (2026-08),
ai.rundatarun.io (2026-05), 0dai.dev (2026-04), developers.googleblog.com Agent Plugins
(2026-08-06), arcade.dev Skills over MCP (2026-09-08), findaiagency.com standards guide
(2026-08-27), arXiv 2603.23802 (agent tool ecosystem).

Tooling: astral.sh/blog/ty, docs.astral.sh/ty, microsoft/typescript-go, opencode.ai/docs/lsp,
toolchew.com (2026-06), blogs.abhipanseriya.dev (2026-05), knip.dev, pkg.go.dev deadexports
and go-fynx/deadcode, jordanconway/package-manager-hardening (2026-04), Flo5k5/supply-chain-scan,
magnusj/secure-supply, playwright.dev MCP docs, qaskills.sh (2026-06, 2026-08),
safeguard.sh (2026-02), dev.to AI-code SAST experiment (2026-09), hivebook.wiki ast-grep.

Models and inference: huggingface.co/Qwen model cards, unsloth.ai Qwen3.5 guide,
benchlm.ai and llm-stats.com comparisons, artificialanalysis.ai, docs.vllm.ai structured
outputs, SGLang constrained decoding docs, squeezebits guided-decoding benchmark,
dreaming.press (2026-07), kmarble.dev (2026-05), forum.level1techs.com (2026-04),
datahardware.ai (2026-08), github.com/hogeheer499-commits/strix-halo-guide,
developers.redhat.com (2026-06).

Jev: docs.typesafe.ai, flaviocopes.com/jev (2026-09-18), bernoulli.app (2026-09-18),
jev.novcog.us.com (2026-09), mchromiak.github.io (2026-09-17), github.com/TheoLeeCJ/openjev,
github.com/daseinlabs/open-jev.

Benchmarks: swebench.com/multilingual, github.com/multi-swe-bench, llm-stats.com,
artificialanalysis.ai.

*Vendor-reported numbers are marked as such throughout. Where sources disagree, the
disagreement is noted rather than averaged.*


