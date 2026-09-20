# ADR-0008: Git worktrees for shadow state

**Status:** accepted
**Date:** 2026-09-20

## Context

Model edits must be isolated until they pass the gates, and rollback must be trivial.
The source design document proposed an external side-git repository
(`.harness/side-git`). Alternatives: plain git worktrees with private refs, a temporary
branch in the user's repository, or a filesystem copy. The chosen mechanism must work
without polluting the user's history, must not require network, and must degrade
gracefully when the target is not a git repository.

## Decision

Use `git worktree` at HEAD, registered under `.heretek/worktrees/<id>`, with per-turn
snapshots committed to private refs (`refs/heretek/turns/...`). Applying a passing change
is a patch application against the user's working tree. Non-git targets fall back to a
filesystem copy under `.heretek/shadow/`.

## Consequences

- No speculative commits appear in the user's branch history and no side repository has
  to be maintained.
- `git worktree` shares the object store, so snapshots are cheap.
- Worktree-specific files (node_modules, build output) are not duplicated automatically;
  the harness config declares which paths to link or skip.
- The hook surface (`gate --staged`) does not need a shadow at all and never mutates the
  worktree.
