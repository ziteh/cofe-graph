#!/usr/bin/env python3
"""Q1 benchmark: with vs without cofe-graph. Dirty MVP."""

import argparse, json, subprocess, time, re
from openai import OpenAI
from pathlib import Path

BINARY    = "/Users/klein/ws/cofe-graph/target/release/cofe-graph"
INDEX_DIR = "/Users/klein/ws/cofe-graph/tests/nRF5_SDK_17.1.0_ddde560/examples/ble_peripheral/ble_app_hids_keyboard"
MAIN_C    = INDEX_DIR + "/main.c"
RESULTS   = Path(__file__).parent / "results"

parser = argparse.ArgumentParser()
parser.add_argument("--model", default="qwen2.5:7b")
parser.add_argument("--base-url", default="http://localhost:11434/v1")
parser.add_argument("--api-key", default="ollama")
ARGS = parser.parse_args()
MODEL = ARGS.model

Q1 = (
    "Trace the complete call chain from when BSP_EVENT_KEY_0 is triggered "
    "(button press) to when a BLE HID keyboard report is actually transmitted. "
    "List every function in the chain in order, with a one-line description of each."
)

SYSTEM_WITHOUT = (
    f"You are analyzing BLE HID keyboard firmware. "
    f"The main source file is {MAIN_C} (1606 lines, ~50 static functions). "
    "You have NO prior knowledge of this code. "
    "Use read_file and grep_file to find the answer. "
    "You MUST read the actual source code of each function in the chain. "
    "Do NOT guess function names — grep to verify each one exists. "
    "Give your final answer only after reading the code."
)

SYSTEM_WITH = (
    f"You are analyzing BLE HID keyboard firmware. "
    f"The main source file is {MAIN_C} (1606 lines, ~50 static functions). "
    "You have access to a pre-built call graph. "
    "To trace a call chain: use get_callees(name, depth=3) on the entry-point function. "
    "To find the entry point first: use find_function or grep. "
    "Use get_source to verify the source of any function. "
    "Do NOT guess — use the tools to get exact data. "
    "Give your final answer only after traversing the actual call graph."
)

# ── MCP client ────────────────────────────────────────────────────────────────
class MCPClient:
    def __init__(self):
        self._id = 0
        self._proc = subprocess.Popen(
            [BINARY, INDEX_DIR, "--quiet"],
            stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL,
        )
        self._handshake()
        print("  [mcp] warming up (waiting for index)...", flush=True)
        self.call("find_functions_in_file", {"filename": "main.c"})
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
def _read_file(path, offset=1, limit=100):
    lines = open(MAIN_C).readlines()
    s = max(0, int(offset) - 1)
    e = s + min(int(limit), 300)
    return "".join(f"{s+i+1}: {l}" for i, l in enumerate(lines[s:e]))

def _grep_file(path, pattern):
    try:
        rx = re.compile(pattern, re.IGNORECASE)
    except re.error:
        return f"bad regex: {pattern}"
    lines = open(MAIN_C).readlines()
    hits = [f"{i+1}: {l.rstrip()}" for i, l in enumerate(lines) if rx.search(l)]
    return "\n".join(hits[:80]) or "no matches"

WITHOUT_TOOLS = [
    {"type": "function", "function": {
        "name": "read_file",
        "description": "Read lines from main.c. Use offset+limit to navigate the 1606-line file.",
        "parameters": {"type": "object", "properties": {
            "path":   {"type": "string", "description": f"use {MAIN_C}"},
            "offset": {"type": "integer", "description": "1-based start line"},
            "limit":  {"type": "integer", "description": "lines to read (max 300)"},
        }, "required": ["path"]}}},
    {"type": "function", "function": {
        "name": "grep_file",
        "description": "Regex search in main.c. Returns matching lines with line numbers.",
        "parameters": {"type": "object", "properties": {
            "path":    {"type": "string", "description": f"use {MAIN_C}"},
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

    for turn in range(20):
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
                    import json as _j
                    wait = e.response.json()["error"]["metadata"].get("retry_after_seconds", 30)
                except Exception:
                    pass
                print(f"  [retry {attempt+1}] sleeping {wait}s: {e}", flush=True)
                time.sleep(float(wait) + 1)
        else:
            raise RuntimeError("max retries exceeded")
        if resp.usage:
            in_tok  += resp.usage.prompt_tokens or 0
            out_tok += resp.usage.completion_tokens or 0

        choice = resp.choices[0]
        msg    = choice.message
        print(f"  turn {turn+1}: finish={choice.finish_reason!r}  "
              f"tool_calls={len(msg.tool_calls or [])}", flush=True)

        if choice.finish_reason == "tool_calls" and msg.tool_calls:
            messages.append(msg)
            for tc in msg.tool_calls:
                n_calls += 1
                args = json.loads(tc.function.arguments)
                print(f"    -> {tc.function.name}({json.dumps(args)[:100]})", flush=True)
                result = dispatch_fn(tc.function.name, args)
                result_str = result if isinstance(result, str) else json.dumps(result)
                print(f"       {result_str[:120]!r}", flush=True)
                messages.append({"role": "tool", "tool_call_id": tc.id, "content": result_str})
        else:
            answer = msg.content or ""
            break
    else:
        answer = "MAX_TURNS exceeded"

    return {
        "answer":        answer,
        "tool_calls":    n_calls,
        "input_tokens":  in_tok,
        "output_tokens": out_tok,
        "elapsed_sec":   round(time.time() - t0, 2),
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
    print("=" * 64)
    print()
    print("── WITHOUT answer " + "─" * 44)
    print(W["answer"])
    print()
    print("── WITH answer " + "─" * 47)
    print(WW["answer"])

if __name__ == "__main__":
    main()
