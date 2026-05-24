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

# ── venv bootstrap ─────────────────────────────────────────────────────────────
# If the script is run outside a venv, auto-create .venv, install deps, re-exec.
import os
import sys


def _bootstrap() -> None:
    if sys.prefix != sys.base_prefix:  # already inside a venv
        return
    here = os.path.dirname(os.path.abspath(__file__))
    venv = os.path.join(here, ".venv")
    python = os.path.join(venv, "bin", "python")
    if not os.path.isfile(python):
        import subprocess

        print("[bench] Creating .venv ...", file=sys.stderr)
        subprocess.check_call([sys.executable, "-m", "venv", venv])
        req = os.path.join(here, "requirements.txt")
        if os.path.isfile(req):
            print("[bench] Installing dependencies ...", file=sys.stderr)
            subprocess.check_call(
                [os.path.join(venv, "bin", "pip"), "install", "-q", "-r", req]
            )
    os.execv(python, [python] + sys.argv)


_bootstrap()

# ── stdlib ────────────────────────────────────────────────────────────────────
import argparse
import asyncio
import datetime
import json
import shlex
import shutil
import subprocess
import tempfile
import time
from pathlib import Path
from typing import Any, Optional

# ── third-party ───────────────────────────────────────────────────────────────
from dotenv import load_dotenv
from mcp import ClientSession, StdioServerParameters
from mcp.client.stdio import stdio_client
from openai import OpenAI

load_dotenv()

# ── repo layout ───────────────────────────────────────────────────────────────
SCRIPT_DIR = Path(__file__).resolve().parent
REPO_ROOT = SCRIPT_DIR.parent

# ── test cases ────────────────────────────────────────────────────────────────
from test_cases import TEST_CASES


def make_question(task: dict) -> str:
    return task["question"]


SYSTEM_WITHOUT = (
    "You are an experienced embedded C developer. "
    "You have access to shell and filesystem tools (bash, read, grep, glob) "
    "to explore a codebase. "
    "ALWAYS use the tools to locate and read the actual source files before answering. "
    "Base every detail strictly on what you find in the code — "
    "do not guess or invent anything. "
    "The filesystem is read-only; write and edit tools are unavailable."
)

SYSTEM_WITH = (
    "You are an experienced embedded C developer. "
    "You have access to shell and filesystem tools (bash, read, grep, glob) "
    "AND cofe-graph semantic tools (get_source, callers, callees, context, etc.) "
    "that let you inspect the live codebase. "
    "ALWAYS use the tools to read the actual source before answering. "
    "Prefer cofe-graph tools for semantic queries (function signatures, call graphs, types). "
    "Base every detail strictly on what you find in the code — "
    "do not rely on any prior knowledge about this project. "
    "The filesystem is read-only; write and edit tools are unavailable."
)

