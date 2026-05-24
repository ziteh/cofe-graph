#!/usr/bin/env python3
"""Q1 benchmark: with vs without cofe-graph. Dirty MVP."""

import argparse, json, subprocess, sys, time, re
from typing import Any
from openai import OpenAI
from pathlib import Path

_HERE     = Path(__file__).parent.resolve()
_IS_WIN   = sys.platform == "win32"
_EXE      = "cofe-graph.exe" if _IS_WIN else "cofe-graph"

# Paths: auto-detect relative to bench.py location, or override via --binary / --index-dir
_DEFAULT_BINARY    = _HERE.parent / "target" / ("x86_64-pc-windows-gnu/release" if _IS_WIN else "release") / _EXE
_DEFAULT_INDEX_DIR = _HERE.parent / "tests/stm32mp1-baremetal-master/examples/usb_msc_device"

parser = argparse.ArgumentParser()
parser.add_argument("--model",     default="gemma4:e4b")
parser.add_argument("--base-url",  default="http://localhost:11434/v1")
parser.add_argument("--api-key",   default="ollama")
parser.add_argument("--binary",    default=str(_DEFAULT_BINARY))
parser.add_argument("--index-dir", default=str(_DEFAULT_INDEX_DIR))
ARGS = parser.parse_args()

MODEL     = ARGS.model
BINARY    = ARGS.binary
INDEX_DIR = ARGS.index_dir
INDEX_PATH = Path(INDEX_DIR)
RUN_ID    = time.strftime("%Y%m%d_%H%M%S")
RESULTS   = _HERE / "results" / f"{RUN_ID}_{MODEL.replace('/', '_').replace(':', '-')}"

_SOURCES_DESC = (
    "The codebase has 4 source files: "
    "main.cc (65 lines, USB init + LED loop), "
    "usbd_conf.c (455 lines, HAL PCD callbacks bridging hardware interrupts to the USB stack), "
    "usbd_desc.c (255 lines, USB descriptors), "
    "usbd_msc_storage.c (164 lines, virtual RAM-disk storage backend)."
)

Q1 = (
    "The global `USBD_MSC_fops` connects the storage backend to the USB MSC class. "
    "Without assuming which file to look in: "
    "(1) which source file defines `USBD_MSC_fops`, "
    "(2) which source file passes it to the USB stack for registration, "
    "(3) list the storage operation function names it exposes."
)

# Reference answer for Q1 correctness scoring (names that must appear in the answer)
Q1_REFERENCE = [
    "usbd_msc_storage",
    "main",
    "STORAGE_Init",
    "STORAGE_Read",
    "STORAGE_Write",
]

SYSTEM_WITHOUT = (
    f"You are analyzing STM32MP1 USB Mass Storage Device bare-metal firmware. "
    f"{_SOURCES_DESC} "
    "You have NO prior knowledge of this code. "
    "Use read_file and grep_file to find the answer. "
    "You will need to search across multiple source files — do not assume which file. "
    "Give your final answer only after reading the code."
)

SYSTEM_WITH = (
    f"You are analyzing STM32MP1 USB Mass Storage Device bare-metal firmware. "
    f"{_SOURCES_DESC} "
    "You have access to a pre-built call graph and global variable index. "
    "Key tools: get_global_users('name') finds every function in any file that references "
    "a global variable; get_globals('filename') lists globals defined in a file. "
    "Do NOT guess — use the tools to get exact data. "
    "Give your final answer only after querying the actual call graph."
)

# ── MCP client ────────────────────────────────────────────────────────────────
class MCPClient:
    def __init__(self):
        self._id = 0
        self._proc = subprocess.Popen(
            [BINARY, INDEX_DIR, "--quiet", "--toon"],
            stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL,
        )
        self._handshake()
        print("  [mcp] warming up (waiting for index)...", flush=True)
        self.call("find_functions_in_file", {"filename": "main"})
        print("  [mcp] ready", flush=True)

    def _send(self, obj):
        self._proc.stdin.write(json.dumps(obj).encode() + b"\n")
        self._proc.stdin.flush()

    def _recv(self):
        return json.loads(self._proc.stdout.readline())

    def _handshake(self):
        self._id += 1
        self._send({"jsonrpc": "2.0", "id": self._id, "method": "initialize",
            "params": {"protocolVersion": "2024-11-05", "capabilities": {},
                       "clientInfo": {"name": "bench", "version": "0.1.0"}}})
        self._recv()
        self._send({"jsonrpc": "2.0", "method": "notifications/initialized", "params": {}})

    def call(self, name, args):
        self._id += 1
        expected = self._id
        self._send({"jsonrpc": "2.0", "id": expected, "method": "tools/call",
                    "params": {"name": name, "arguments": args}})
        while True:
            resp = self._recv()
            if resp.get("id") == expected:
                break  # skip unsolicited notifications
        content = resp.get("result", {}).get("content", [])
        return content[0]["text"] if content else json.dumps(resp)

    def close(self):
        self._proc.stdin.close()
        self._proc.wait(timeout=5)

