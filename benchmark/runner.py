#!/usr/bin/env python3
"""
runner.py — pi agent benchmark runner

Runs pi coding agent for each question in --cases, collects answers, writes results.
Designed to run inside the exp or ctrl Docker container.

Usage (inside Docker):
  python runner.py --condition exp --cases /bench/cases/questions.json --out /results
  python runner.py --condition ctrl --cases /bench/cases/questions.json --out /results

Environment variables:
  PI_PROVIDER      — provider name registered in models.json (default: ollama)
  PI_MODEL         — model id (default: qwen2.5-coder:7b)
  PI_BASE_URL      — OpenAI-compatible base URL (default: http://localhost:11434/v1)
  PI_API_KEY       — API key sent with requests (default: ollama)
  ANTHROPIC_API_KEY / OPENAI_API_KEY — picked up by pi automatically for those providers
"""

import argparse
import json
import os
import subprocess
import sys
import time
from datetime import datetime, timezone
from pathlib import Path

SCRIPT_DIR = Path(__file__).resolve().parent

EXP_CONTEXT = (
    "You are analyzing a C firmware codebase using graph-RAG tools.\n"
    "Use the available MCP tools to look up functions, source code, and call graphs.\n"
    "Do not rely on prior knowledge about this specific project.\n\n"
)

CTRL_CONTEXT = (
    "You are analyzing a C firmware codebase. The codebase is in your current working directory.\n"
    "ALWAYS use tools to search and read actual source files before answering.\n"
    "Do not rely on prior knowledge about this specific project.\n\n"
)


def configure_pi(condition: str) -> None:
    """Write ~/.pi/agent/{settings,models}.json from env vars."""
    settings_dir = Path.home() / ".pi" / "agent"
    settings_dir.mkdir(parents=True, exist_ok=True)

    provider = os.environ.get("PI_PROVIDER", "ollama")
    model = os.environ.get("PI_MODEL", "qwen2.5-coder:7b")
    base_url = os.environ.get("PI_BASE_URL", "http://localhost:11434/v1")
    api_key = os.environ.get("PI_API_KEY", "ollama")

    settings = {"defaultProvider": provider, "defaultModel": model}
    (settings_dir / "settings.json").write_text(json.dumps(settings, indent=2))

    models_config = {
        "providers": {
            provider: {
                "baseUrl": base_url,
                "api": "openai-completions",
                "apiKey": api_key,
                "compat": {"supportsDeveloperRole": False},
                "models": [{"id": model}],
            }
        }
    }
    (settings_dir / "models.json").write_text(json.dumps(models_config, indent=2))

    if condition == "exp":
        cofe_graph_bin = os.environ.get("COFE_GRAPH_BIN", "cofe-graph")
        fw_path = os.environ.get("FW_PATH", "/fw")
        mcp_config = {
            "mcpServers": {
                "cofe-graph": {
                    "command": cofe_graph_bin,
                    "args": [fw_path, "mcp", "--toon", "--quiet"],
                    "lifecycle": "lazy",
                }
            }
        }
        mcp_path = settings_dir / "mcp.json"
        mcp_path.write_text(json.dumps(mcp_config, indent=2))
        print(f"[runner] MCP config: {mcp_path}", flush=True)
    else:
        mcp_path = Path.home() / ".pi" / "agent" / "mcp.json"
        if mcp_path.exists():
            mcp_path.unlink()
        print("[runner] No MCP config (ctrl condition)", flush=True)


def run_pi(prompt: str, cwd: str, timeout: int = 900) -> tuple[str, float]:
    t0 = time.perf_counter()
    proc = subprocess.Popen(
        ["pi", "-p", prompt],
        cwd=cwd,
        stdin=subprocess.DEVNULL,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
    )
    try:
        stdout, stderr = proc.communicate(timeout=timeout)
    except subprocess.TimeoutExpired:
        proc.kill()
        proc.communicate()
        raise

    duration = time.perf_counter() - t0

    if proc.returncode != 0:
        print(f"[warn] pi exited {proc.returncode}:\n{stderr[:500]}", file=sys.stderr, flush=True)
    elif stderr.strip():
        print(f"[pi stderr] {stderr[:300]}", flush=True)

    return stdout.strip(), round(duration, 2)


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--condition", required=True, choices=["exp", "ctrl"])
    parser.add_argument("--cases", default=str(SCRIPT_DIR / "cases" / "questions.json"))
    parser.add_argument("--out", default=str(SCRIPT_DIR / "results"))
    parser.add_argument("--cwd", default="/fw")
    args = parser.parse_args()

    cases = json.loads(Path(args.cases).read_text())
    out_dir = Path(args.out)
    out_dir.mkdir(parents=True, exist_ok=True)

    configure_pi(args.condition)

    ts = datetime.now(timezone.utc).strftime("%Y%m%d_%H%M%S")
    run_dir = out_dir / ts
    run_dir.mkdir(parents=True, exist_ok=True)

    context = EXP_CONTEXT if args.condition == "exp" else CTRL_CONTEXT

    records = []
    for case in cases:
        qid = case["id"]
        question = case["question"]
        prompt = context + question

        print(f"\n── [{qid}] condition={args.condition}", flush=True)
        answer, duration = run_pi(prompt, args.cwd)
        print(f"  done in {duration}s", flush=True)

        records.append({
            "task_id": qid,
            "condition": args.condition,
            "question": question,
            "response": answer,
            "duration_sec": duration,
            "timestamp": datetime.now(timezone.utc).isoformat(),
        })

    out_file = run_dir / f"{args.condition}.json"
    out_file.write_text(json.dumps(records, indent=2) + "\n")
    print(f"\n[runner] written: {out_file}", flush=True)
    print(f"[runner] run dir: {run_dir}", flush=True)


if __name__ == "__main__":
    main()
