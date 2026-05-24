#!/usr/bin/env python3
"""bench.py — RAG vs no-RAG benchmark for cofe-graph MCP server

Measures AI agent response quality on Flipper Zero firmware C code understanding,
comparing results with and without cofe-graph as a RAG tool via MCP.

Usage:
    python bench.py --model <model> --base-url <url> [options]

Examples:
    # All 12 questions, both conditions, via local Ollama:
    python bench.py --model qwen2.5-coder:7b --base-url http://localhost:11434/v1

    # Only Q2, without-MCP only:
    python bench.py --model qwen2.5-coder:7b --base-url http://localhost:11434/v1 \\
        --condition without --question Q2

    # Via OpenRouter:
    python bench.py --model openai/gpt-4o --base-url https://openrouter.ai/api/v1 \\
        --api-key $OPENROUTER_API_KEY
"""

import os
import sys

if sys.version_info < (3, 10):
    sys.exit(
        f"[bench] Python 3.10+ required (found {sys.version_info.major}.{sys.version_info.minor}).\n"
        "        Install a newer Python and retry."
    )

_IS_WIN = sys.platform == "win32"
_VENV_BIN = "Scripts" if _IS_WIN else "bin"
_PY_EXE = "python.exe" if _IS_WIN else "python"
_PIP_EXE = "pip.exe" if _IS_WIN else "pip"


def _bootstrap() -> None:
    if sys.prefix != sys.base_prefix:  # already inside a venv
        return
    here = os.path.dirname(os.path.abspath(__file__))
    venv = os.path.join(here, ".venv")
    python = os.path.join(venv, _VENV_BIN, _PY_EXE)
    if not os.path.isfile(python):
        import subprocess

        print("[bench] Creating .venv ...", file=sys.stderr)
        subprocess.check_call([sys.executable, "-m", "venv", venv])
        req = os.path.join(here, "requirements.txt")
        if os.path.isfile(req):
            print("[bench] Installing dependencies ...", file=sys.stderr)
            subprocess.check_call(
                [os.path.join(venv, _VENV_BIN, _PIP_EXE), "install", "-q", "-r", req]
            )
    # Use subprocess instead of os.execv for reliable cross-platform re-exec
    import subprocess

    result = subprocess.run([python] + sys.argv)
    sys.exit(result.returncode)


_bootstrap()

import argparse
import datetime
import json
import shutil
import subprocess
import tempfile
import time
from pathlib import Path
from typing import Any, Optional

from dotenv import load_dotenv

load_dotenv()

SCRIPT_DIR = Path(__file__).resolve().parent
REPO_ROOT = SCRIPT_DIR.parent

TEST_CASES = json.loads((SCRIPT_DIR / "test_cases.json").read_text(encoding="utf-8"))


def make_question(task: dict) -> str:
    return task["question"]


_OC_BUILTIN_TOOLS = {
    "bash",
    "edit",
    "write",
    "read",
    "list_directory",
    "grep",
    "glob",
    "webfetch",
}


def find_cofe_bin(override: Optional[str]) -> Path:
    if override:
        p = Path(override)
        if not p.is_file():
            raise FileNotFoundError(f"cofe-graph binary not found: {p}")
        return p
    exe = "cofe-graph.exe" if _IS_WIN else "cofe-graph"
    for candidate in [
        REPO_ROOT / "target" / "release" / exe,
        REPO_ROOT / "target" / "debug" / exe,
    ]:
        if candidate.is_file():
            return candidate
    raise FileNotFoundError(
        "cofe-graph binary not found. Build it first:\n  cargo build --release"
    )


def flush_condition(run_dir: Path, condition: str, records: list[dict]) -> None:
    """Overwrite results/{ts}/{condition}.json with the current list (pretty-printed)."""
    out = run_dir / f"{condition}.json"
    out.write_text(
        json.dumps(records, indent=2, ensure_ascii=False) + "\n", encoding="utf-8"
    )
    print(f"  → {out.relative_to(SCRIPT_DIR)}")