# ── WITHOUT tools (raw file) ───────────────────────────────────────────────
_VALID_EXTS = {".c", ".cc", ".cpp", ".h", ".hpp"}

def _resolve_path(path: str) -> Path:
    """Resolve path to an allowed source file within INDEX_DIR."""
    p = Path(path)
    if not p.is_absolute():
        p = INDEX_PATH / p
    p = p.resolve()
    if not str(p).startswith(str(INDEX_PATH.resolve())):
        raise ValueError(f"path outside index dir: {path}")
    if p.suffix not in _VALID_EXTS:
        raise ValueError(f"not a source file: {path}")
    return p

def _read_file(path: str, offset: int = 1, limit: int = 100):
    try:
        lines = _resolve_path(path).read_text().splitlines(keepends=True)
    except (ValueError, FileNotFoundError) as e:
        return f"error: {e}"
    s = max(0, int(offset) - 1)
    e = s + min(int(limit), 300)
    return "".join(f"{s+i+1}: {ln}" for i, ln in enumerate(lines[s:e]))

def _grep_file(path: str, pattern: str):
    try:
        rx = re.compile(pattern, re.IGNORECASE)
    except re.error:
        return f"bad regex: {pattern}"
    try:
        lines = _resolve_path(path).read_text().splitlines()
    except (ValueError, FileNotFoundError) as e:
        return f"error: {e}"
    hits = [f"{i+1}: {ln}" for i, ln in enumerate(lines) if rx.search(ln)]
    return "\n".join(hits[:80]) or "no matches"

_FILES_HINT = "filename relative to index dir, e.g. usbd_conf.c or main.cc"

WITHOUT_TOOLS = [
    {"type": "function", "function": {
        "name": "read_file",
        "description": (
            "Read lines from a source file. Available files: main.cc, usbd_conf.c, "
            "usbd_desc.c, usbd_msc_storage.c (and their headers). "
            "Use offset+limit to navigate large files."
        ),
        "parameters": {"type": "object", "properties": {
            "path":   {"type": "string", "description": _FILES_HINT},
            "offset": {"type": "integer", "description": "1-based start line"},
            "limit":  {"type": "integer", "description": "lines to read (max 300)"},
        }, "required": ["path"]}}},
    {"type": "function", "function": {
        "name": "grep_file",
        "description": (
            "Regex search in a source file. Returns matching lines with line numbers. "
            "Available files: main.cc, usbd_conf.c, usbd_desc.c, usbd_msc_storage.c."
        ),
        "parameters": {"type": "object", "properties": {
            "path":    {"type": "string", "description": _FILES_HINT},
            "pattern": {"type": "string"},
        }, "required": ["path", "pattern"]}}},
]

def without_dispatch(name, args):
    if name == "read_file": return _read_file(**args)
    if name == "grep_file":  return _grep_file(**args)
    return f"unknown tool: {name}"

# ── WITH tools (cofe-graph) ────────────────────────────────────────────────
def _t(name, desc, props, req):
    return {"type": "function", "function": {
        "name": name, "description": desc,
        "parameters": {"type": "object", "properties": props, "required": req}}}

_s = {"type": "string"}
_i = {"type": "integer"}

WITH_TOOLS = [
    _t("find_function",          "Find functions by name substring.",                              {"name": _s}, ["name"]),
    _t("find_functions_in_file", "List all functions in files matching filename substring.",       {"filename": _s}, ["filename"]),
    _t("get_callers",            "Who calls this function? BFS up to depth.",                     {"name": _s, "depth": _i}, ["name"]),
    _t("get_callees",            "What does this function call? BFS down to depth.",              {"name": _s, "depth": _i}, ["name"]),
    _t("get_path",               "Shortest call path between two functions.",                     {"from": _s, "to": _s}, ["from", "to"]),
    _t("get_source",             "Raw source code of a function by exact name.",                  {"name": _s}, ["name"]),
    _t("get_fn_globals",         "Global variables referenced inside a function.",                {"name": _s}, ["name"]),
    _t("get_global_users",       "All functions that reference a named global variable.",         {"name": _s}, ["name"]),
    _t("find_dead_code",         "List functions with no in-file callers (potential dead code).", {}, []),
]

