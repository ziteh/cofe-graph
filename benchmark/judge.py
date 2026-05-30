#!/usr/bin/env python3
"""
judge.py — blind LLM judge for benchmark

Collects responses from one or more result directories, assigns random labels
(A, B, C …) so the judge cannot see which condition produced each answer, then
either auto-judges via LLM or generates files for manual scoring.

After scoring, --summary deanonymises scores and prints a per-condition tally
with SVG charts.

Usage
-----
  # Full pipeline: run bench N times, auto-judge, summarise:
  uv run judge.py --run \\
    --model gemma4:e4b --base-url http://192.168.50.44:11434/v1 \\
    --judge-model gemma4:e4b --judge-base-url http://192.168.50.44:11434/v1 \\
    --question Q1 Q2 --runs 2

  # Generate judge prompts from existing result directories:
  uv run judge.py results/run1 results/run2 [--out results/judge_01] [--seed 42]

  # Deanonymise scores after filling judgment_template.json:
  uv run judge.py --summary results/judge_01
"""

import argparse
import json
import random
import re
import subprocess
import sys
from dataclasses import dataclass
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

from dotenv import load_dotenv
import openai

load_dotenv()

HERE = Path(__file__).resolve().parent


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
class ResultEntry:
    run: str
    condition: str
    response: str


@dataclass
class StatsEntry:
    run: str
    condition: str
    input_tokens: int
    output_tokens: int
    num_tool_calls: int
    duration_sec: float


@dataclass
class LabeledEntry:
    label: str
    entry: ResultEntry


@dataclass
class TemplateScore:
    criteria_scores: list[int | None]
    total: int | None
    notes: str


def _load_test_cases() -> list[TestCase]:
    data = json.loads((HERE / "test_cases.json").read_text(encoding="utf-8"))
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
TASK_BY_ID = {t.id: t for t in TEST_CASES}


def load_results(dirs: list[str]) -> dict[str, list[ResultEntry]]:
    results: dict[str, list[ResultEntry]] = {}
    for d in dirs:
        dp = Path(d)
        for cond in ("without", "with"):
            f = dp / f"{cond}.json"
            if not f.exists():
                continue
            raw = json.loads(f.read_text(encoding="utf-8"))
            entries: list[dict[str, Any]] = raw if isinstance(raw, list) else [raw]
            for e in entries:
                tid: str | None = e.get("task_id")
                if not tid:
                    continue
                results.setdefault(tid, []).append(
                    ResultEntry(
                        run=dp.name,
                        condition=cond,
                        response=e.get("response") or "",
                    )
                )
    return results


def load_stats(dirs: list[str]) -> dict[str, list[StatsEntry]]:
    stats: dict[str, list[StatsEntry]] = {}
    for d in dirs:
        dp = Path(d)
        for cond in ("without", "with"):
            f = dp / f"{cond}.json"
            if not f.exists():
                continue
            raw2 = json.loads(f.read_text(encoding="utf-8"))
            entries2: list[dict[str, Any]] = raw2 if isinstance(raw2, list) else [raw2]
            for e in entries2:
                tid2: str | None = e.get("task_id")
                if not tid2:
                    continue
                stats.setdefault(tid2, []).append(
                    StatsEntry(
                        run=dp.name,
                        condition=cond,
                        input_tokens=int(e.get("input_tokens") or 0),
                        output_tokens=int(e.get("output_tokens") or 0),
                        num_tool_calls=int(e.get("num_tool_calls") or 0),
                        duration_sec=float(e.get("duration_sec") or 0.0),
                    )
                )
    return stats


def assign_labels(entries: list[ResultEntry], rng: random.Random) -> list[LabeledEntry]:
    shuffled = entries[:]
    rng.shuffle(shuffled)
    alphabet = "ABCDEFGHIJKLMNOPQRSTUVWXYZ"
    return [LabeledEntry(label=alphabet[i], entry=e) for i, e in enumerate(shuffled)]