# ── basic agent tools (WITHOUT condition, opencode-style) ───────────────────
BASIC_TOOLS: list[dict] = [
    {
        "type": "function",
        "function": {
            "name": "bash",
            "description": (
                "Run a shell command in the project root directory. "
                "Returns stdout and stderr combined. Use for one-off commands "
                "such as find, cat, wc, tree, etc. "
            ),
            "parameters": {
                "type": "object",
                "properties": {
                    "command": {
                        "type": "string",
                        "description": "Shell command to execute.",
                    },
                    "timeout": {
                        "type": "integer",
                        "description": "Timeout in seconds (default 30).",
                    },
                },
                "required": ["command"],
            },
        },
    },
    {
        "type": "function",
        "function": {
            "name": "read",
            "description": "Read the full contents of a file.",
            "parameters": {
                "type": "object",
                "properties": {
                    "file_path": {
                        "type": "string",
                        "description": "Absolute or project-relative path.",
                    },
                },
                "required": ["file_path"],
            },
        },
    },
    {
        "type": "function",
        "function": {
            "name": "write",
            "description": "Create or overwrite a file with the given content.",
            "parameters": {
                "type": "object",
                "properties": {
                    "file_path": {
                        "type": "string",
                        "description": "Absolute or project-relative path.",
                    },
                    "content": {"type": "string", "description": "Content to write."},
                },
                "required": ["file_path", "content"],
            },
        },
    },
    {
        "type": "function",
        "function": {
            "name": "edit",
            "description": (
                "Replace an exact, unique string in a file with a new string. "
                "The old_string must appear exactly once."
            ),
            "parameters": {
                "type": "object",
                "properties": {
                    "file_path": {
                        "type": "string",
                        "description": "Absolute or project-relative path.",
                    },
                    "old_string": {
                        "type": "string",
                        "description": "Exact string to replace.",
                    },
                    "new_string": {
                        "type": "string",
                        "description": "Replacement string.",
                    },
                },
                "required": ["file_path", "old_string", "new_string"],
            },
        },
    },
    {
        "type": "function",
        "function": {
            "name": "grep",
            "description": (
                "Search for a regex or literal pattern in files. "
                "Returns matching lines with file:line prefix (max 200 lines)."
            ),
            "parameters": {
                "type": "object",
                "properties": {
                    "pattern": {
                        "type": "string",
                        "description": "Pattern to search for.",
                    },
                    "path": {
                        "type": "string",
                        "description": "File or directory to search in.",
                    },
                    "flags": {
                        "type": "string",
                        "description": "grep flags string, e.g. '-r -n -i' (default: '-r -n').",
                    },
                },
                "required": ["pattern", "path"],
            },
        },
    },
    {
        "type": "function",
        "function": {
            "name": "glob",
            "description": "Find files matching a glob pattern under a base directory.",
            "parameters": {
                "type": "object",
                "properties": {
                    "pattern": {
                        "type": "string",
                        "description": "Glob pattern, e.g. '**/*.c'.",
                    },
                    "path": {
                        "type": "string",
                        "description": "Base directory to search from.",
                    },
                },
                "required": ["pattern", "path"],
            },
        },
    },
]


# ── Docker sandbox ─────────────────────────────────────────────────────────────

BASIC_TOOL_NAMES = {"bash", "read", "write", "edit", "grep", "glob"}


class DockerSandbox:
    """Ephemeral alpine container for sandboxed filesystem tool execution.

    Firmware source is mounted read-only at /project inside the container.
    All tool calls execute via ``docker exec``.
    """

    IMAGE = "bench-sandbox"
    CONTAINER_ROOT = "/project"

    def __init__(self, firmware_src: Path, cofe_tmpdir: Optional[Path] = None) -> None:
        self.firmware_src = firmware_src.resolve()
        self.cofe_tmpdir = cofe_tmpdir.resolve() if cofe_tmpdir else None
        ts = datetime.datetime.now().strftime("%Y%m%d_%H%M%S_%f")
        self._name = f"bench_{ts}"
        # Temp empty dir mounted over /project/.cofe-graph to hide any
        # pre-existing index/logs from the model's filesystem tools.
        self._shadow_dir: Optional[Path] = None

    def to_container_path(self, host_path: str) -> str:
        """Map an absolute host path to /project/... inside the container.

        Accepts paths under firmware_src or cofe_tmpdir.  Returns the path
        unchanged if it already starts with CONTAINER_ROOT or is relative.
        """
        if not host_path or host_path.startswith(self.CONTAINER_ROOT):
            return host_path or self.CONTAINER_ROOT
        p = Path(host_path)
        if not p.is_absolute():
            return host_path  # relative — cwd inside container is /project
        for base in filter(None, [self.cofe_tmpdir, self.firmware_src]):
            try:
                rel = p.relative_to(base)
                return f"{self.CONTAINER_ROOT}/{rel}"
            except ValueError:
                continue
        return host_path  # outside known roots — pass through

    def start(self) -> None:
        self._shadow_dir = Path(tempfile.mkdtemp(prefix="bench_shadow_"))
        subprocess.run(
            [
                "docker", "run", "-d", "--rm",
                "--name", self._name,
                "-v", f"{self.firmware_src}:{self.CONTAINER_ROOT}:ro",
                # Shadow any pre-existing .cofe-graph/ so the model cannot see it
                "-v", f"{self._shadow_dir}:{self.CONTAINER_ROOT}/.cofe-graph:ro",
                "-w", self.CONTAINER_ROOT,
                "--network", "none",
                self.IMAGE, "sleep", "infinity",
            ],
            check=True,
            capture_output=True,
        )

    def stop(self) -> None:
        subprocess.run(["docker", "kill", self._name], capture_output=True)
        if self._shadow_dir:
            shutil.rmtree(self._shadow_dir, ignore_errors=True)
            self._shadow_dir = None

    def exec(self, cmd: str, timeout: int = 30) -> str:
        try:
            result = subprocess.run(
                ["docker", "exec", self._name, "sh", "-c", cmd],
                capture_output=True,
                text=True,
                timeout=timeout,
            )
            out = result.stdout
            if result.stderr:
                out += "\n[stderr]\n" + result.stderr
            return out[:8000] or "[no output]"
        except subprocess.TimeoutExpired:
            return f"[error] timed out after {timeout}s"
        except Exception as exc:
            return f"[error] {exc}"

    def __enter__(self) -> "DockerSandbox":
        self.start()
        return self

    def __exit__(self, *_: Any) -> None:
        self.stop()


