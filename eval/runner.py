#!/usr/bin/env python3
"""Heretek evaluation runner.

Runs a task suite against any agent command (Heretek or a baseline harness),
applies deterministic checks, and writes a results report. Standard library
only; no network access beyond the configured model endpoint.

Task file format (JSON):

{
  "id": "add-function",
  "description": "Add an exported add() function",
  "fixture": "ts-basic",              # directory under eval/fixtures/
  "task": "prompt given to the agent",
  "max_turns": 8,
  "checks": [
    {"type": "file_contains", "path": "src/add.ts", "text": "export function add"},
    {"type": "command", "run": "npm test", "expect_exit": 0, "timeout_secs": 300}
  ]
}

Usage:
  python3 eval/runner.py --agent "heretek run"            [--model-config PATH]
  python3 eval/runner.py --agent "opencode run" --label baseline
"""

from __future__ import annotations

import argparse
import json
import os
import shutil
import subprocess
import sys
import tempfile
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parent


def load_tasks(tasks_dir: Path) -> list[dict]:
    tasks = []
    for path in sorted(tasks_dir.glob("*.json")):
        with path.open() as handle:
            task = json.load(handle)
        task.setdefault("id", path.stem)
        task.setdefault("max_turns", 8)
        task.setdefault("checks", [])
        tasks.append(task)
    return tasks


def prepare_workspace(task: dict, workdir: Path) -> None:
    fixture = ROOT / "fixtures" / task["fixture"]
    if not fixture.is_dir():
        raise SystemExit(f"fixture not found: {fixture}")
    shutil.copytree(fixture, workdir, dirs_exist_ok=True)
    subprocess.run(["git", "init", "-q"], cwd=workdir, check=True)
    subprocess.run(["git", "config", "user.email", "eval@heretek.local"], cwd=workdir, check=True)
    subprocess.run(["git", "config", "user.name", "heretek-eval"], cwd=workdir, check=True)
    subprocess.run(["git", "add", "-A"], cwd=workdir, check=True)
    subprocess.run(["git", "commit", "-qm", "fixture"], cwd=workdir, check=True)


def install_model_config(workdir: Path, model_config: Path | None) -> None:
    if model_config is not None:
        shutil.copy(model_config, workdir / ".heretek.toml")
        return
    base_url = os.environ.get("HERETEK_BASE_URL")
    model = os.environ.get("HERETEK_MODEL")
    if base_url and model:
        (workdir / ".heretek.toml").write_text(
            "[models.fast]\n"
            'lane = "fast"\n'
            f'base_url = "{base_url}"\n'
            f'model = "{model}"\n'
        )


def run_agent(agent: str, task_text: str, workdir: Path, max_turns: int, timeout: int) -> dict:
    command = agent.split() + [task_text, "--max-turns", str(max_turns)]
    if command[0].endswith("heretek"):
        command.append("--apply")
    started = time.time()
    try:
        completed = subprocess.run(
            command,
            cwd=workdir,
            capture_output=True,
            text=True,
            timeout=timeout,
        )
        exit_code = completed.returncode
        stdout = completed.stdout
        stderr = completed.stderr
    except subprocess.TimeoutExpired as error:
        exit_code = 124
        stdout = (error.stdout or b"").decode() if isinstance(error.stdout, bytes) else (error.stdout or "")
        stderr = f"timeout after {timeout}s"
    return {
        "exit_code": exit_code,
        "wall_secs": round(time.time() - started, 2),
        "stdout_tail": stdout[-2000:],
        "stderr_tail": stderr[-2000:],
        "turns": parse_int(stdout, "heretek run: ", " turn"),
        "tokens_prompt": parse_int(stdout, "tokens: ", " prompt"),
    }


def parse_int(text: str, prefix: str, suffix: str) -> int | None:
    for line in text.splitlines():
        line = line.strip()
        if prefix in line:
            fragment = line.split(prefix, 1)[1].split(suffix, 1)[0].strip()
            try:
                return int(fragment)
            except ValueError:
                return None
    return None


