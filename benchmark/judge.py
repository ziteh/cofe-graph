#!/usr/bin/env python3
"""
judge.py — blind LLM judge for benchmark

Collects responses from result directories, assigns random labels (A, B, …)
so the judge cannot see which condition produced each answer, then either
auto-judges via LLM or generates files for manual scoring.

After scoring, --summary deanonymises scores and prints a per-condition tally.

Usage
-----
  # Generate blind judge files from result directories:
  uv run judge.py results/20240101_120000 [results/20240101_130000 ...]

  # Auto-judge an existing judge dir:
  uv run judge.py --judge results/judge_01 \\
    --judge-model claude-haiku-4-5-20251001 --judge-base-url https://api.anthropic.com

  # Deanonymise scores after filling judgment_template.json:
  uv run judge.py --summary results/judge_01
"""

import argparse
import json
import random
import re
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
    question: str
    rubric: list[Criterion]


@dataclass
class ResultEntry:
    run: str
    condition: str
    response: str


@dataclass
class LabeledEntry:
    label: str
    entry: ResultEntry


def _load_questions() -> list[TestCase]:
    data = json.loads((HERE / "cases" / "questions.json").read_text(encoding="utf-8"))
    return [
        TestCase(
            id=tc["id"],
            question=tc["question"],
            rubric=[Criterion(**c) for c in tc["rubric"]],
        )
        for tc in data
    ]


QUESTIONS = _load_questions()
QUESTION_BY_ID = {q.id: q for q in QUESTIONS}

CONDITIONS = ("exp", "ctrl")


def load_results(dirs: list[str]) -> dict[str, list[ResultEntry]]:
    results: dict[str, list[ResultEntry]] = {}
    for d in dirs:
        dp = Path(d)
        for cond in CONDITIONS:
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


def load_stats(dirs: list[str]) -> dict[str, list[dict[str, Any]]]:
    stats: dict[str, list[dict[str, Any]]] = {}
    for d in dirs:
        dp = Path(d)
        for cond in CONDITIONS:
            f = dp / f"{cond}.json"
            if not f.exists():
                continue
            raw = json.loads(f.read_text(encoding="utf-8"))
            entries: list[dict[str, Any]] = raw if isinstance(raw, list) else [raw]
            for e in entries:
                tid: str | None = e.get("task_id")
                if not tid:
                    continue
                stats.setdefault(tid, []).append(
                    {
                        "run": dp.name,
                        "condition": cond,
                        "duration_sec": float(e.get("duration_sec") or 0.0),
                    }
                )
    return stats


def assign_labels(entries: list[ResultEntry], rng: random.Random) -> list[LabeledEntry]:
    shuffled = entries[:]
    rng.shuffle(shuffled)
    alphabet = "ABCDEFGHIJKLMNOPQRSTUVWXYZ"
    return [LabeledEntry(label=alphabet[i], entry=e) for i, e in enumerate(shuffled)]


JUDGE_SYSTEM = (
    "You are an expert embedded-systems C code reviewer. "
    "Score AI assistant responses about Flipper Zero firmware against a precise rubric. "
    "Be strict: award full points only when the answer is completely and exactly correct; "
    "award 0 for anything wrong, imprecise, or missing."
)


def build_judge_prompt(
    task: TestCase,
    labels: list[str],
    responses: dict[str, str],
) -> str:
    max_pts = sum(r.points for r in task.rubric)
    lines: list[str] = [
        f"TASK: {task.id}",
        "",
        "QUESTION:",
        task.question,
        "",
        "RUBRIC:",
    ]
    for i, r in enumerate(task.rubric):
        lines.append(f"  [{i + 1}] {r.criterion}  ({r.points} pt{'s' if r.points > 1 else ''})")
    lines += [f"  MAX: {max_pts} pts", ""]
    for label in labels:
        lines += [f"--- RESPONSE {label} ---", (responses.get(label) or "").strip(), ""]
    schema_fields = ", ".join(f"0_or_{r.points}" for r in task.rubric)
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
        f"There must be exactly {len(task.rubric)} entries in criteria_scores, one per criterion.",
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