def format_task_block(task: TestCase, labeled: list[LabeledEntry]) -> str:
    rubric = task.rubric
    max_pts = sum(r.points for r in rubric)
    dw = task.difficulty_weight
    lines: list[str] = [
        f"=== TASK {task.id}: {task.function}  [difficulty_weight×{dw}] ===",
        "",
        "QUESTION:",
        f"  {task.question}",
        "",
        "RUBRIC  (score each criterion independently:",
        "          0 = wrong or missing,  full points = fully correct):",
    ]
    for i, r in enumerate(rubric):
        lines.append(
            f"  [{i + 1}] {r.criterion}  ({r.points} pt{'s' if r.points > 1 else ''})"
        )
    lines += [f"  MAX TOTAL: {max_pts} pts", ""]

    for le in labeled:
        lines += [f"--- RESPONSE {le.label} ---", le.entry.response.strip(), ""]

    lines += [
        "---",
        "For each response label and criterion [N], write one line:",
        '  "<Label>[<N>]: <score>/<max> — <brief note>"',
        "",
        "Then write totals:",
    ]
    for le in labeled:
        lines.append(f"  TOTAL {le.label}: ?/{max_pts}")
    label_str = " > ".join(le.label for le in labeled)
    lines += [
        "",
        f"  RANKING (best → worst):  {label_str}  ← reorder as appropriate",
        "",
        "=" * 72,
        "",
    ]
    return "\n".join(lines)


def build_template_entry(
    task: TestCase, labeled: list[LabeledEntry]
) -> dict[str, dict[str, Any]]:
    return {
        le.label: {
            "criteria_scores": [None] * len(task.rubric),
            "total": None,
            "notes": "",
        }
        for le in labeled
    }


JUDGE_SYSTEM = (
    "You are an expert embedded-systems C code reviewer. "
    "Score AI assistant responses about Flipper Zero firmware against a precise rubric. "
    "Be strict: award full points only when the answer is completely and exactly correct; "
    "award 0 for anything wrong, imprecise, or missing."
)


def build_judge_user_prompt(
    task: TestCase,
    labels: list[str],
    responses: dict[str, str],
) -> str:
    rubric = task.rubric
    max_pts = sum(r.points for r in rubric)
    lines: list[str] = [
        f"TASK: {task.function}  (id: {task.id})",
        "",
        "QUESTION:",
        task.question,
        "",
        "RUBRIC:",
    ]
    for i, r in enumerate(rubric):
        lines.append(
            f"  [{i + 1}] {r.criterion}  ({r.points} pt{'s' if r.points > 1 else ''})"
        )
    lines += [f"  MAX: {max_pts} pts", ""]
    for label in labels:
        lines += [f"--- RESPONSE {label} ---", (responses.get(label) or "").strip(), ""]
    schema_fields = ", ".join(f"0_or_{r.points}" for r in rubric)
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
        "Each criteria_scores entry must be either 0 or the criterion's full point value.",
        f"There must be exactly {len(rubric)} entries in criteria_scores, one per criterion.",
    ]
    return "\n".join(lines)


def parse_scores(text: str) -> dict[str, Any] | None:
    cleaned = re.sub(r"```(?:json)?\s*", "", text).strip()
    try:
        return json.loads(cleaned)
    except json.JSONDecodeError:
        pass
    m = re.search(r"(\{[\s\S]*\})", cleaned)
    if m:
        try:
            return json.loads(m.group(1))
        except json.JSONDecodeError:
            pass
    return None


