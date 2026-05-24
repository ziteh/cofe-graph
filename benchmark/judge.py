#!/usr/bin/env python3
"""judge.py — blind LLM judge for cofe-graph benchmark

Collects responses from one or more result directories, assigns random labels
(A, B, C …) so the judge cannot see which condition produced each answer, then
generates a formatted prompt file for manual submission to an LLM judge.

After the judge fills in judgment_template.json, --summary deanonymises scores
and prints a per-condition tally.

Usage
-----
    # Full pipeline (auto-creates .venv on first run):
    python judge.py --run \
      --model gemma4:e4b --base-url http://192.168.50.44:11434/v1 \
      --judge-model gemma4:e4b --judge-base-url http://192.168.50.44:11434/v1 \
      --question Q1 Q2 --runs 2

    # Generate judge prompts from existing result directories:
    python judge.py results/run1 results/run2 [--out results/judge_01] [--seed 42]

    # Deanonymise scores after filling judgment_template.json:
    python judge.py --summary results/judge_01
"""

# ── stdlib-only bootstrap (runs before any third-party import) ─────────────────
import os
import sys

if sys.version_info < (3, 10):
    sys.exit(
        f"[judge] Python 3.10+ required (found {sys.version_info.major}.{sys.version_info.minor}).\n"
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
        import subprocess as _sp

        print("[judge] Creating .venv ...", file=sys.stderr)
        _sp.check_call([sys.executable, "-m", "venv", venv])
        req = os.path.join(here, "requirements.txt")
        if os.path.isfile(req):
            print("[judge] Installing dependencies ...", file=sys.stderr)
            _sp.check_call(
                [os.path.join(venv, _VENV_BIN, _PIP_EXE), "install", "-q", "-r", req]
            )
    import subprocess as _sp

    result = _sp.run([python] + sys.argv)
    sys.exit(result.returncode)


_bootstrap()
# ── end bootstrap — imports below are intentionally after bootstrap ─────────────
import argparse  # noqa: E402
import datetime  # noqa: E402
import json  # noqa: E402
import random  # noqa: E402
import re  # noqa: E402
import string  # noqa: E402
import subprocess  # noqa: E402
from pathlib import Path  # noqa: E402

_HERE = Path(os.path.abspath(__file__)).parent  # noqa: E402
TEST_CASES = json.loads((_HERE / "test_cases.json").read_text(encoding="utf-8"))  # noqa: E402
TASK_BY_ID = {t["id"]: t for t in TEST_CASES}


# ── data loading ───────────────────────────────────────────────────────────────


def load_results(dirs: list[Path]) -> dict:
    """Load all responses from result directories.

    Returns
    -------
    dict mapping task_id → list of {run, condition, response}
    """
    results: dict = {}
    for d in dirs:
        for cond in ("without", "with"):
            f = d / f"{cond}.json"
            if not f.exists():
                continue
            raw = json.loads(f.read_text())
            entries = raw if isinstance(raw, list) else [raw]
            for entry in entries:
                tid = entry.get("task_id")
                if not tid:
                    continue
                results.setdefault(tid, []).append(
                    {
                        "run": d.name,
                        "condition": cond,
                        "response": entry.get("response", ""),
                    }
                )
    return results


def load_stats(dirs: list[Path]) -> dict:
    """Load per-entry bench stats from result directories.

    Returns
    -------
    dict mapping task_id → list of {run, condition, input_tokens, output_tokens,
                                     num_tool_calls, duration_sec}
    """
    stats: dict = {}
    for d in dirs:
        for cond in ("without", "with"):
            f = d / f"{cond}.json"
            if not f.exists():
                continue
            raw = json.loads(f.read_text())
            entries = raw if isinstance(raw, list) else [raw]
            for entry in entries:
                tid = entry.get("task_id")
                if not tid:
                    continue
                stats.setdefault(tid, []).append(
                    {
                        "run": d.name,
                        "condition": cond,
                        "input_tokens": entry.get("input_tokens", 0),
                        "output_tokens": entry.get("output_tokens", 0),
                        "num_tool_calls": entry.get("num_tool_calls", 0),
                        "duration_sec": entry.get("duration_sec", 0.0),
                    }
                )
    return stats


# ── label assignment ───────────────────────────────────────────────────────────


def assign_labels(entries: list, rng: random.Random) -> list[tuple[str, dict]]:
    """Shuffle entries and assign uppercase letters A, B, C …"""
    shuffled = list(entries)
    rng.shuffle(shuffled)
    labels = list(string.ascii_uppercase[: len(shuffled)])
    return list(zip(labels, shuffled))


# ── formatting ─────────────────────────────────────────────────────────────────


def format_task_block(task: dict, labeled: list[tuple[str, dict]]) -> str:
    rubric: list = task["rubric"]
    max_pts: int = sum(r["points"] for r in rubric)
    dw: float = task["difficulty_weight"]
    lines: list[str] = [
        f"=== TASK {task['id']}: {task['function']}  [difficulty_weight×{dw}] ===",
        "",
        "QUESTION:",
        f"  {task['question']}",
        "",
        "RUBRIC  (score each criterion independently:",
        "          0 = wrong or missing,  full points = fully correct):",
    ]
    for i, r in enumerate(rubric, 1):
        p = r["points"]
        lines.append(f"  [{i}] {r['criterion']}  ({p} pt{'s' if p > 1 else ''})")
    lines += [f"  MAX TOTAL: {max_pts} pts", ""]

    for label, entry in labeled:
        lines += [f"--- RESPONSE {label} ---", entry["response"].strip(), ""]

    lines += [
        "---",
        "For each response label and criterion [N], write one line:",
        '  "<Label>[<N>]: <score>/<max> — <brief note>"',
        "",
        "Then write totals:",
    ]
    for label, _ in labeled:
        lines.append(f"  TOTAL {label}: ?/{max_pts}")
    label_str = " > ".join(l for l, _ in labeled)
    lines += [
        "",
        f"  RANKING (best → worst):  {label_str}  ← reorder as appropriate",
        "",
        "=" * 72,
        "",
    ]
    return "\n".join(lines)


def build_template_entry(task: dict, labeled: list[tuple[str, dict]]) -> dict:
    return {
        label: {
            "criteria_scores": [None] * len(task["rubric"]),
            "total": None,
            "notes": "",
        }
        for label, _ in labeled
    }


# ── run command ───────────────────────────────────────────────────────────────


def cmd_run(
    model: str,
    base_url: str,
    question_ids: list[str],
    n_runs: int,
    condition: str,
    api_key: str | None,
    cofe_graph_bin: str | None,
    project_path: str | None,
    out_dir: Path | None,
    seed: int | None,
) -> None:
    here = Path(__file__).parent
    bench = here / "bench.py"
    python = sys.executable

    result_dirs: list[Path] = []

    for i in range(n_runs):
        print(f"[judge] ── run {i + 1}/{n_runs} ", flush=True)
        cmd = [
            python,
            str(bench),
            "--model",
            model,
            "--base-url",
            base_url,
            "--condition",
            condition,
            "--question",
            *question_ids,
        ]
        if api_key:
            cmd += ["--api-key", api_key]
        if cofe_graph_bin:
            cmd += ["--cofe-graph-bin", cofe_graph_bin]
        if project_path:
            cmd += ["--project-path", project_path]

        run_dir: Path | None = None
        with subprocess.Popen(
            cmd,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            text=True,
            cwd=str(here),
        ) as proc:
            assert proc.stdout is not None
            for line in proc.stdout:
                print(line, end="", flush=True)
                m = re.search(r"\[bench\] run dir\s*:\s*(\S+)", line)
                if m:
                    run_dir = here / m.group(1)
            proc.wait()
            if proc.returncode != 0:
                print(
                    f"[judge] bench.py exited with code {proc.returncode} — skipping run",
                    file=sys.stderr,
                )
                continue

        if run_dir and run_dir.is_dir():
            result_dirs.append(run_dir)
        else:
            print(
                "[judge] Could not determine run dir from bench.py output",
                file=sys.stderr,
            )

    if not result_dirs:
        sys.exit("[judge] No successful bench runs — aborting.")

    print(
        f"\n[judge] Collected {len(result_dirs)} run(s): {[d.name for d in result_dirs]}"
    )
    rng_seed = seed if seed is not None else random.randint(0, 2**32 - 1)
    final_out = out_dir or (
        here / "results" / f"judge_{datetime.datetime.now().strftime('%Y%m%d_%H%M%S')}"
    )
    cmd_generate(result_dirs, final_out, rng_seed)
    return final_out


# ── auto-judge command ─────────────────────────────────────────────────────────

_JUDGE_SYSTEM = (
    "You are an expert embedded-systems C code reviewer. "
    "Score AI assistant responses about Flipper Zero firmware against a precise rubric. "
    "Be strict: award full points only when the answer is completely and exactly correct; "
    "award 0 for anything wrong, imprecise, or missing."
)


def _build_judge_user_prompt(task: dict, labels: list[str], responses: dict) -> str:
    rubric = task["rubric"]
    max_pts = sum(r["points"] for r in rubric)
    lines = [
        f"TASK: {task['function']}  (id: {task['id']})",
        "",
        "QUESTION:",
        task["question"],
        "",
        "RUBRIC:",
    ]
    for i, r in enumerate(rubric, 1):
        p = r["points"]
        lines.append(f"  [{i}] {r['criterion']}  ({p} pt{'s' if p > 1 else ''})")
    lines += [f"  MAX: {max_pts} pts", ""]
    for label in labels:
        resp = responses.get(label, "")
        lines += [f"--- RESPONSE {label} ---", resp.strip(), ""]
    schema_fields = ", ".join(f"0_or_{r['points']}" for r in rubric)
    lines += [
        "---",
        "Respond with ONLY a JSON object — no explanation, no markdown fences:",
        "{",
    ]
    for label in labels:
        lines.append(
            f'  "{label}": {{"criteria_scores": [{schema_fields}], "notes": "<one sentence>"}}'
        )
    lines += [
        "}",
        "",
        f"Each criteria_scores entry must be either 0 or the criterion's full point value.",
        f"There must be exactly {len(rubric)} entries in criteria_scores, one per criterion.",
    ]
    return "\n".join(lines)


def _parse_scores(text: str) -> dict | None:
    """Extract the JSON scores object from LLM output."""
    text = re.sub(r"```(?:json)?\s*", "", text).strip()
    try:
        return json.loads(text)
    except json.JSONDecodeError:
        pass
    m = re.search(r"(\{.*\})", text, re.DOTALL)
    if m:
        try:
            return json.loads(m.group(1))
        except json.JSONDecodeError:
            pass
    return None


def cmd_auto_judge(
    judge_dir: Path,
    model: str,
    base_url: str,
    api_key: str | None,
) -> None:
    resp_file = judge_dir / "responses.json"
    tmpl_file = judge_dir / "judgment_template.json"
    bm_file = judge_dir / "blinding_map.json"
    for f in (tmpl_file, bm_file):
        if not f.exists():
            sys.exit(f"[judge] Missing {f}")

    # Backfill responses.json if absent (judge dir pre-dates this feature)
    if not resp_file.exists():
        print("[judge] responses.json not found — reconstructing from blinding_map ...")
        bm_data = json.loads(bm_file.read_text())
        result_dirs = [Path(d) for d in bm_data.get("result_dirs", [])]
        raw = load_results(result_dirs)
        bmap = bm_data["map"]
        responses_backfill: dict = {}
        for tid, label_map in bmap.items():
            entries = raw.get(tid, [])
            responses_backfill[tid] = {}
            for label, info in label_map.items():
                match = next(
                    (
                        e
                        for e in entries
                        if e["run"] == info["run"]
                        and e["condition"] == info["condition"]
                    ),
                    None,
                )
                responses_backfill[tid][label] = match["response"] if match else ""
        resp_file.write_text(json.dumps(responses_backfill, indent=2))
        print(f"[judge] Written: {resp_file}")

    try:
        from openai import OpenAI  # local import — only needed here
    except ImportError:
        sys.exit("[judge] openai package not found — run: pip install openai")

    client = OpenAI(base_url=base_url, api_key=api_key or "ollama")
    all_responses: dict = json.loads(resp_file.read_text())
    tmpl: dict = json.loads(tmpl_file.read_text())

    for task in TEST_CASES:
        tid = task["id"]
        if tid not in tmpl:
            continue
        labels = list(tmpl[tid].keys())
        task_responses = all_responses.get(tid, {})
        rubric = task["rubric"]

        prompt = _build_judge_user_prompt(task, labels, task_responses)
        print(
            f"[judge] scoring {tid} ({len(labels)} responses) ...", end=" ", flush=True
        )

        try:
            resp = client.chat.completions.create(
                model=model,
                messages=[
                    {"role": "system", "content": _JUDGE_SYSTEM},
                    {"role": "user", "content": prompt},
                ],
                temperature=0,
            )
            raw_text = resp.choices[0].message.content or ""
        except Exception as exc:
            print(f"ERROR: {exc}")
            continue

        scores = _parse_scores(raw_text)
        if scores is None:
            print(f"PARSE FAILED — raw output saved to {judge_dir}/{tid}_raw.txt")
            (judge_dir / f"{tid}_raw.txt").write_text(raw_text)
            continue

        ok_count = 0
        for label in labels:
            entry = scores.get(label)
            if not entry or "criteria_scores" not in entry:
                print(f"\n  [warn] missing scores for label {label}")
                continue
            cs = entry["criteria_scores"]
            clamped = [min(max(int(v), 0), r["points"]) for v, r in zip(cs, rubric)]
            tmpl[tid][label]["criteria_scores"] = clamped
            tmpl[tid][label]["total"] = sum(clamped)
            tmpl[tid][label]["notes"] = entry.get("notes", "")
            ok_count += 1
        print(f"OK ({ok_count}/{len(labels)} labels scored)")

    tmpl_file.write_text(json.dumps(tmpl, indent=2))
    print(f"[judge] Updated: {tmpl_file}")
    print()
    cmd_summary(judge_dir)


# ── generate command ───────────────────────────────────────────────────────────


def cmd_generate(result_dirs: list[Path], out_dir: Path, seed: int) -> None:
    raw = load_results(result_dirs)
    if not raw:
        sys.exit("[judge] No results found in the specified directories.")

    rng = random.Random(seed)
    blinding_map: dict = {}
    template: dict = {}
    prompt_blocks: list[str] = []

    # Process tasks in TEST_CASES definition order
    found_ids: list[str] = []
    responses: dict = {}  # task_id → {label → response text} — used by auto-judge
    for task in TEST_CASES:
        tid = task["id"]
        if tid not in raw:
            continue
        found_ids.append(tid)
        labeled = assign_labels(raw[tid], rng)
        blinding_map[tid] = {
            label: {"run": entry["run"], "condition": entry["condition"]}
            for label, entry in labeled
        }
        template[tid] = build_template_entry(task, labeled)
        responses[tid] = {label: entry["response"] for label, entry in labeled}
        prompt_blocks.append(format_task_block(task, labeled))

    out_dir.mkdir(parents=True, exist_ok=True)

    # judge_input.txt — no condition or run names visible
    header = (
        "# cofe-graph Benchmark — Blind Judge Input\n"
        f"# Seed: {seed}    Tasks: {', '.join(found_ids)}\n"
        "#\n"
        "# Instructions:\n"
        "#   Score each response against the rubric for its task.\n"
        "#   Do NOT guess which system produced each answer.\n"
        "#   Fill in judgment_template.json with your per-criterion scores.\n\n"
    )
    footer = (
        f"\n\n# When done, write your scores into:\n"
        f"#   {out_dir / 'judgment_template.json'}\n"
    )
    (out_dir / "judge_input.txt").write_text(header + "\n".join(prompt_blocks) + footer)

    # blinding_map.json — kept separate; not for the judge
    (out_dir / "blinding_map.json").write_text(
        json.dumps(
            {
                "seed": seed,
                "result_dirs": [str(d) for d in result_dirs],
                "map": blinding_map,
            },
            indent=2,
        )
    )

    # judgment_template.json — judge fills this in
    (out_dir / "judgment_template.json").write_text(json.dumps(template, indent=2))

    # responses.json — used by auto-judge (kept in judge dir, not for human judge)
    (out_dir / "responses.json").write_text(json.dumps(responses, indent=2))

    total_resp = sum(len(v) for v in blinding_map.values())
    print(f"[judge] tasks:      {len(found_ids)} ({', '.join(found_ids)})")
    print(
        f"[judge] responses:  {total_resp} total  ({total_resp // len(found_ids) if found_ids else 0} per task)"
    )
    print(f"[judge] seed:       {seed}")
    print(f"[judge] out dir:    {out_dir}")
    print()
    print("Next steps:")
    print(
        f"  1. python judge.py --judge {out_dir} --judge-model <model> --judge-base-url <url>"
    )
    print(f"     — or manually submit {out_dir}/judge_input.txt to an LLM")
    print(f"  2. Fill    {out_dir}/judgment_template.json  with the scores (if manual)")
    print(f"  3. Run:    python judge.py --summary {out_dir}")


# ── summary command ────────────────────────────────────────────────────────────


def cmd_summary(judge_dir: Path) -> None:
    bm_file = judge_dir / "blinding_map.json"
    tmpl_file = judge_dir / "judgment_template.json"
    for f in (bm_file, tmpl_file):
        if not f.exists():
            sys.exit(f"[judge] Missing {f}")

    bm_data = json.loads(bm_file.read_text())
    tmpl = json.loads(tmpl_file.read_text())
    blinding_map = bm_data["map"]

    result_dirs = [Path(d) for d in bm_data.get("result_dirs", [])]
    stats_by_task = load_stats(result_dirs)

    judgment = {}
    cond_agg = {}  # condition → {score, max, count, in_tok, out_tok, tools, secs}

    col_w = {"task": 6, "fn": 36, "run": 24, "cond": 8, "score": 10, "wscore": 12}
    header = (
        f"{'Task':<{col_w['task']}}  {'Function':<{col_w['fn']}}  "
        f"{'Run':<{col_w['run']}}  {'Cond':<{col_w['cond']}}  "
        f"{'Score':>{col_w['score']}}  {'Weighted':>{col_w['wscore']}}"
    )
    rows = []

    for tid, labels_data in tmpl.items():
        task = TASK_BY_ID.get(tid)
        if not task:
            continue
        max_pts = sum(r["points"] for r in task["rubric"])
        dw = task["difficulty_weight"]
        map_task = blinding_map.get(tid, {})
        judgment[tid] = {}

        for label, scores_data in labels_data.items():
            info = map_task.get(label, {})
            condition = info.get("condition", "?")
            run = info.get("run", "?")
            raw_scores = scores_data.get("criteria_scores")
            if raw_scores and all(s is not None for s in raw_scores):
                total = sum(raw_scores)
            elif scores_data.get("total") is not None:
                total = int(scores_data["total"])
            else:
                total = None

            judgment[tid][label] = {
                "condition": condition,
                "run": run,
                "criteria_scores": raw_scores,
                "total": total,
                "difficulty_weight": dw,
                "notes": scores_data.get("notes", ""),
            }

            # Find matching bench stats entry for this (tid, run, condition)
            stat = next(
                (
                    s
                    for s in stats_by_task.get(tid, [])
                    if s["run"] == run and s["condition"] == condition
                ),
                None,
            )

            if total is not None:
                agg = cond_agg.setdefault(
                    condition,
                    {
                        "score": 0.0,
                        "max": 0.0,
                        "count": 0,
                        "in_tok": 0,
                        "out_tok": 0,
                        "tools": 0,
                        "secs": 0.0,
                    },
                )
                agg["score"] += total * dw
                agg["max"] += max_pts * dw
                agg["count"] += 1
                if stat:
                    agg["in_tok"] += stat["input_tokens"]
                    agg["out_tok"] += stat["output_tokens"]
                    agg["tools"] += stat["num_tool_calls"]
                    agg["secs"] += stat["duration_sec"]

            score_str = f"{total}/{max_pts}" if total is not None else f"?/{max_pts}"
            wscore_str = (
                f"{total * dw:.1f}/{max_pts * dw:.1f}" if total is not None else "?"
            )
            rows.append(
                f"{tid:<{col_w['task']}}  {task['function']:<{col_w['fn']}}  "
                f"{run:<{col_w['run']}}  {condition:<{col_w['cond']}}  "
                f"{score_str:>{col_w['score']}}  {wscore_str:>{col_w['wscore']}}"
            )

    judgment["_meta"] = {
        "seed": bm_data.get("seed"),
        "result_dirs": bm_data.get("result_dirs"),
        "condition_totals": cond_agg,
    }
    (judge_dir / "judgment.json").write_text(json.dumps(judgment, indent=2))

    sep = "-" * len(header)
    print(header)
    print(sep)
    for row in rows:
        print(row)
    print(sep)
    print()
    print("CONDITION TOTALS  (weighted score | tokens in/out | tool calls | time)")
    print()
    for cond, data in sorted(cond_agg.items()):
        pct = data["score"] / data["max"] * 100 if data["max"] else 0.0
        n = data["count"]
        print(
            f"  {cond:<10}  score {data['score']:>6.1f}/{data['max']:.1f} ({pct:.1f}%)"
            f"  [{n} resp]"
        )
        print(
            f"             in={data['in_tok']:,}  out={data['out_tok']:,}"
            f"  tools={data['tools']}  time={data['secs']:.1f}s"
        )
    print()
    print(f"[judge] Written: {judge_dir}/judgment.json")


# ── main ───────────────────────────────────────────────────────────────────────


def main() -> None:
    parser = argparse.ArgumentParser(
        description="Blind LLM judge for cofe-graph benchmark",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog=(
            "Examples:\n"
            "  # Full pipeline: run bench N times, auto-judge, summarise:\n"
            "  python judge.py --run --model gemma4:e4b --base-url http://host/v1 \\\n"
            "                  --question Q1 Q2 --runs 3 \\\n"
            "                  --judge-model qwen2.5-coder:32b --judge-base-url http://host/v1\n"
            "  # Auto-judge an existing judge dir:\n"
            "  python judge.py --judge results/judge_01 \\\n"
            "                  --judge-model qwen2.5-coder:32b --judge-base-url http://host/v1\n"
            "  # Generate judge files from existing result directories:\n"
            "  python judge.py results/run1 results/run2 [--out results/judge_01]\n"
            "  # Deanonymise and report after filling judgment_template.json manually:\n"
            "  python judge.py --summary results/judge_01\n"
        ),
    )

    # ── mode flags
    mode = parser.add_mutually_exclusive_group()
    mode.add_argument(
        "--run",
        action="store_true",
        help="Run bench.py N times then generate (and optionally auto-judge) files",
    )
    mode.add_argument(
        "--judge",
        action="store_true",
        help="Auto-judge an existing judge dir using an LLM",
    )
    mode.add_argument(
        "--summary",
        action="store_true",
        help="Deanonymise scores and print per-condition report",
    )

    # ── positional: result dirs (generate mode) or judge dir (summary mode)
    parser.add_argument(
        "paths",
        nargs="*",
        type=Path,
        metavar="PATH",
        help="Result directories (generate mode) or judge output directory (--summary)",
    )

    # ── --run mode args (forwarded to bench.py)
    parser.add_argument("--model", default=None, help="Model name passed to bench.py")
    parser.add_argument(
        "--base-url", default=None, dest="base_url", help="Base URL passed to bench.py"
    )
    parser.add_argument(
        "--api-key", default=None, dest="api_key", help="API key passed to bench.py"
    )
    parser.add_argument(
        "--condition",
        choices=["with", "without", "both"],
        default="both",
        help="Condition(s) to run (default: both)",
    )
    parser.add_argument(
        "--question",
        nargs="+",
        metavar="QN",
        default=None,
        help="Question IDs to benchmark, e.g. --question Q1 Q2",
    )
    parser.add_argument(
        "--runs",
        type=int,
        default=1,
        metavar="N",
        help="Number of bench.py runs (default: 1)",
    )
    parser.add_argument(
        "--cofe-graph-bin", default=None, metavar="PATH", help="cofe-graph binary path"
    )
    parser.add_argument(
        "--project-path", default=None, dest="project_path", metavar="PATH"
    )

    # ── judge LLM args (--judge mode and optional auto-judge after --run)
    parser.add_argument(
        "--judge-model",
        default=None,
        metavar="MODEL",
        help="LLM model for auto-judging responses",
    )
    parser.add_argument(
        "--judge-base-url",
        default=None,
        metavar="URL",
        dest="judge_base_url",
        help="Base URL for judge LLM (defaults to --base-url)",
    )
    parser.add_argument(
        "--judge-api-key",
        default=None,
        metavar="KEY",
        dest="judge_api_key",
        help="API key for judge LLM (defaults to --api-key)",
    )

    # ── shared args
    parser.add_argument(
        "--out",
        type=Path,
        default=None,
        metavar="DIR",
        help="Output directory for judge files (default: results/judge_<timestamp>)",
    )
    parser.add_argument(
        "--seed",
        type=int,
        default=None,
        help="Random seed for label assignment (default: random)",
    )

    args = parser.parse_args()

    if args.run:
        if not args.model or not args.base_url:
            parser.error("--run requires --model and --base-url")
        if not args.question:
            parser.error("--run requires --question QN [QN ...]")
        judge_dir = cmd_run(
            model=args.model,
            base_url=args.base_url,
            question_ids=args.question,
            n_runs=args.runs,
            condition=args.condition,
            api_key=args.api_key,
            cofe_graph_bin=args.cofe_graph_bin,
            project_path=args.project_path,
            out_dir=args.out,
            seed=args.seed,
        )
        if args.judge_model:
            print()
            cmd_auto_judge(
                judge_dir=judge_dir,
                model=args.judge_model,
                base_url=args.judge_base_url or args.base_url,
                api_key=args.judge_api_key or args.api_key,
            )
    elif args.judge:
        if len(args.paths) != 1:
            parser.error(
                "--judge requires exactly one PATH (the judge output directory)"
            )
        if not args.judge_model:
            parser.error("--judge requires --judge-model")
        cmd_auto_judge(
            judge_dir=args.paths[0],
            model=args.judge_model,
            base_url=args.judge_base_url or args.base_url or "",
            api_key=args.judge_api_key or args.api_key,
        )
    elif args.summary:
        if len(args.paths) != 1:
            parser.error(
                "--summary requires exactly one PATH (the judge output directory)"
            )
        cmd_summary(args.paths[0])
    else:
        if not args.paths:
            parser.print_help()
            sys.exit(1)
        seed = args.seed if args.seed is not None else random.randint(0, 2**32 - 1)
        out_dir = args.out or (
            Path("results")
            / f"judge_{datetime.datetime.now().strftime('%Y%m%d_%H%M%S')}"
        )
        cmd_generate(args.paths, out_dir, seed)


if __name__ == "__main__":
    main()
