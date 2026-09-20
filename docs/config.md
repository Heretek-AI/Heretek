# Configuration reference

Heretek reads `.heretek.toml` from the repository root. Every field is
optional; unknown fields are rejected so typos fail loudly. Run
`heretek doctor` to validate tooling, and `heretek doctor --models` to probe
endpoints.

```toml
# Optional: names the default lane. Must match a [models.*] key.
default_lane = "fast"

[gate]
baseline = "origin/main"        # optional default baseline ref (validated)
stage_timeout_secs = 300        # per stage, 1..=900 for typecheck
max_output_bytes = 1048576      # per stage stdout/stderr cap
allow_network = false           # registry semgrep rules and similar
ast_grep_rules = "sgconfig.yml" # file or directory of rules
semgrep_config = "semgrep.yml"  # local path, or a registry id (needs network)
ignore = ["generated/"]         # extra ignore patterns (segment or glob match)

[gate.stages]
syntax = true                   # tree-sitter grammar check        (blocking)
format = true                   # biome; auto-fix only with --fix  (blocking)
typecheck = true                # tsgo / tsc                       (blocking)
tests = true                    # vitest / jest affected tests     (blocking)
ast_grep = true                 # structural rules                 (blocking)
secrets = true                  # semgrep secrets                  (advisory)
sast = true                     # semgrep OSS                      (advisory)
dead_code = true                # knip                             (advisory)
deps = true                     # osv-scanner, uses the network    (advisory)

[agent]
max_turns = 40                  # hard turn cap per session
max_wall_secs = 1800            # hard wall-clock cap
escalate = true                 # move to the deep lane after repeated failures
tool_result_token_budget = 3000 # per tool result before compaction
auditor = false                 # evidence-only adversarial pass after the gate
auditor_max_cycles = 2          # objections the generator must resolve

# Any OpenAI-compatible endpoint: llama-server, vLLM, SGLang, Ollama, or remote.
[models.fast]
lane = "fast"
base_url = "http://127.0.0.1:8080/v1"
model = "qwen3.6-35b-a3b"
context_tokens = 32768
api_key_env = "HERETEK_FAST_KEY"   # optional; read from the environment

[models.deep]
lane = "deep"
base_url = "http://127.0.0.1:8080/v1"
model = "qwen3.6-122b-a10b"
context_tokens = 131072

[models.micro]
lane = "micro"
base_url = "http://127.0.0.1:8080/v1"
model = "qwen3.5-4b"
context_tokens = 8192
```

`context_tokens` sizes the tool-result compaction cap for that lane (roughly a
twelfth of the window, or the `agent.tool_result_token_budget`, whichever is
smaller), so long tool output does not overflow the endpoint.

## Semantics worth knowing

- **Diff-aware blocking.** With a baseline (flag, config, or the session's
  internal HEAD baseline), only diagnostics absent from the baseline block.
  The baseline is matched as a multiset, so duplicate errors are handled.
- **Staged targets materialize the index.** `heretek gate --staged` checks
  exactly what a commit would contain, not the working tree.
- **Hook mode never mutates.** Formatting findings become warnings; auto-fix
  (`--fix`) is only honored for worktree targets.
- **Missing tools are loud, and unverified runs fail.** A blocking stage
  skipped because its tool is absent produces a top-level warning.
  `heretek gate` exits 1 unless at least one blocking stage actually ran on the
  gateable changed files (or no gateable files changed), and `heretek run`
  refuses `--apply` in the same case.
- **The auditor executes only sandboxed tests.** When `--audit` is enabled,
  `node --permission --allow-fs-read/-write=<shadow>` runs the auditor's
  `node:test` files. If the runtime cannot execute them, the session fails
  rather than clearing the audit.
- **Harness-owned configs.** tsconfig, biome, sgconfig, semgrep, vitest, and
  jest configs are restored from HEAD inside the shadow before each gate run.
- **Network.** Gate subprocesses run in an unprivileged network namespace when
  the kernel allows it; otherwise proxy environment variables are set and the
  isolation is best-effort. The dependency stage is the only intentional
  network user besides model and package downloads.