def _execute_basic_tool(name: str, args: dict, sandbox: DockerSandbox) -> str:
    if name == "bash":
        cmd = args.get("command", "")
        timeout = int(args.get("timeout", 30))
        return sandbox.exec(cmd, timeout=timeout)

    if name == "read":
        container_path = sandbox.to_container_path(args.get("file_path", ""))
        return sandbox.exec(f"cat {shlex.quote(container_path)}")

    if name in ("write", "edit"):
        return "[sandbox] filesystem is read-only — write/edit not available"

    if name == "grep":
        pattern = args.get("pattern", "")
        path = sandbox.to_container_path(
            args.get("path", DockerSandbox.CONTAINER_ROOT)
        )
        flags = args.get("flags", "-r -n")
        cmd = (
            f"grep --exclude-dir=.cofe-graph {flags} "
            f"{shlex.quote(pattern)} {shlex.quote(path)}"
        )
        result = sandbox.exec(cmd)
        lines = result.splitlines()[:200]
        return "\n".join(lines) if lines else f"[no matches for '{pattern}']"

    if name == "glob":
        pattern = args.get("pattern", "")
        base = sandbox.to_container_path(
            args.get("path", DockerSandbox.CONTAINER_ROOT)
        )
        name_pat = Path(pattern).name or "*"
        if "**" in pattern or "/" in pattern:
            cmd = f"find {shlex.quote(base)} -name {shlex.quote(name_pat)} | sort | head -200"
        else:
            cmd = f"find {shlex.quote(base)} -maxdepth 1 -name {shlex.quote(name_pat)} | sort"
        result = sandbox.exec(cmd).strip()
        return result if result else f"[no files matching '{pattern}']"

    return f"[unknown tool] {name}"


# ── MCP ↔ OpenAI tool conversion ──────────────────────────────────────────────


def _mcp_to_openai_tool(tool: Any) -> dict:
    schema: dict = getattr(tool, "inputSchema", None) or {}
    if "type" not in schema:
        schema = {"type": "object", "properties": {}}
    return {
        "type": "function",
        "function": {
            "name": tool.name,
            "description": getattr(tool, "description", "") or "",
            "parameters": schema,
        },
    }


# ── helpers ───────────────────────────────────────────────────────────────────


def find_cofe_bin(override: Optional[str]) -> Path:
    if override:
        p = Path(override)
        if not p.is_file():
            raise FileNotFoundError(f"cofe-graph binary not found: {p}")
        return p
    for candidate in [
        REPO_ROOT / "target" / "release" / "cofe-graph",
        REPO_ROOT / "target" / "debug" / "cofe-graph",
    ]:
        if candidate.is_file():
            return candidate
    raise FileNotFoundError(
        "cofe-graph binary not found. Run `cargo build --release` first."
    )


def flush_condition(run_dir: Path, condition: str, records: list[dict]) -> None:
    """Overwrite results/{ts}/{condition}.json with the current list (pretty-printed)."""
    out = run_dir / f"{condition}.json"
    out.write_text(
        json.dumps(records, indent=2, ensure_ascii=False) + "\n", encoding="utf-8"
    )
    print(f"  → {out.relative_to(SCRIPT_DIR)}")