def run_opencode(
    question: str,
    model: str,
    base_url: str,
    api_key: str,
    project_path: Path,
    *,
    with_mcp: bool,
    cofe_bin: Optional[Path] = None,
) -> tuple[str, list, dict]:
    """Run opencode for one benchmark question.

    WITHOUT condition (with_mcp=False): copies firmware to a tmpdir that excludes
    .cofe-graph so the agent cannot see the pre-built index.

    WITH condition (with_mcp=True): uses the original project path (which contains
    .cofe-graph) and enables the cofe-graph MCP server.

    Returns (response_text, tool_calls_log, usage_dict).
    """
    tmpdir: Optional[Path] = None
    try:
        if not with_mcp:
            # Shadow the index: copy source tree without .cofe-graph.
            # Copy directly into tmpdir (not a subdir) so the directory
            # layout matches the with-MCP condition and relative paths
            # derived from grep output work correctly.
            tmpdir = Path(tempfile.mkdtemp(prefix="bench_without_"))
            actual_dir = tmpdir
            shutil.copytree(
                str(project_path),
                str(tmpdir),
                dirs_exist_ok=True,
                ignore=shutil.ignore_patterns(".cofe-graph"),
            )
        else:
            actual_dir = project_path

        # Build OPENCODE_CONFIG_CONTENT
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
        if with_mcp and cofe_bin:
            # Use short name without hyphens so tool names become "cg_find_function"
            # instead of "cofe-graph_find_function". This prevents the model from
            # wasting a round-trip guessing the separator on every question.
            config["mcp"] = {
                "cg": {
                    "type": "local",
                    "command": [str(cofe_bin), str(actual_dir), "--quiet"],
                    "enabled": True,
                }
            }
            # Deny all built-in filesystem/shell tools so the model is forced to
            # use only cg_* MCP tools. Config-level deny is stronger than a prompt
            # instruction and doesn't waste a round-trip on a refused call.
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
                "When searching for C source files, use include=\"**/*.c\" (double asterisk, for recursive search).\n\n"
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
            str(actual_dir),
            prompt,
        ]

        result = subprocess.run(
            cmd, capture_output=True, text=True, timeout=300, env=env
        )

        # Parse ndjson event stream
        text_parts: list[str] = []
        tool_calls_log: list[dict] = []
        total_in = 0
        total_out = 0

        for line in result.stdout.splitlines():
            line = line.strip()
            if not line:
                continue
            try:
                event = json.loads(line)
            except json.JSONDecodeError:
                continue

            etype = event.get("type")
            part = event.get("part", {})

            if etype == "text":
                text_parts.append(part.get("text", ""))
            elif etype == "tool_use":
                state = part.get("state", {})
                tool_name = part.get("tool", "")
                tool_args = state.get("input", {})
                tag = "fs " if tool_name in _OC_BUILTIN_TOOLS else "mcp"
                print(f"    [{tag}] {tool_name}({json.dumps(tool_args)[:100]})")
                tool_calls_log.append(
                    {
                        "tool": tool_name,
                        "args": tool_args,
                        "result_preview": str(state.get("output", ""))[:300],
                    }
                )
            elif etype == "step_finish":
                tokens = part.get("tokens", {})
                total_in += tokens.get("input", 0)
                total_out += tokens.get("output", 0)

        response = "".join(text_parts).strip()
        usage = {"input_tokens": total_in, "output_tokens": total_out}
        return response, tool_calls_log, usage
    finally:
        if tmpdir:
            shutil.rmtree(tmpdir, ignore_errors=True)


