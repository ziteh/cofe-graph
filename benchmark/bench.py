#!/usr/bin/env python3
"""
bench.py — RAG vs no-RAG benchmark for code indexer MCP server

Measures AI agent response quality on Flipper Zero firmware C code understanding,
comparing results with and without code analysis as a RAG tool via MCP.

Usage:
  uv run bench.py --model <model> --base-url <url> [options]

Examples:
  # All questions, both conditions, via local Ollama:
  uv run bench.py --model qwen2.5-coder:7b --base-url http://localhost:11434/v1

  # Only Q2, without-MCP only:
  uv run bench.py --model qwen2.5-coder:7b --base-url http://localhost:11434/v1 \\
      --condition without --question Q2

  # Via OpenRouter:
  uv run bench.py --model openai/gpt-4o --base-url https://openrouter.ai/api/v1 \\
      --api-key $OPENROUTER_API_KEY
"""

import argparse
import json
import os
import platform
import shutil
import subprocess
import sys
import tempfile
import time
from dataclasses import asdict, dataclass
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

from dotenv import load_dotenv

load_dotenv()

SCRIPT_DIR = Path(__file__).resolve().parent
REPO_ROOT = SCRIPT_DIR.parent

OC_BUILTIN_TOOLS = {
    "bash",
    "edit",
    "write",
    "read",
    "list_directory",
    "grep",
    "glob",
    "webfetch",
}


@dataclass
class Criterion:
    criterion: str
    points: int


@dataclass
class TestCase:
    id: str
    difficulty_weight: float
    function: str
    question: str
    rubric: list[Criterion]


@dataclass
class ToolCallLog:
    tool: str
    args: Any
    result_preview: str


@dataclass
class BenchRecord:
    timestamp: str
    task_id: str
    function_name: str
    condition: str
    model: str
    base_url: str
    question: str
    response: str
    tool_calls_log: list[dict[str, Any]]
    num_tool_calls: int
    input_tokens: int
    output_tokens: int
    duration_sec: float


def _load_test_cases() -> list[TestCase]:
    data = json.loads((SCRIPT_DIR / "test_cases.json").read_text(encoding="utf-8"))
    return [
        TestCase(
            id=tc["id"],
            difficulty_weight=tc["difficulty_weight"],
            function=tc["function"],
            question=tc["question"],
            rubric=[Criterion(**c) for c in tc["rubric"]],
        )
        for tc in data
    ]


TEST_CASES = _load_test_cases()


def find_server_binary(override: str | None = None) -> str:
    if override:
        if not Path(override).exists():
            raise FileNotFoundError(f"Server binary not found: {override}")
        return override
    exe = "cofe-graph.exe" if platform.system() == "Windows" else "cofe-graph"
    for candidate in [
        REPO_ROOT / "target" / "release" / exe,
        REPO_ROOT / "target" / "debug" / exe,
    ]:
        if candidate.exists():
            return str(candidate)
    raise FileNotFoundError(
        "Server binary not found. Build it first:\n  cargo build --release"
    )


def flush_condition(run_dir: Path, condition: str, records: list[BenchRecord]) -> None:
    out = run_dir / f"{condition}.json"
    out.write_text(
        json.dumps([asdict(r) for r in records], indent=2) + "\n", encoding="utf-8"
    )
    print(f"  → {out.relative_to(SCRIPT_DIR)}")


def run_opencode(
    question: str,
    model: str,
    base_url: str,
    api_key: str,
    project_path: str,
    with_mcp: bool,
    server_binary: str | None = None,
) -> tuple[str, list[ToolCallLog], dict[str, int]]:
    tmp_dir: str | None = None
    try:
        if not with_mcp:
            tmp_dir = tempfile.mkdtemp(prefix="bench_without_")
            shutil.copytree(
                project_path,
                tmp_dir,
                dirs_exist_ok=True,
                ignore=shutil.ignore_patterns(".cofe-graph"),
            )
            actual_dir = tmp_dir
        else:
            actual_dir = project_path

        config: dict[str, Any] = {
            "provider": {
                "bench": {
                    "npm": "@ai-sdk/openai-compatible",
                    "name": "bench",
                    "options": {"baseURL": base_url, "apiKey": api_key},
                    "models": {model: {}},
                }
            }
        }

        if with_mcp and server_binary:
            config["mcp"] = {
                "cg": {
                    "type": "local",
                    "command": [server_binary, actual_dir, "--quiet"],
                    "enabled": True,
                }
            }
            config["permission"] = {
                "read": "deny",
                "edit": "deny",
                "write": "deny",
                "glob": "deny",
                "grep": "deny",
                "bash": "deny",
                "list": "deny",
                "webfetch": "deny",
            }

        if with_mcp:
            context = (
                "You are analyzing a C firmware codebase.\n"
                "Use the cg_* graph-RAG tools to look up functions and source code.\n"
                "To retrieve a function's source: call cg_get_source(name).\n"
                "Do not rely on prior knowledge about this specific project.\n\n"
            )
        else:
            context = (
                "You are analyzing a C firmware codebase at the current directory.\n"
                "ALWAYS use tools to read actual source files before answering.\n"
                "Do not rely on prior knowledge about this specific project.\n"
                'When searching for C source files, use include="**/*.c" (double asterisk, for recursive search).\n\n'
            )

        prompt = context + question

        env = {**os.environ, "OPENCODE_CONFIG_CONTENT": json.dumps(config)}
        cmd = [
            "opencode",
            "run",
            "--model",
            f"bench/{model}",
            "--format",
            "json",
            "--dir",
            actual_dir,
            prompt,
        ]

        result = subprocess.run(
            cmd,
            capture_output=True,
            text=True,
            timeout=300,
            env=env,
        )

        text_parts: list[str] = []
        tool_calls_log: list[ToolCallLog] = []
        total_in = 0
        total_out = 0

        for line in (result.stdout or "").splitlines():
            trimmed = line.strip()
            if not trimmed:
                continue
            try:
                event: dict[str, Any] = json.loads(trimmed)
            except json.JSONDecodeError:
                continue

            etype: str | None = event.get("type")
            part: dict[str, Any] = event.get("part") or {}

            if etype == "text":
                text_parts.append(part.get("text") or "")
            elif etype == "tool_use":
                state: dict[str, Any] = part.get("state") or {}
                tool_name: str = part.get("tool") or ""
                tool_args: dict[str, Any] = state.get("input") or {}
                tag = "fs " if tool_name in OC_BUILTIN_TOOLS else "mcp"
                print(f"    [{tag}] {tool_name}({json.dumps(tool_args)[:100]})")
                tool_calls_log.append(
                    ToolCallLog(
                        tool=tool_name,
                        args=tool_args,
                        result_preview=str(state.get("output") or "")[:300],
                    )
                )
            elif etype == "step_finish":
                tokens: dict[str, Any] = part.get("tokens") or {}
                total_in += int(tokens.get("input") or 0)
                total_out += int(tokens.get("output") or 0)

        response = "".join(text_parts).strip()
        return (
            response,
            tool_calls_log,
            {"input_tokens": total_in, "output_tokens": total_out},
        )
    finally:
        if tmp_dir:
            shutil.rmtree(tmp_dir, ignore_errors=True)