def build_llm_client(base_url: str, api_key: str) -> OpenAI:
    return OpenAI(base_url=base_url, api_key=api_key)


def _check_docker() -> None:
    result = subprocess.run(["docker", "info"], capture_output=True)
    if result.returncode != 0:
        sys.exit(
            "[error] Docker is required but not available. "
            "Start Docker Desktop and retry."
        )


def _ensure_sandbox_image() -> None:
    result = subprocess.run(
        ["docker", "image", "inspect", DockerSandbox.IMAGE],
        capture_output=True,
    )
    if result.returncode != 0:
        print(
            f"[bench] Building Docker image '{DockerSandbox.IMAGE}' ...",
            file=sys.stderr,
        )
        dockerfile = SCRIPT_DIR / "Dockerfile.sandbox"
        subprocess.run(
            [
                "docker", "build",
                "-t", DockerSandbox.IMAGE,
                "-f", str(dockerfile),
                str(SCRIPT_DIR),
            ],
            check=True,
        )


# ── benchmark runners ─────────────────────────────────────────────────────────


async def run_without_rag(
    question: str,
    llm: OpenAI,
    model: str,
    project_path: Path,
    max_iters: int = 10,
) -> tuple[str, list, dict]:
    """Agent loop with basic filesystem tools only (no cofe-graph RAG)."""
    with DockerSandbox(project_path) as sandbox:
        messages: list[dict] = [
            {
                "role": "system",
                "content": SYSTEM_WITHOUT
                + f"\n\nThe codebase root directory is: {DockerSandbox.CONTAINER_ROOT}",
            },
            {"role": "user", "content": question},
        ]
        tool_calls_log: list[dict] = []
        total_in = 0
        total_out = 0

        for _ in range(max_iters):
            resp = llm.chat.completions.create(
                model=model,
                messages=messages,
                tools=BASIC_TOOLS,
                tool_choice="auto",
            )
            msg = resp.choices[0].message
            if resp.usage:
                total_in += resp.usage.prompt_tokens
                total_out += resp.usage.completion_tokens

            assistant_entry: dict[str, Any] = {"role": "assistant", "content": msg.content}
            if msg.tool_calls:
                assistant_entry["tool_calls"] = [
                    {
                        "id": tc.id,
                        "type": "function",
                        "function": {
                            "name": tc.function.name,
                            "arguments": tc.function.arguments,
                        },
                    }
                    for tc in msg.tool_calls
                ]
            messages.append(assistant_entry)

            if not msg.tool_calls:
                usage = {"input_tokens": total_in, "output_tokens": total_out}
                return msg.content or "", tool_calls_log, usage

            for tc in msg.tool_calls:
                fn_name = tc.function.name
                try:
                    fn_args = json.loads(tc.function.arguments)
                except json.JSONDecodeError:
                    fn_args = {}

                print(f"    [fs ] {fn_name}({json.dumps(fn_args)})")
                result_text = _execute_basic_tool(fn_name, fn_args, sandbox)
                tool_calls_log.append(
                    {
                        "tool": fn_name,
                        "args": fn_args,
                        "result_preview": result_text[:300],
                    }
                )
                messages.append(
                    {"role": "tool", "tool_call_id": tc.id, "content": result_text}
                )

        usage = {"input_tokens": total_in, "output_tokens": total_out}
        for entry in reversed(messages):
            if entry["role"] == "assistant" and entry.get("content"):
                return entry["content"], tool_calls_log, usage
        return "", tool_calls_log, usage