def cmd_generate(result_dirs: list[str], out_dir: Path, seed: int) -> None:
    raw = load_results(result_dirs)
    if not raw:
        print("[judge] No results found in the specified directories.", file=sys.stderr)
        sys.exit(1)

    rng = random.Random(seed)
    blinding_map: dict[str, dict[str, dict]] = {}
    template: dict[str, dict] = {}
    responses: dict[str, dict[str, str]] = {}
    found_ids: list[str] = []

    for task in QUESTIONS:
        tid = task.id
        if tid not in raw:
            continue
        found_ids.append(tid)
        labeled = assign_labels(raw[tid], rng)
        blinding_map[tid] = {
            le.label: {"run": le.entry.run, "condition": le.entry.condition}
            for le in labeled
        }
        template[tid] = {
            le.label: {"criteria_scores": [None] * len(task.rubric), "total": None, "notes": ""}
            for le in labeled
        }
        responses[tid] = {le.label: le.entry.response for le in labeled}

    out_dir.mkdir(parents=True, exist_ok=True)
    (out_dir / "blinding_map.json").write_text(
        json.dumps({"seed": seed, "result_dirs": result_dirs, "map": blinding_map}, indent=2)
    )
    (out_dir / "judgment_template.json").write_text(json.dumps(template, indent=2))
    (out_dir / "responses.json").write_text(json.dumps(responses, indent=2))

    total_resp = sum(len(v) for v in blinding_map.values())
    per_task = total_resp // len(found_ids) if found_ids else 0
    print(f"[judge] tasks:     {len(found_ids)} ({', '.join(found_ids)})")
    print(f"[judge] responses: {total_resp} total  ({per_task} per task)")
    print(f"[judge] seed:      {seed}")
    print(f"[judge] out dir:   {out_dir}")
    print()
    print("Next steps:")
    print(f"  uv run judge.py --judge {out_dir} --judge-model <model> --judge-base-url <url>")
    print(f"  uv run judge.py --summary {out_dir}")


def cmd_auto_judge(judge_dir: Path, model: str, base_url: str, api_key: str | None) -> None:
    resp_file = judge_dir / "responses.json"
    tmpl_file = judge_dir / "judgment_template.json"
    bm_file = judge_dir / "blinding_map.json"

    for f in (tmpl_file, bm_file, resp_file):
        if not f.exists():
            print(f"[judge] Missing {f}", file=sys.stderr)
            sys.exit(1)

    client = openai.OpenAI(base_url=base_url, api_key=api_key or "unused")
    all_responses: dict[str, dict[str, str]] = json.loads(resp_file.read_text())
    tmpl: dict[str, dict[str, dict]] = json.loads(tmpl_file.read_text())

    for task in QUESTIONS:
        tid = task.id
        if tid not in tmpl:
            continue
        labels = list(tmpl[tid].keys())
        task_responses = all_responses.get(tid) or {}

        prompt = build_judge_prompt(task, labels, task_responses)
        print(f"[judge] scoring {tid} ({len(labels)} responses) ... ", end="", flush=True)

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
            raw_path.write_text(raw_text)
            print(f"PARSE FAILED — raw output saved to {raw_path}")
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

    tmpl_file.write_text(json.dumps(tmpl, indent=2))
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

    bm_data = json.loads(bm_file.read_text())
    tmpl: dict[str, dict[str, dict]] = json.loads(tmpl_file.read_text())
    blinding_map = bm_data["map"]
    result_dirs = bm_data.get("result_dirs") or []
    stats_by_task = load_stats(result_dirs)

    cond_agg: dict[str, dict[str, float]] = {}
    rows: list[str] = []
    judgment: dict[str, Any] = {}

    col_w = {"task": 6, "run": 24, "cond": 6, "score": 10}
    header = (
        f"{'Task':<{col_w['task']}}  {'Run':<{col_w['run']}}  "
        f"{'Cond':<{col_w['cond']}}  {'Score':>{col_w['score']}}"
    )

    for tid, labels_data in tmpl.items():
        task = QUESTION_BY_ID.get(tid)
        if not task:
            continue
        max_pts = sum(r.points for r in task.rubric)
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

            stat = next(
                (s for s in stats_by_task.get(tid) or [] if s["run"] == run and s["condition"] == condition),
                None,
            )

            judgment[tid][label] = {
                "condition": condition,
                "run": run,
                "criteria_scores": raw_scores,
                "total": total,
                "notes": scores_data.get("notes") or "",
            }

            if total is not None:
                agg = cond_agg.setdefault(condition, {"score": 0.0, "max": 0.0, "count": 0, "secs": 0.0})
                agg["score"] += total
                agg["max"] += max_pts
                agg["count"] += 1
                if stat:
                    agg["secs"] += stat["duration_sec"]

            score_str = f"{total}/{max_pts}" if total is not None else f"?/{max_pts}"
            rows.append(
                f"{tid:<{col_w['task']}}  {run:<{col_w['run']}}  "
                f"{condition:<{col_w['cond']}}  {score_str:>{col_w['score']}}"
            )

    judgment["_meta"] = {"seed": bm_data.get("seed"), "result_dirs": result_dirs, "condition_totals": cond_agg}
    (judge_dir / "judgment.json").write_text(json.dumps(judgment, indent=2))

    sep = "-" * len(header)
    print(header)
    print(sep)
    for row in rows:
        print(row)
    print(sep)
    print()
    print("CONDITION TOTALS")
    print()
    for cond, data in sorted(cond_agg.items()):
        pct = (data["score"] / data["max"] * 100) if data["max"] else 0.0
        n = int(data["count"])
        print(
            f"  {cond:<6}  score {data['score']:>5.1f}/{data['max']:.1f} ({pct:.1f}%)"
            f"  [{n} resp]  time={data['secs']:.1f}s"
        )
    print()
    print(f"[judge] Written: {judge_dir / 'judgment.json'}")


