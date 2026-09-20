# Evaluation harness

The runner measures whether Heretek (or a baseline harness) completes real tasks
and whether the gates help or hurt. It is deliberately small: fixtures, JSON
task files, deterministic checks, and a JSON report.

## Running

```sh
# Heretek, using a local model endpoint from the environment
HERETEK_BASE_URL=http://127.0.0.1:8080/v1 HERETEK_MODEL=qwen3.6-35b-a3b \
  python3 eval/runner.py --label heretek

# Or with an explicit config file copied into every fixture
python3 eval/runner.py --model-config examples/heretek.toml

# Baseline: the same tasks through another harness and the same model
python3 eval/runner.py --agent "opencode run" --label baseline
```

Reports land in `eval/results/` as `<timestamp>-<label>.json`.

## Task format

```json
{
  "id": "add-function",
  "description": "Create src/add.ts exporting an add(a, b) function",
  "fixture": "ts-basic",
  "task": "prompt given to the agent",
  "max_turns": 8,
  "checks": [
    { "type": "file_exists", "path": "src/add.ts" },
    { "type": "file_contains", "path": "src/add.ts", "text": "export function add" },
    { "type": "command", "run": "npm test", "expect_exit": 0, "timeout_secs": 300 }
  ]
}
```

Supported checks: `file_exists`, `file_contains`, `command`.

Fixtures live under `eval/fixtures/`. The runner copies the fixture into a
temporary directory, initializes git, installs the model config, runs the agent
with `--apply`, then applies the checks. Fixtures should be small and their
tests should run offline.

## Metrics and the proceed gate

The report records per task: pass/fail, check outcomes, exit code, wall time,
turns, and prompt tokens. Compare a Heretek run with a baseline run over the
same tasks. The project's pre-registered gate (spec section 13) is:

- at least +5 percentage points pass rate, or at least 30% fewer human
  interventions, and
- false rejections at or below 10%.

A false rejection is a task where the agent's work satisfied every check but
the harness reported a blocking gate failure (non-zero exit with all checks
passing). The runner surfaces the raw signal; keep a human in the loop when
scoring false rejections until the suite is stable.

## Local task suite

The maintainer-owned repository tasks required by the spec are tracked in issue
#1. Until they are linked here, this directory ships the public fixture only.