async def run_with_mcp(
    question: str,
    llm: OpenAI,
    model: str,
    cofe_bin: Path,
    project_path: Path,
    max_iters: int = 10,
) -> tuple[str, list, dict]:
    # Copy firmware to a fresh tmpdir so cofe-graph's .cofe-graph/ index
    # is written there, not into the original source tree.
    tmpdir = Path(tempfile.mkdtemp(prefix="cofe_bench_with_"))
    try:
        src_copy = tmpdir / "src"
        shutil.copytree(
            str(project_path),
            str(src_copy),
            ignore=shutil.ignore_patterns(".cofe-graph"),
        )
        server_params = StdioServerParameters(
            command=str(cofe_bin),
            args=[str(src_copy), "--quiet", "--toon"],
        )

        with DockerSandbox(project_path, cofe_tmpdir=src_copy) as sandbox:
            async with stdio_client(server_params) as (read, write):
                async with ClientSession(read, write) as session:
                    await session.initialize()

                    tools_resp = await session.list_tools()
                    mcp_tools = [_mcp_to_openai_tool(t) for t in tools_resp.tools]
                    combined_tools = BASIC_TOOLS + mcp_tools

                    messages: list[dict] = [
                        {
                            "role": "system",
                            "content": SYSTEM_WITH
                            + f"\n\nThe codebase root directory is: {DockerSandbox.CONTAINER_ROOT}",
                        },
                        {"role": "user", "content": question},
                    ]
                    tool_calls_log: list[dict] = []
                    total_in = 0
                    total_out = 0

                    for _ in range(max_iters):
                        resp = llm.chat.completions.create(
                            model=model,
                            messages=messages,
                            tools=combined_tools,
                            tool_choice="auto",
                        )
                        msg = resp.choices[0].message
                        if resp.usage:
                            total_in += resp.usage.prompt_tokens
                            total_out += resp.usage.completion_tokens

                        assistant_entry: dict[str, Any] = {
                            "role": "assistant",
                            "content": msg.content,
                        }
                        if msg.tool_calls:
                            assistant_entry["tool_calls"] = [
                                {
                                    "id": tc.id,
                                    "type": "function",
                                    "function": {
                                        "name": tc.function.name,
                                        "arguments": tc.function.arguments,
                                    },
                                }
                                for tc in msg.tool_calls
                            ]
                        messages.append(assistant_entry)

                        if not msg.tool_calls:
                            usage = {"input_tokens": total_in, "output_tokens": total_out}
                            return msg.content or "", tool_calls_log, usage

                        for tc in msg.tool_calls:
                            fn_name = tc.function.name
                            try:
                                fn_args = json.loads(tc.function.arguments)
                            except json.JSONDecodeError:
                                fn_args = {}

                            if fn_name in BASIC_TOOL_NAMES:
                                print(f"    [fs ] {fn_name}({json.dumps(fn_args)})")
                                result_text = _execute_basic_tool(fn_name, fn_args, sandbox)
                            else:
                                print(f"    [mcp] {fn_name}({json.dumps(fn_args)})")
                                try:
                                    mcp_result = await session.call_tool(
                                        fn_name, arguments=fn_args
                                    )
                                    result_text = "\n".join(
                                        c.text
                                        for c in mcp_result.content
                                        if hasattr(c, "text")
                                    )
                                except Exception as exc:  # noqa: BLE001
                                    result_text = f"[tool error] {exc}"

                            tool_calls_log.append(
                                {
                                    "tool": fn_name,
                                    "args": fn_args,
                                    "result_preview": result_text[:300],
                                }
                            )
                            messages.append(
                                {
                                    "role": "tool",
                                    "tool_call_id": tc.id,
                                    "content": result_text,
                                }
                            )

                    # Reached max iterations — return last assistant content
                    usage = {"input_tokens": total_in, "output_tokens": total_out}
                    for entry in reversed(messages):
                        if entry["role"] == "assistant" and entry.get("content"):
                            return entry["content"], tool_calls_log, usage
                    return "", tool_calls_log, usage
    finally:
        shutil.rmtree(tmpdir, ignore_errors=True)


# ── CLI ───────────────────────────────────────────────────────────────────────


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


async def main() -> None:
    args = _parse_args()

    _check_docker()
    _ensure_sandbox_image()

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

    llm = build_llm_client(args.base_url, api_key)

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

            if condition == "without":
                response, tc_log, usage = await run_without_rag(
                    question, llm, args.model, project_path
                )
            else:
                assert cofe_bin is not None
                response, tc_log, usage = await run_with_mcp(
                    question, llm, args.model, cofe_bin, project_path
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
    asyncio.run(main())
