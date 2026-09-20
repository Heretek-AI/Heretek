# lefthook integration

[lefthook](https://lefthook.dev) is the reference git-hook manager for Heretek. The
gate binary is a plain CLI, so any hook runner works; lefthook is convenient because it
is fast, cross-platform, and supports staged-file filtering.

Add to `lefthook.yml`:

```yaml
pre-commit:
  commands:
    heretek:
      run: heretek gate --staged --format text

pre-push:
  commands:
    heretek:
      run: heretek gate --baseline origin/main --format text
```

Behavior guarantees in hook mode:

- The worktree is never modified and auto-fixes are disabled.
- Only new diagnostics block (diff-aware against `--baseline` or the index).
- Exit codes: `0` pass, `1` blocking diagnostics, `2` usage/config error, `3` internal.
- `heretek doctor` reports which gate tools are available before hooks are enabled.

Because a failing hook blocks the commit, start with the pipeline's advisory stages and
promote stages to blocking after measuring false rejects on real history.