def main() -> None:
    parser = argparse.ArgumentParser(
        description="Blind LLM judge for code analysis benchmark",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog="""
Examples:
  # Generate blind judge files from result directories:
  uv run judge.py results/20240101_120000 [results/20240101_130000 ...]

  # Auto-judge using an LLM:
  uv run judge.py --judge results/judge_01 \\
    --judge-model claude-haiku-4-5-20251001 --judge-base-url https://api.anthropic.com

  # Summarise after filling judgment_template.json manually:
  uv run judge.py --summary results/judge_01
""",
    )

    parser.add_argument("paths", nargs="*", help="Result directories or judge dir")
    parser.add_argument("--judge", action="store_true", help="Auto-judge an existing judge dir")
    parser.add_argument("--summary", action="store_true", help="Deanonymise scores and report")
    parser.add_argument("--judge-model", dest="judge_model")
    parser.add_argument("--judge-base-url", dest="judge_base_url")
    parser.add_argument("--judge-api-key", dest="judge_api_key")
    parser.add_argument("--out", help="Output directory for judge files")
    parser.add_argument("--seed", type=int)
    args = parser.parse_args()

    if args.judge:
        if len(args.paths) != 1:
            parser.error("--judge requires exactly one PATH (the judge output directory)")
        if not args.judge_model or not args.judge_base_url:
            parser.error("--judge requires --judge-model and --judge-base-url")
        cmd_auto_judge(Path(args.paths[0]), args.judge_model, args.judge_base_url, args.judge_api_key)

    elif args.summary:
        if not args.paths:
            parser.error("--summary requires a PATH (the judge output directory)")
        cmd_summary(Path(args.paths[0]))

    else:
        if not args.paths:
            parser.print_help()
            sys.exit(0)
        out_dir = (
            Path(args.out)
            if args.out
            else HERE / "results" / f"judge_{datetime.now(timezone.utc).strftime('%Y%m%d_%H%M%S')}"
        )
        seed = args.seed if args.seed is not None else random.randint(0, 2**32 - 1)
        cmd_generate(args.paths, out_dir, seed)


if __name__ == "__main__":
    main()