# ── agent loop ─────────────────────────────────────────────────────────────
def run_agent(question, tools, dispatch_fn, system_prompt):
    client = OpenAI(base_url=ARGS.base_url, api_key=ARGS.api_key)
    messages = [
        {"role": "system", "content": system_prompt},
        {"role": "user",   "content": question},
    ]
    n_calls = in_tok = out_tok = 0
    t0 = time.time()
    turns_log = []   # full structured log for analysis

    for turn in range(20):
        t_turn = time.time()
        for attempt in range(5):
            try:
                resp = client.chat.completions.create(
                    model=MODEL, max_tokens=4096,
                    tools=tools, tool_choice="auto",
                    messages=messages,
                )
                break
            except Exception as e:
                wait = 30
                try:
                    wait = e.response.json()["error"]["metadata"].get("retry_after_seconds", 30)
                except Exception:
                    pass
                print(f"  [retry {attempt+1}] sleeping {wait}s: {e}", flush=True)
                time.sleep(float(wait) + 1)
        else:
            raise RuntimeError("max retries exceeded")

        turn_in  = resp.usage.prompt_tokens     if resp.usage else 0
        turn_out = resp.usage.completion_tokens if resp.usage else 0
        in_tok  += turn_in
        out_tok += turn_out

        choice = resp.choices[0]
        msg    = choice.message
        print(f"  turn {turn+1}: finish={choice.finish_reason!r}  "
              f"tool_calls={len(msg.tool_calls or [])}", flush=True)

        turn_entry = {
            "turn":          turn + 1,
            "finish_reason": choice.finish_reason,
            "input_tokens":  turn_in,
            "output_tokens": turn_out,
            "elapsed_sec":   round(time.time() - t_turn, 2),
            "assistant_text": msg.content or "",
            "tool_calls":    [],
        }

        if choice.finish_reason == "tool_calls" and msg.tool_calls:
            messages.append(msg)
            for tc in msg.tool_calls:
                n_calls += 1
                args = json.loads(tc.function.arguments)
                print(f"    -> {tc.function.name}({json.dumps(args)[:100]})", flush=True)
                t_tool = time.time()
                result = dispatch_fn(tc.function.name, args)
                result_str = result if isinstance(result, str) else json.dumps(result)
                tool_elapsed = round(time.time() - t_tool, 3)
                print(f"       {result_str[:120]!r}  ({tool_elapsed}s)", flush=True)
                messages.append({"role": "tool", "tool_call_id": tc.id, "content": result_str})
                turn_entry["tool_calls"].append({
                    "name":        tc.function.name,
                    "args":        args,
                    "result":      result_str,
                    "elapsed_sec": tool_elapsed,
                })
            turns_log.append(turn_entry)
        else:
            answer = msg.content or ""
            turns_log.append(turn_entry)
            break
    else:
        answer = "MAX_TURNS exceeded"

    return {
        "model":         MODEL,
        "answer":        answer,
        "tool_calls":    n_calls,
        "input_tokens":  in_tok,
        "output_tokens": out_tok,
        "elapsed_sec":   round(time.time() - t0, 2),
        "turns":         turns_log,
    }

# ── main ───────────────────────────────────────────────────────────────────
def main():
    RESULTS.mkdir(parents=True, exist_ok=True)
    (RESULTS / "with").mkdir(exist_ok=True)
    (RESULTS / "without").mkdir(exist_ok=True)

    print("\n=== Q1  WITHOUT cofe-graph ===", flush=True)
    r_wo = run_agent(Q1, WITHOUT_TOOLS, without_dispatch, SYSTEM_WITHOUT)
    r_wo["condition"] = "without"
    (RESULTS / "without" / "Q1_without.json").write_text(json.dumps(r_wo, indent=2))
    print(f"  => {r_wo['tool_calls']} calls  {r_wo['elapsed_sec']}s")

    print("\n=== Q1  WITH cofe-graph ===", flush=True)
    mcp = MCPClient()
    r_w = run_agent(Q1, WITH_TOOLS, mcp.call, SYSTEM_WITH)
    mcp.close()
    r_w["condition"] = "with"
    (RESULTS / "with" / "Q1_with.json").write_text(json.dumps(r_w, indent=2))
    print(f"  => {r_w['tool_calls']} calls  {r_w['elapsed_sec']}s")

    # ── correctness scoring ───────────────────────────────────────────────
    def score(result: Any) -> str:
        answer = str(result.get("answer", "")).lower().replace("\\_", "_")
        hits = sum(1 for fn in Q1_REFERENCE if fn.lower() in answer)
        return f"{hits}/{len(Q1_REFERENCE)}"

    wo_score = score(r_wo)
    w_score  = score(r_w)
    r_wo["correctness"] = wo_score
    r_w["correctness"]  = w_score
    # re-save with correctness included
    (RESULTS / "without" / "Q1_without.json").write_text(json.dumps(r_wo, indent=2))
    (RESULTS / "with"    / "Q1_with.json").write_text(json.dumps(r_w, indent=2))

    # ── summary ──────────────────────────────────────────────────────────
    W  = r_wo
    WW = r_w
    print()
    print("=" * 64)
    print(f"  {'Metric':<20} {'WITHOUT':>18} {'WITH':>18}")
    print(f"  {'-'*56}")
    print(f"  {'Tool calls':<20} {W['tool_calls']:>18} {WW['tool_calls']:>18}")
    print(f"  {'Tokens (in+out)':<20} {W['input_tokens']+W['output_tokens']:>18} {WW['input_tokens']+WW['output_tokens']:>18}")
    print(f"  {'Elapsed (s)':<20} {W['elapsed_sec']:>18} {WW['elapsed_sec']:>18}")
    print(f"  {'Correctness (Q1)':<20} {wo_score:>18} {w_score:>18}")
    print("=" * 64)
    print()
    print("── WITHOUT answer " + "─" * 44)
    print(W["answer"])
    print()
    print("── WITH answer " + "─" * 47)
    print(WW["answer"])

if __name__ == "__main__":
    main()
