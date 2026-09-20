# Security policy

## Reporting a vulnerability

Open a private security advisory on GitHub
(`Security` -> `Advisories` -> `Report a vulnerability`) or email the
maintainers listed on the organization profile. Do not open a public issue for
a suspected vulnerability. We aim to acknowledge within 72 hours.

## Threat model

Heretek runs untrusted model output inside a workspace. The relevant assets are
the developer's working tree, their git history, their credentials, and their
network.

Controls in place:

- Agent edits happen in a `git worktree` shadow, never directly in the working
  tree. Applying a shadow is an explicit, separate step.
- The tool sandbox rejects absolute paths, parent traversal, symlink escapes,
  and writes to `.git`, `.heretek`, or shadow metadata.
- Gate subprocesses run with a scrubbed environment and, where available, an
  unprivileged network namespace (`unshare --user --map-root-user --net`).
  Proxy blackholing is the fallback, not the guarantee.
- Gate stages are deterministic and never call a language model.
- Baseline refs from repository configuration are validated before reaching
  git, blocking option injection.
- Harness-owned tool configuration (tsconfig, biome, sgconfig, semgrep, test
  runner configs) is restored from HEAD before every gate run so a model cannot
  disable verification by rewriting config.
- No telemetry. Network access is limited to dependency and model downloads,
  and OSV lookups for the dependency stage.

Known limits to evaluate before deploying:

- Gate subprocess sandboxing depends on user namespaces being available; the
  fallback is best-effort. Run Heretek in a container or VM when gating
  untrusted code.
- The dependency stage performs OSV lookups over the network by design
  (ADR-0007). Disable `gate.stages.deps` for fully offline runs.
- A remote (cloud) model endpoint receives whatever the agent reads. Keep the
  `micro` and `fast` lanes local for sensitive repositories.
- Supply-chain attacks that target repository configuration (agent hooks,
  devcontainers, MCP entries) are out of scope for the gates themselves; do
  not run `heretek` inside a repository whose configuration you have not
  reviewed.