def generate_charts(
    judge_dir: Path, chart_rows: list[dict[str, Any]], cond_stats: list[dict[str, Any]]
) -> None:
    import matplotlib

    matplotlib.use("Agg")
    import matplotlib.pyplot as plt

    colors = {"with": "#4e79a7", "without": "#f28e2b"}

    # Chart 1: accuracy by task, faceted by task
    # Pre-aggregate per task+condition
    agg: dict[tuple[str, str], list[float]] = {}
    for r in chart_rows:
        key = (r["task"], r["condition"])
        agg.setdefault(key, []).append(r["pct"])
    agg_scores = [
        {"task": t, "condition": c, "pct": sum(v) / len(v)} for (t, c), v in agg.items()
    ]

    task_ids = sorted({r["task"] for r in agg_scores})
    n_tasks = len(task_ids)
    fig, axes = plt.subplots(1, max(n_tasks, 1), figsize=(max(n_tasks * 2.5, 5), 4))
    if n_tasks == 1:
        axes = [axes]
    fig.suptitle("Accuracy by task (% of rubric score)", y=1.02)

    for ax, tid in zip(axes, task_ids):
        task_data = [r for r in agg_scores if r["task"] == tid]
        conds = [r["condition"] for r in task_data]
        pcts = [r["pct"] for r in task_data]
        bar_colors = [colors.get(c, "#999") for c in conds]
        bars = ax.bar(conds, pcts, color=bar_colors)
        ax.set_title(tid)
        ax.set_ylim(0, 110)
        ax.set_ylabel("Score (%)" if tid == task_ids[0] else "")
        ax.axhline(50, color="#bbb", linestyle="--", linewidth=0.8)
        for bar, pct in zip(bars, pcts):
            ax.text(
                bar.get_x() + bar.get_width() / 2,
                bar.get_height() + 1,
                f"{pct:.0f}%",
                ha="center",
                va="bottom",
                fontsize=9,
            )

    fig.tight_layout()
    fig.savefig(
        str(judge_dir / "chart_accuracy.svg"), format="svg", bbox_inches="tight"
    )
    plt.close(fig)
    print(f"[judge] Written: {judge_dir / 'chart_accuracy.svg'}")

    # Chart 2: token usage
    if cond_stats:
        metrics = [("input tokens", "in_tok"), ("output tokens", "out_tok")]
        fig, axes2 = plt.subplots(1, 2, figsize=(8, 4))
        fig.suptitle("Token usage (totals across all tasks)")
        for ax, (metric_label, field) in zip(axes2, metrics):
            conds = [s["condition"] for s in cond_stats]
            vals = [s[field] for s in cond_stats]
            bar_colors = [colors.get(c, "#999") for c in conds]
            bars = ax.bar(conds, vals, color=bar_colors)
            ax.set_title(metric_label)
            ax.set_ylabel("Tokens")
            for bar, val in zip(bars, vals):
                ax.text(
                    bar.get_x() + bar.get_width() / 2,
                    bar.get_height() * 1.01,
                    f"{val:,}",
                    ha="center",
                    va="bottom",
                    fontsize=9,
                )
        fig.tight_layout()
        fig.savefig(
            str(judge_dir / "chart_tokens.svg"), format="svg", bbox_inches="tight"
        )
        plt.close(fig)
        print(f"[judge] Written: {judge_dir / 'chart_tokens.svg'}")

    # Chart 3: tool calls
    if cond_stats:
        fig, ax3 = plt.subplots(figsize=(5, 4))
        fig.suptitle("Tool calls (totals across all tasks)")
        conds = [s["condition"] for s in cond_stats]
        vals = [s["tools"] for s in cond_stats]
        bar_colors = [colors.get(c, "#999") for c in conds]
        bars = ax3.bar(conds, vals, color=bar_colors)
        ax3.set_ylabel("Number of Tool Calls")
        for bar, val in zip(bars, vals):
            ax3.text(
                bar.get_x() + bar.get_width() / 2,
                bar.get_height() * 1.01,
                str(round(val)),
                ha="center",
                va="bottom",
                fontsize=9,
            )
        fig.tight_layout()
        fig.savefig(
            str(judge_dir / "chart_tool_calls.svg"), format="svg", bbox_inches="tight"
        )
        plt.close(fig)
        print(f"[judge] Written: {judge_dir / 'chart_tool_calls.svg'}")

    # Chart 4: duration
    if cond_stats:
        fig, ax4 = plt.subplots(figsize=(5, 4))
        fig.suptitle("Execution duration (totals across all tasks)")
        conds = [s["condition"] for s in cond_stats]
        vals = [s["secs"] for s in cond_stats]
        bar_colors = [colors.get(c, "#999") for c in conds]
        bars = ax4.bar(conds, vals, color=bar_colors)
        ax4.set_ylabel("Time (seconds)")
        for bar, val in zip(bars, vals):
            ax4.text(
                bar.get_x() + bar.get_width() / 2,
                bar.get_height() * 1.01,
                f"{val:.1f}s",
                ha="center",
                va="bottom",
                fontsize=9,
            )
        fig.tight_layout()
        fig.savefig(
            str(judge_dir / "chart_duration.svg"), format="svg", bbox_inches="tight"
        )
        plt.close(fig)
        print(f"[judge] Written: {judge_dir / 'chart_duration.svg'}")