def _parse_args() -> argparse.Namespace:
    p = argparse.ArgumentParser(
        description=(
            "Benchmark AI agent with/without cofe-graph MCP RAG "
            "on Flipper Zero firmware C code understanding."
        ),
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog=__doc__,
    )
    p.add_argument(
        "--model",
        required=True,
        help="Model name (e.g. qwen2.5-coder:7b, openai/gpt-4o)",
    )
    p.add_argument(
        "--base-url",
        required=True,
        help="OpenAI-compatible base URL (e.g. http://localhost:11434/v1)",
    )
    p.add_argument(
        "--api-key",
        default=None,
        help="API key — defaults to OPENROUTER_API_KEY env var, then 'ollama'",
    )
    p.add_argument(
        "--condition",
        choices=["with", "without", "both"],
        default="both",
        help="Which condition to run (default: both)",
    )
    p.add_argument(
        "--question",
        nargs="+",
        metavar="QN",
        help="Run only specific question IDs, e.g. --question Q1 Q3 Q12",
    )
    p.add_argument(
        "--cofe-graph-bin",
        default=None,
        metavar="PATH",
        help="Path to cofe-graph binary (default: auto-detect release then debug)",
    )
    p.add_argument(
        "--project-path",
        default=None,
        metavar="PATH",
        help=(
            "Project path passed to cofe-graph for indexing "
            "(default: tests/flipperzero-firmware-dev/applications)"
        ),
    )
    return p.parse_args()


def main() -> None:
    args = _parse_args()

    api_key: str = args.api_key or os.environ.get("OPENROUTER_API_KEY") or "ollama"

    project_path = (
        Path(args.project_path)
        if args.project_path
        else (REPO_ROOT / "tests" / "flipperzero-firmware-dev" / "applications")
    )
    if not project_path.is_dir():
        sys.exit(f"[error] project-path not found: {project_path}")

    # Create per-run output directory: results/{timestamp}/
    ts = datetime.datetime.now(datetime.timezone.utc).strftime("%Y%m%d_%H%M%S")
    run_dir = SCRIPT_DIR / "results" / ts
    run_dir.mkdir(parents=True, exist_ok=True)

    # Accumulate records per condition so we can rewrite the file incrementally
    accumulated: dict[str, list[dict]] = {"with": [], "without": []}

    # Filter tasks by question IDs
    tasks = TEST_CASES
    if args.question:
        ids = {q.upper() for q in args.question}
        tasks = [t for t in TEST_CASES if t["id"] in ids]
        if not tasks:
            available = [t["id"] for t in TEST_CASES]
            sys.exit(f"[error] No matching IDs in {ids}. Available: {available}")

    conditions: list[str] = (
        ["with", "without"] if args.condition == "both" else [args.condition]
    )

    # Resolve binary only when the 'with' condition is needed
    cofe_bin: Optional[Path] = None
    if "with" in conditions:
        try:
            cofe_bin = find_cofe_bin(args.cofe_graph_bin)
            print(f"[bench] cofe-graph binary : {cofe_bin}")
        except FileNotFoundError as exc:
            sys.exit(f"[error] {exc}")

    print(f"[bench] model       : {args.model}")
    print(f"[bench] base_url    : {args.base_url}")
    print(f"[bench] conditions  : {conditions}")
    print(f"[bench] tasks       : {[t['id'] for t in tasks]}")
    print(f"[bench] run dir     : {run_dir.relative_to(SCRIPT_DIR)}\n")

    for task in tasks:
        question = make_question(task)
        for condition in conditions:
            print(f"── [{task['id']}] condition={condition}  fn={task['function']}")
            t0 = time.monotonic()

            response, tc_log, usage = run_opencode(
                question,
                args.model,
                args.base_url,
                api_key,
                project_path,
                with_mcp=(condition == "with"),
                cofe_bin=cofe_bin if condition == "with" else None,
            )

            duration = time.monotonic() - t0
            print(
                f"  done in {duration:.1f}s  |  tool_calls={len(tc_log)}"
                f"  |  in={usage['input_tokens']} out={usage['output_tokens']}"
            )

            record: dict = {
                "timestamp": datetime.datetime.now(datetime.timezone.utc).isoformat(),
                "task_id": task["id"],
                "function_name": task["function"],
                "condition": condition,
                "model": args.model,
                "base_url": args.base_url,
                "question": question,
                "response": response,
                "tool_calls_log": tc_log,
                "num_tool_calls": len(tc_log),
                "input_tokens": usage["input_tokens"],
                "output_tokens": usage["output_tokens"],
                "duration_sec": round(duration, 2),
            }
            accumulated[condition].append(record)
            flush_condition(run_dir, condition, accumulated[condition])
            print()


if __name__ == "__main__":
    main()