def main() -> None:
    parser = argparse.ArgumentParser(
        description="Benchmark AI agent with/without code indexer MCP RAG on Flipper Zero firmware C code understanding."
    )
    parser.add_argument(
        "--model",
        required=True,
        help="Model name (e.g. qwen2.5-coder:7b, openai/gpt-4o)",
    )
    parser.add_argument(
        "--base-url", required=True, dest="base_url", help="OpenAI-compatible base URL"
    )
    parser.add_argument(
        "--api-key",
        dest="api_key",
        help="API key — defaults to OPENROUTER_API_KEY env var, then 'ollama'",
    )
    parser.add_argument(
        "--condition", default="both", choices=["with", "without", "both"]
    )
    parser.add_argument(
        "--question",
        nargs="+",
        help="Run only specific question IDs, e.g. --question Q1 Q3",
    )
    parser.add_argument(
        "--bin", help="Path to server binary (default: auto-detect release then debug)"
    )
    parser.add_argument(
        "--project-path", dest="project_path", help="Project path to analyze"
    )
    args = parser.parse_args()

    api_key = args.api_key or os.environ.get("OPENROUTER_API_KEY") or "ollama"

    project_path = (
        Path(args.project_path).resolve()
        if args.project_path
        else REPO_ROOT / "tests" / "flipperzero-firmware-dev" / "applications"
    )

    if not project_path.exists():
        print(f"[error] project-path not found: {project_path}", file=sys.stderr)
        sys.exit(1)

    ts = datetime.now(timezone.utc).strftime("%Y%m%d_%H%M%S")
    run_dir = SCRIPT_DIR / "results" / ts
    run_dir.mkdir(parents=True, exist_ok=True)

    accumulated: dict[str, list[BenchRecord]] = {"with": [], "without": []}

    tasks = TEST_CASES
    if args.question:
        ids = {q.upper() for q in args.question}
        tasks = [t for t in TEST_CASES if t.id in ids]
        if not tasks:
            available = [t.id for t in TEST_CASES]
            print(
                f"[error] No matching IDs in {sorted(ids)}. Available: {available}",
                file=sys.stderr,
            )
            sys.exit(1)

    conditions: list[str] = (
        ["with", "without"] if args.condition == "both" else [args.condition]
    )

    server_binary: str | None = None
    if "with" in conditions:
        try:
            server_binary = find_server_binary(args.bin)
            print(f"[bench] server binary : {server_binary}")
        except FileNotFoundError as e:
            print(f"[error] {e}", file=sys.stderr)
            sys.exit(1)

    print(f"[bench] model       : {args.model}")
    print(f"[bench] base_url    : {args.base_url}")
    print(f"[bench] conditions  : {conditions}")
    print(f"[bench] tasks       : {[t.id for t in tasks]}")
    print(f"[bench] run dir     : {run_dir.relative_to(SCRIPT_DIR)}\n")

    for task in tasks:
        for condition in conditions:
            print(f"── [{task.id}] condition={condition}  fn={task.function}")
            t0 = time.perf_counter()

            response, tc_log, usage = run_opencode(
                task.question,
                args.model,
                args.base_url,
                api_key,
                str(project_path),
                condition == "with",
                server_binary if condition == "with" else None,
            )

            duration = time.perf_counter() - t0
            print(
                f"  done in {duration:.1f}s  |  tool_calls={len(tc_log)}"
                f"  |  in={usage['input_tokens']} out={usage['output_tokens']}"
            )

            record = BenchRecord(
                timestamp=datetime.now(timezone.utc).isoformat(),
                task_id=task.id,
                function_name=task.function,
                condition=condition,
                model=args.model,
                base_url=args.base_url,
                question=task.question,
                response=response,
                tool_calls_log=[asdict(tc) for tc in tc_log],
                num_tool_calls=len(tc_log),
                input_tokens=usage["input_tokens"],
                output_tokens=usage["output_tokens"],
                duration_sec=round(duration, 2),
            )
            accumulated[condition].append(record)
            flush_condition(run_dir, condition, accumulated[condition])
            print()


if __name__ == "__main__":
    main()