def cmd_generate(result_dirs: list[str], out_dir: Path, seed: int) -> None:
    raw = load_results(result_dirs)
    if not raw:
        print("[judge] No results found in the specified directories.", file=sys.stderr)
        sys.exit(1)

    rng = random.Random(seed)
    blinding_map: dict[str, dict[str, dict]] = {}
    template: dict[str, dict] = {}
    prompt_blocks: list[str] = []
    found_ids: list[str] = []
    responses: dict[str, dict[str, str]] = {}

    for task in TEST_CASES:
        tid = task.id
        if tid not in raw:
            continue
        found_ids.append(tid)
        labeled = assign_labels(raw[tid], rng)
        blinding_map[tid] = {
            le.label: {"run": le.entry.run, "condition": le.entry.condition}
            for le in labeled
        }
        template[tid] = build_template_entry(task, labeled)
        responses[tid] = {le.label: le.entry.response for le in labeled}
        prompt_blocks.append(format_task_block(task, labeled))

    out_dir.mkdir(parents=True, exist_ok=True)

    header = (
        "# Benchmark — Blind Judge Input\n"
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
    (out_dir / "judge_input.txt").write_text(
        header + "\n".join(prompt_blocks) + footer, encoding="utf-8"
    )
    (out_dir / "blinding_map.json").write_text(
        json.dumps(
            {"seed": seed, "result_dirs": result_dirs, "map": blinding_map}, indent=2
        ),
        encoding="utf-8",
    )
    (out_dir / "judgment_template.json").write_text(
        json.dumps(template, indent=2), encoding="utf-8"
    )
    (out_dir / "responses.json").write_text(
        json.dumps(responses, indent=2), encoding="utf-8"
    )

    total_resp = sum(len(v) for v in blinding_map.values())
    per_task = total_resp // len(found_ids) if found_ids else 0
    print(f"[judge] tasks:      {len(found_ids)} ({', '.join(found_ids)})")
    print(f"[judge] responses:  {total_resp} total  ({per_task} per task)")
    print(f"[judge] seed:       {seed}")
    print(f"[judge] out dir:    {out_dir}")
    print()
    print("Next steps:")
    print(
        f"  1. uv run judge.py --judge {out_dir} --judge-model <model> --judge-base-url <url>"
    )
    print(f"     — or manually submit {out_dir / 'judge_input.txt'} to an LLM")
    print(
        f"  2. Fill    {out_dir / 'judgment_template.json'}  with the scores (if manual)"
    )
    print(f"  3. Run:    uv run judge.py --summary {out_dir}")


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
            print(f"[judge] Missing {f}", file=sys.stderr)
            sys.exit(1)

    if not resp_file.exists():
        print("[judge] responses.json not found — reconstructing from blinding_map ...")
        bm_data = json.loads(bm_file.read_text(encoding="utf-8"))
        result_dirs = bm_data.get("result_dirs") or []
        raw = load_results(result_dirs)
        bmap = bm_data["map"]
        responses_backfill: dict[str, dict[str, str]] = {}
        for tid, label_map in bmap.items():
            entries = raw.get(tid) or []
            responses_backfill[tid] = {}
            for label, info in label_map.items():
                match = next(
                    (
                        e
                        for e in entries
                        if e.run == info["run"] and e.condition == info["condition"]
                    ),
                    None,
                )
                responses_backfill[tid][label] = match.response if match else ""
        resp_file.write_text(json.dumps(responses_backfill, indent=2), encoding="utf-8")
        print(f"[judge] Written: {resp_file}")

    client = openai.OpenAI(base_url=base_url, api_key=api_key or "ollama")
    all_responses: dict[str, dict[str, str]] = json.loads(
        resp_file.read_text(encoding="utf-8")
    )
    tmpl: dict[str, dict[str, dict]] = json.loads(tmpl_file.read_text(encoding="utf-8"))

    for task in TEST_CASES:
        tid = task.id
        if tid not in tmpl:
            continue
        labels = list(tmpl[tid].keys())
        task_responses = all_responses.get(tid) or {}

        prompt = build_judge_user_prompt(task, labels, task_responses)
        print(
            f"[judge] scoring {tid} ({len(labels)} responses) ... ", end="", flush=True
        )

        raw_text = ""
        try:
            resp = client.chat.completions.create(
                model=model,
                messages=[
                    {"role": "system", "content": JUDGE_SYSTEM},
                    {"role": "user", "content": prompt},
                ],
                temperature=0,
            )
            raw_text = resp.choices[0].message.content or ""
        except Exception as e:
            print(f"ERROR: {e}")
            continue

        scores = parse_scores(raw_text)
        if scores is None:
            raw_path = judge_dir / f"{tid}_raw.txt"
            print(f"PARSE FAILED — raw output saved to {raw_path}")
            raw_path.write_text(raw_text, encoding="utf-8")
            continue

        ok_count = 0
        for label in labels:
            entry = scores.get(label)
            if not entry or not isinstance(entry.get("criteria_scores"), list):
                print(f"\n  [warn] missing scores for label {label}")
                continue
            cs = entry["criteria_scores"]
            clamped = [
                min(max(round(float(v)), 0), task.rubric[i].points)
                for i, v in enumerate(cs)
            ]
            tmpl[tid][label]["criteria_scores"] = clamped
            tmpl[tid][label]["total"] = sum(clamped)
            tmpl[tid][label]["notes"] = entry.get("notes") or ""
            ok_count += 1
        print(f"OK ({ok_count}/{len(labels)} labels scored)")

    tmpl_file.write_text(json.dumps(tmpl, indent=2), encoding="utf-8")
    print(f"[judge] Updated: {tmpl_file}")
    print()
    cmd_summary(judge_dir)


def cmd_summary(judge_dir: Path) -> None:
    bm_file = judge_dir / "blinding_map.json"
    tmpl_file = judge_dir / "judgment_template.json"
    for f in (bm_file, tmpl_file):
        if not f.exists():
            print(f"[judge] Missing {f}", file=sys.stderr)
            sys.exit(1)

    bm_data = json.loads(bm_file.read_text(encoding="utf-8"))
    tmpl: dict[str, dict[str, dict]] = json.loads(tmpl_file.read_text(encoding="utf-8"))
    blinding_map = bm_data["map"]
    result_dirs = bm_data.get("result_dirs") or []
    stats_by_task = load_stats(result_dirs)

    judgment: dict[str, Any] = {}
    cond_agg: dict[str, dict[str, float]] = {}

    col_w = {"task": 6, "fn": 36, "run": 24, "cond": 8, "score": 10, "wscore": 12}
    header = (
        f"{'Task':<{col_w['task']}}  {'Function':<{col_w['fn']}}  "
        f"{'Run':<{col_w['run']}}  {'Cond':<{col_w['cond']}}  "
        f"{'Score':>{col_w['score']}}  {'Weighted':>{col_w['wscore']}}"
    )
    rows: list[str] = []
    chart_rows: list[dict] = []

    for tid, labels_data in tmpl.items():
        task = TASK_BY_ID.get(tid)
        if not task:
            continue
        max_pts = sum(r.points for r in task.rubric)
        dw = task.difficulty_weight
        map_task = blinding_map.get(tid) or {}
        judgment[tid] = {}

        for label, scores_data in labels_data.items():
            info = map_task.get(label) or {}
            condition = info.get("condition") or "?"
            run = info.get("run") or "?"
            raw_scores = scores_data.get("criteria_scores")
            total: int | None = None
            if raw_scores and all(s is not None for s in raw_scores):
                total = sum(int(s) for s in raw_scores)
            elif scores_data.get("total") is not None:
                total = round(scores_data["total"])

            judgment[tid][label] = {
                "condition": condition,
                "run": run,
                "criteria_scores": raw_scores,
                "total": total,
                "difficulty_weight": dw,
                "notes": scores_data.get("notes") or "",
            }

            stat = next(
                (
                    s
                    for s in stats_by_task.get(tid) or []
                    if s.run == run and s.condition == condition
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
                    agg["in_tok"] += stat.input_tokens
                    agg["out_tok"] += stat.output_tokens
                    agg["tools"] += stat.num_tool_calls
                    agg["secs"] += stat.duration_sec

            score_str = f"{total}/{max_pts}" if total is not None else f"?/{max_pts}"
            wscore_str = (
                f"{total * dw:.1f}/{max_pts * dw:.1f}" if total is not None else "?"
            )
            rows.append(
                f"{tid:<{col_w['task']}}  {task.function:<{col_w['fn']}}  "
                f"{run:<{col_w['run']}}  {condition:<{col_w['cond']}}  "
                f"{score_str:>{col_w['score']}}  {wscore_str:>{col_w['wscore']}}"
            )
            if total is not None:
                chart_rows.append(
                    {
                        "task": tid,
                        "condition": condition,
                        "total": total,
                        "max": max_pts,
                        "pct": total / max_pts * 100,
                    }
                )

    judgment["_meta"] = {
        "seed": bm_data.get("seed"),
        "result_dirs": bm_data.get("result_dirs"),
        "condition_totals": cond_agg,
    }
    (judge_dir / "judgment.json").write_text(
        json.dumps(judgment, indent=2), encoding="utf-8"
    )

    sep = "-" * len(header)
    print(header)
    print(sep)
    for row in rows:
        print(row)
    print(sep)
    print()
    print("CONDITION TOTALS  (weighted score | tokens in/out | tool calls | time)")
    print()

    cond_stats: list[dict] = []
    for cond, data in sorted(cond_agg.items()):
        pct = (data["score"] / data["max"] * 100) if data["max"] else 0.0
        n = int(data["count"])
        print(
            f"  {cond:<10}  score {data['score']:>6.1f}/{data['max']:.1f} ({pct:.1f}%)"
            f"  [{n} resp]"
        )
        print(
            f"             in={int(data['in_tok']):,}  out={int(data['out_tok']):,}"
            f"  tools={int(data['tools'])}  time={data['secs']:.1f}s"
        )
        cond_stats.append(
            {
                "condition": cond,
                "score_pct": pct,
                "in_tok": int(data["in_tok"]),
                "out_tok": int(data["out_tok"]),
                "tools": int(data["tools"]),
                "secs": data["secs"],
            }
        )
    print()
    print(f"[judge] Written: {judge_dir / 'judgment.json'}")

    if chart_rows:
        generate_charts(judge_dir, chart_rows, cond_stats)


def cmd_run(
    model: str,
    base_url: str,
    question_ids: list[str],
    n_runs: int,
    condition: str,
    api_key: str | None,
    bin_path: str | None,
    project_path: str | None,
    out_dir: str | None,
    seed: int | None,
) -> str:
    bench_py = HERE / "bench.py"
    result_dirs: list[str] = []

    for i in range(n_runs):
        print(f"[judge] ── run {i + 1}/{n_runs} ")
        cmd = [
            sys.executable,
            str(bench_py),
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
        if bin_path:
            cmd += ["--bin", bin_path]
        if project_path:
            cmd += ["--project-path", project_path]

        run_dir: str | None = None
        proc = subprocess.Popen(
            cmd,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            text=True,
            cwd=str(HERE),
        )
        assert proc.stdout is not None
        for line in proc.stdout:
            print(line, end="")
            m = re.search(r"\[bench\] run dir\s*:\s*(\S+)", line)
            if m:
                run_dir = str(HERE / m.group(1))
        proc.wait()
        if proc.returncode != 0:
            print(f"[judge] bench.py exited with code {proc.returncode} — skipping run")

        if run_dir and Path(run_dir).exists():
            result_dirs.append(run_dir)
        else:
            print("[judge] Could not determine run dir from bench.py output")

    if not result_dirs:
        print("[judge] No successful bench runs — aborting.", file=sys.stderr)
        sys.exit(1)

    print(
        f"\n[judge] Collected {len(result_dirs)} run(s): "
        f"{[Path(d).name for d in result_dirs]}"
    )

    rng_seed = seed if seed is not None else random.randint(0, 2**32 - 1)

    final_out = (
        Path(out_dir)
        if out_dir
        else (
            HERE
            / "results"
            / f"judge_{datetime.now(timezone.utc).strftime('%Y%m%d_%H%M%S')}"
        )
    )
    cmd_generate(result_dirs, final_out, rng_seed)
    return str(final_out)


def main() -> None:
    parser = argparse.ArgumentParser(
        description="Blind LLM judge for code analysis benchmark",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog="""
Examples:
  # Full pipeline: run bench N times, auto-judge, summarise:
  uv run judge.py --run --model gemma4:e4b --base-url http://host/v1 \\
                  --question Q1 Q2 --runs 3 \\
                  --judge-model qwen2.5-coder:32b --judge-base-url http://host/v1

  # Auto-judge an existing judge dir:
  uv run judge.py --judge results/judge_01 \\
                  --judge-model qwen2.5-coder:32b --judge-base-url http://host/v1

  # Generate judge files from existing result directories:
  uv run judge.py results/run1 results/run2 [--out results/judge_01]

  # Deanonymise and report after filling judgment_template.json manually:
  uv run judge.py --summary results/judge_01
""",
    )

    parser.add_argument(
        "paths",
        nargs="*",
        help="Result directories (generate mode) or judge dir (--summary/--judge)",
    )
    parser.add_argument(
        "--run",
        action="store_true",
        help="Run bench.py N times then generate (and optionally auto-judge) files",
    )
    parser.add_argument(
        "--judge",
        action="store_true",
        help="Auto-judge an existing judge dir using an LLM",
    )
    parser.add_argument(
        "--summary",
        action="store_true",
        help="Deanonymise scores and print per-condition report",
    )

    parser.add_argument("--model", help="Model name passed to bench.py")
    parser.add_argument(
        "--base-url", dest="base_url", help="Base URL passed to bench.py"
    )
    parser.add_argument("--api-key", dest="api_key", help="API key passed to bench.py")
    parser.add_argument(
        "--condition", default="both", help="Condition(s) to run: with | without | both"
    )
    parser.add_argument(
        "--question", nargs="+", help="Question IDs to benchmark, e.g. --question Q1 Q2"
    )
    parser.add_argument("--runs", type=int, default=1, help="Number of bench.py runs")
    parser.add_argument("--bin", help="Server binary path")
    parser.add_argument(
        "--project-path", dest="project_path", help="Project path to analyze"
    )
    parser.add_argument(
        "--judge-model", dest="judge_model", help="LLM model for auto-judging responses"
    )
    parser.add_argument(
        "--judge-base-url",
        dest="judge_base_url",
        help="Base URL for judge LLM (defaults to --base-url)",
    )
    parser.add_argument(
        "--judge-api-key",
        dest="judge_api_key",
        help="API key for judge LLM (defaults to --api-key)",
    )
    parser.add_argument("--out", help="Output directory for judge files")
    parser.add_argument("--seed", type=int, help="Random seed for label assignment")

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
            bin_path=args.bin,
            project_path=args.project_path,
            out_dir=args.out,
            seed=args.seed,
        )
        if args.judge_model:
            print()
            cmd_auto_judge(
                Path(judge_dir),
                args.judge_model,
                args.judge_base_url or args.base_url or "",
                args.judge_api_key or args.api_key,
            )

    elif args.judge:
        if len(args.paths) != 1:
            parser.error(
                "--judge requires exactly one PATH (the judge output directory)"
            )
        if not args.judge_model:
            parser.error("--judge requires --judge-model")
        judge_base = args.judge_base_url or args.base_url
        if not judge_base:
            parser.error("--judge requires --judge-base-url or --base-url")
        cmd_auto_judge(
            Path(args.paths[0]),
            args.judge_model,
            judge_base,
            args.judge_api_key or args.api_key,
        )

    elif args.summary:
        path = args.paths[0] if args.paths else None
        if not path:
            parser.error("--summary requires a PATH (the judge output directory)")
        cmd_summary(Path(path))

    else:
        if not args.paths:
            parser.print_help()
            sys.exit(0)
        out_dir = (
            Path(args.out)
            if args.out
            else (
                HERE
                / "results"
                / f"judge_{datetime.now(timezone.utc).strftime('%Y%m%d_%H%M%S')}"
            )
        )
        seed = args.seed if args.seed is not None else random.randint(0, 2**32 - 1)
        cmd_generate(args.paths, out_dir, seed)


if __name__ == "__main__":
    main()