def run_checks(checks: list[dict], workdir: Path) -> list[dict]:
    results = []
    for check in checks:
        kind = check["type"]
        if kind == "file_contains":
            target = workdir / check["path"]
            passed = target.is_file() and check["text"] in target.read_text(errors="replace")
            results.append({"check": check, "passed": passed})
        elif kind == "file_exists":
            results.append({"check": check, "passed": (workdir / check["path"]).exists()})
        elif kind == "command":
            try:
                completed = subprocess.run(
                    check["run"],
                    shell=True,
                    cwd=workdir,
                    capture_output=True,
                    text=True,
                    timeout=check.get("timeout_secs", 300),
                )
                expected = check.get("expect_exit", 0)
                results.append(
                    {
                        "check": check,
                        "passed": completed.returncode == expected,
                        "output_tail": (completed.stdout + completed.stderr)[-1000:],
                    }
                )
            except subprocess.TimeoutExpired:
                results.append({"check": check, "passed": False, "output_tail": "timeout"})
        else:
            results.append({"check": check, "passed": False, "output_tail": "unknown check type"})
    return results


def main() -> int:
    parser = argparse.ArgumentParser(description="Heretek evaluation runner")
    parser.add_argument("--agent", default="heretek run", help="agent command prefix")
    parser.add_argument("--label", default="heretek", help="label for this run")
    parser.add_argument("--tasks", default=str(ROOT / "tasks"), help="task directory")
    parser.add_argument("--model-config", default=None, help=".heretek.toml to install in each fixture")
    parser.add_argument("--out", default=str(ROOT / "results"), help="results directory")
    parser.add_argument("--timeout", type=int, default=1800, help="per-task timeout in seconds")
    args = parser.parse_args()

    tasks = load_tasks(Path(args.tasks))
    if not tasks:
        print("no tasks found", file=sys.stderr)
        return 2

    out_dir = Path(args.out)
    out_dir.mkdir(parents=True, exist_ok=True)
    report = {
        "label": args.label,
        "agent": args.agent,
        "started": time.strftime("%Y-%m-%dT%H:%M:%S%z"),
        "tasks": [],
    }

    passed_count = 0
    for task in tasks:
        print(f"task {task['id']} ...", flush=True)
        with tempfile.TemporaryDirectory(prefix=f"heretek-eval-{task['id']}-") as temp:
            workdir = Path(temp) / "repo"
            prepare_workspace(task, workdir)
            install_model_config(workdir, Path(args.model_config) if args.model_config else None)
            run = run_agent(args.agent, task["task"], workdir, task["max_turns"], args.timeout)
            checks = run_checks(task["checks"], workdir)
        task_passed = run["exit_code"] == 0 and all(check["passed"] for check in checks)
        passed_count += int(task_passed)
        report["tasks"].append(
            {
                "id": task["id"],
                "description": task.get("description", ""),
                "passed": task_passed,
                "checks": checks,
                **run,
            }
        )
        print(f"  -> {'PASS' if task_passed else 'FAIL'} (exit {run['exit_code']}, {run['wall_secs']}s)")

    report["passed"] = passed_count
    report["total"] = len(tasks)
    report["pass_rate"] = round(passed_count / len(tasks), 4)
    stamp = time.strftime("%Y%m%d-%H%M%S")
    result_path = out_dir / f"{stamp}-{args.label}.json"
    result_path.write_text(json.dumps(report, indent=2))
    print(f"pass rate: {passed_count}/{len(tasks)}")
    print(f"report: {result_path}")
    print(
        "note: compare against a baseline run (`--label baseline`) with the same tasks; "
        "false rejections are runs where the agent exit was non-zero but all checks passed"
    )
    return 0 if passed_count == len(tasks) else 1


if __name__ == "__main__":
    raise SystemExit(main())
