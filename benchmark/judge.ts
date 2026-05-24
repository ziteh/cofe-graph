#!/usr/bin/env tsx
/**
 * judge.ts — blind LLM judge for cofe-graph benchmark
 *
 * Collects responses from one or more result directories, assigns random labels
 * (A, B, C …) so the judge cannot see which condition produced each answer, then
 * generates a formatted prompt file for manual submission to an LLM judge.
 *
 * After the judge fills in judgment_template.json, --summary deanonymises scores
 * and prints a per-condition tally.
 *
 * Usage
 * -----
 *   # Full pipeline:
 *   npx tsx judge.ts --run \
 *     --model gemma4:e4b --base-url http://192.168.50.44:11434/v1 \
 *     --judge-model gemma4:e4b --judge-base-url http://192.168.50.44:11434/v1 \
 *     --question Q1 Q2 --runs 2
 *
 *   # Generate judge prompts from existing result directories:
 *   npx tsx judge.ts results/run1 results/run2 [--out results/judge_01] [--seed 42]
 *
 *   # Deanonymise scores after filling judgment_template.json:
 *   npx tsx judge.ts --summary results/judge_01
 */

import "dotenv/config";
import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { spawn } from "node:child_process";
import { fileURLToPath } from "node:url";
import { Command } from "commander";
import seedrandom from "seedrandom";
import OpenAI from "openai";
import * as Plot from "@observablehq/plot";
import { JSDOM } from "jsdom";

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);
const HERE = resolve(__dirname);

interface Criterion {
  criterion: string;
  points: number;
}

interface TestCase {
  id: string;
  difficulty_weight: number;
  function: string;
  question: string;
  rubric: Criterion[];
}

interface ResultEntry {
  run: string;
  condition: string;
  response: string;
}

interface StatsEntry {
  run: string;
  condition: string;
  input_tokens: number;
  output_tokens: number;
  num_tool_calls: number;
  duration_sec: number;
}

interface ChartRow {
  task: string;
  condition: string;
  total: number;
  max: number;
  pct: number;
}

interface CondStats {
  condition: string;
  score_pct: number;
  in_tok: number;
  out_tok: number;
  tools: number;
  secs: number;
}

interface LabeledEntry {
  label: string;
  entry: ResultEntry;
}

interface TemplateScore {
  criteria_scores: (number | null)[];
  total: number | null;
  notes: string;
}

const TEST_CASES: TestCase[] = JSON.parse(
  readFileSync(join(HERE, "test_cases.json"), "utf-8")
);
const TASK_BY_ID = Object.fromEntries(TEST_CASES.map((t) => [t.id, t]));

function loadResults(dirs: string[]): Record<string, ResultEntry[]> {
  const results: Record<string, ResultEntry[]> = {};
  for (const d of dirs) {
    for (const cond of ["without", "with"]) {
      const f = join(d, `${cond}.json`);
      if (!existsSync(f)) continue;
      const raw = JSON.parse(readFileSync(f, "utf-8"));
      const entries: unknown[] = Array.isArray(raw) ? raw : [raw];
      for (const e of entries) {
        const entry = e as Record<string, unknown>;
        const tid = entry["task_id"] as string | undefined;
        if (!tid) continue;
        (results[tid] ??= []).push({
          run: (d.split("/").pop() ?? d),
          condition: cond,
          response: (entry["response"] as string) ?? "",
        });
      }
    }
  }
  return results;
}

function loadStats(dirs: string[]): Record<string, StatsEntry[]> {
  const stats: Record<string, StatsEntry[]> = {};
  for (const d of dirs) {
    for (const cond of ["without", "with"]) {
      const f = join(d, `${cond}.json`);
      if (!existsSync(f)) continue;
      const raw = JSON.parse(readFileSync(f, "utf-8"));
      const entries: unknown[] = Array.isArray(raw) ? raw : [raw];
      for (const e of entries) {
        const entry = e as Record<string, unknown>;
        const tid = entry["task_id"] as string | undefined;
        if (!tid) continue;
        (stats[tid] ??= []).push({
          run: (d.split("/").pop() ?? d),
          condition: cond,
          input_tokens: (entry["input_tokens"] as number) ?? 0,
          output_tokens: (entry["output_tokens"] as number) ?? 0,
          num_tool_calls: (entry["num_tool_calls"] as number) ?? 0,
          duration_sec: (entry["duration_sec"] as number) ?? 0,
        });
      }
    }
  }
  return stats;
}

function assignLabels(entries: ResultEntry[], rng: seedrandom.PRNG): LabeledEntry[] {
  const shuffled = [...entries];
  for (let i = shuffled.length - 1; i > 0; i--) {
    const j = Math.floor(rng() * (i + 1));
    [shuffled[i], shuffled[j]] = [shuffled[j], shuffled[i]];
  }
  return shuffled.map((entry, idx) => ({
    label: "ABCDEFGHIJKLMNOPQRSTUVWXYZ"[idx],
    entry,
  }));
}

function formatTaskBlock(task: TestCase, labeled: LabeledEntry[]): string {
  const rubric = task.rubric;
  const maxPts = rubric.reduce((s, r) => s + r.points, 0);
  const dw = task.difficulty_weight;
  const lines: string[] = [
    `=== TASK ${task.id}: ${task.function}  [difficulty_weight×${dw}] ===`,
    "",
    "QUESTION:",
    `  ${task.question}`,
    "",
    "RUBRIC  (score each criterion independently:",
    "          0 = wrong or missing,  full points = fully correct):",
  ];
  for (let i = 0; i < rubric.length; i++) {
    const r = rubric[i];
    lines.push(`  [${i + 1}] ${r.criterion}  (${r.points} pt${r.points > 1 ? "s" : ""})`);
  }
  lines.push(`  MAX TOTAL: ${maxPts} pts`, "");

  for (const { label, entry } of labeled) {
    lines.push(`--- RESPONSE ${label} ---`, entry.response.trim(), "");
  }

  lines.push(
    "---",
    "For each response label and criterion [N], write one line:",
    '  "<Label>[<N>]: <score>/<max> — <brief note>"',
    "",
    "Then write totals:"
  );
  for (const { label } of labeled) {
    lines.push(`  TOTAL ${label}: ?/${maxPts}`);
  }
  const labelStr = labeled.map((l) => l.label).join(" > ");
  lines.push(
    "",
    `  RANKING (best → worst):  ${labelStr}  ← reorder as appropriate`,
    "",
    "=".repeat(72),
    ""
  );
  return lines.join("\n");
}

function buildTemplateEntry(
  task: TestCase,
  labeled: LabeledEntry[]
): Record<string, TemplateScore> {
  return Object.fromEntries(
    labeled.map(({ label }) => [
      label,
      { criteria_scores: new Array(task.rubric.length).fill(null), total: null, notes: "" },
    ])
  );
}

const JUDGE_SYSTEM =
  "You are an expert embedded-systems C code reviewer. " +
  "Score AI assistant responses about Flipper Zero firmware against a precise rubric. " +
  "Be strict: award full points only when the answer is completely and exactly correct; " +
  "award 0 for anything wrong, imprecise, or missing.";

function buildJudgeUserPrompt(
  task: TestCase,
  labels: string[],
  responses: Record<string, string>
): string {
  const rubric = task.rubric;
  const maxPts = rubric.reduce((s, r) => s + r.points, 0);
  const lines: string[] = [
    `TASK: ${task.function}  (id: ${task.id})`,
    "",
    "QUESTION:",
    task.question,
    "",
    "RUBRIC:",
  ];
  for (let i = 0; i < rubric.length; i++) {
    const r = rubric[i];
    lines.push(`  [${i + 1}] ${r.criterion}  (${r.points} pt${r.points > 1 ? "s" : ""})`);
  }
  lines.push(`  MAX: ${maxPts} pts`, "");
  for (const label of labels) {
    const resp = responses[label] ?? "";
    lines.push(`--- RESPONSE ${label} ---`, resp.trim(), "");
  }
  const schemaFields = rubric.map((r) => `0_or_${r.points}`).join(", ");
  lines.push("---", "Respond with ONLY a JSON object — no explanation, no markdown fences:", "{");
  for (const label of labels) {
    lines.push(
      `  "${label}": {"criteria_scores": [${schemaFields}], "notes": "<one sentence>"}`
    );
  }
  lines.push(
    "}",
    "",
    `Each criteria_scores entry must be either 0 or the criterion's full point value.`,
    `There must be exactly ${rubric.length} entries in criteria_scores, one per criterion.`
  );
  return lines.join("\n");
}

function parseScores(text: string): Record<string, unknown> | null {
  const cleaned = text.replace(/```(?:json)?\s*/g, "").trim();
  try {
    return JSON.parse(cleaned) as Record<string, unknown>;
  } catch {
    // ignore
  }
  const m = cleaned.match(/(\{[\s\S]*\})/);
  if (m) {
    try {
      return JSON.parse(m[1]) as Record<string, unknown>;
    } catch {
      // ignore
    }
  }
  return null;
}

async function cmdAutoJudge(
  judgeDir: string,
  model: string,
  baseUrl: string,
  apiKey?: string
): Promise<void> {
  const respFile = join(judgeDir, "responses.json");
  const tmplFile = join(judgeDir, "judgment_template.json");
  const bmFile = join(judgeDir, "blinding_map.json");

  for (const f of [tmplFile, bmFile]) {
    if (!existsSync(f)) {
      console.error(`[judge] Missing ${f}`);
      process.exit(1);
    }
  }

  if (!existsSync(respFile)) {
    console.log("[judge] responses.json not found — reconstructing from blinding_map ...");
    const bmData = JSON.parse(readFileSync(bmFile, "utf-8")) as {
      result_dirs: string[];
      map: Record<string, Record<string, { run: string; condition: string }>>;
    };
    const resultDirs = bmData.result_dirs ?? [];
    const raw = loadResults(resultDirs);
    const bmap = bmData.map;
    const responsesBackfill: Record<string, Record<string, string>> = {};
    for (const [tid, labelMap] of Object.entries(bmap)) {
      const entries = raw[tid] ?? [];
      responsesBackfill[tid] = {};
      for (const [label, info] of Object.entries(labelMap)) {
        const match = entries.find(
          (e) => e.run === info.run && e.condition === info.condition
        );
        responsesBackfill[tid][label] = match?.response ?? "";
      }
    }
    writeFileSync(respFile, JSON.stringify(responsesBackfill, null, 2));
    console.log(`[judge] Written: ${respFile}`);
  }

  const client = new OpenAI({ baseURL: baseUrl, apiKey: apiKey ?? "ollama" });
  const allResponses: Record<string, Record<string, string>> = JSON.parse(
    readFileSync(respFile, "utf-8")
  );
  const tmpl: Record<string, Record<string, TemplateScore>> = JSON.parse(
    readFileSync(tmplFile, "utf-8")
  );

  for (const task of TEST_CASES) {
    const tid = task.id;
    if (!(tid in tmpl)) continue;
    const labels = Object.keys(tmpl[tid]);
    const taskResponses = allResponses[tid] ?? {};

    const prompt = buildJudgeUserPrompt(task, labels, taskResponses);
    process.stdout.write(
      `[judge] scoring ${tid} (${labels.length} responses) ... `
    );

    let rawText = "";
    try {
      const resp = await client.chat.completions.create({
        model,
        messages: [
          { role: "system", content: JUDGE_SYSTEM },
          { role: "user", content: prompt },
        ],
        temperature: 0,
      });
      rawText = resp.choices[0]?.message?.content ?? "";
    } catch (err) {
      console.log(`ERROR: ${(err as Error).message}`);
      continue;
    }

    const scores = parseScores(rawText);
    if (scores === null) {
      const rawPath = join(judgeDir, `${tid}_raw.txt`);
      console.log(`PARSE FAILED — raw output saved to ${rawPath}`);
      writeFileSync(rawPath, rawText);
      continue;
    }

    let okCount = 0;
    for (const label of labels) {
      const entry = scores[label] as
        | { criteria_scores?: unknown[]; notes?: string }
        | undefined;
      if (!entry || !Array.isArray(entry.criteria_scores)) {
        console.log(`\n  [warn] missing scores for label ${label}`);
        continue;
      }
      const cs = entry.criteria_scores;
      const clamped = cs.map((v, i) =>
        Math.min(Math.max(Math.round(Number(v)), 0), task.rubric[i].points)
      );
      tmpl[tid][label].criteria_scores = clamped;
      tmpl[tid][label].total = clamped.reduce((s, x) => s + x, 0);
      tmpl[tid][label].notes = (entry.notes as string) ?? "";
      okCount++;
    }
    console.log(`OK (${okCount}/${labels.length} labels scored)`);
  }

  writeFileSync(tmplFile, JSON.stringify(tmpl, null, 2));
  console.log(`[judge] Updated: ${tmplFile}`);
  console.log();
  cmdSummary(judgeDir);
}

function cmdGenerate(resultDirs: string[], outDir: string, seed: number): void {
  const raw = loadResults(resultDirs);
  if (Object.keys(raw).length === 0) {
    console.error("[judge] No results found in the specified directories.");
    process.exit(1);
  }

  const rng = seedrandom(String(seed));
  const blindingMap: Record<
    string,
    Record<string, { run: string; condition: string }>
  > = {};
  const template: Record<string, Record<string, TemplateScore>> = {};
  const promptBlocks: string[] = [];
  const foundIds: string[] = [];
  const responses: Record<string, Record<string, string>> = {};

  for (const task of TEST_CASES) {
    const tid = task.id;
    if (!(tid in raw)) continue;
    foundIds.push(tid);
    const labeled = assignLabels(raw[tid], rng);
    blindingMap[tid] = Object.fromEntries(
      labeled.map(({ label, entry }) => [
        label,
        { run: entry.run, condition: entry.condition },
      ])
    );
    template[tid] = buildTemplateEntry(task, labeled);
    responses[tid] = Object.fromEntries(
      labeled.map(({ label, entry }) => [label, entry.response])
    );
    promptBlocks.push(formatTaskBlock(task, labeled));
  }

  mkdirSync(outDir, { recursive: true });

  const header =
    "# cofe-graph Benchmark — Blind Judge Input\n" +
    `# Seed: ${seed}    Tasks: ${foundIds.join(", ")}\n` +
    "#\n" +
    "# Instructions:\n" +
    "#   Score each response against the rubric for its task.\n" +
    "#   Do NOT guess which system produced each answer.\n" +
    "#   Fill in judgment_template.json with your per-criterion scores.\n\n";
  const footer =
    `\n\n# When done, write your scores into:\n` +
    `#   ${join(outDir, "judgment_template.json")}\n`;
  writeFileSync(
    join(outDir, "judge_input.txt"),
    header + promptBlocks.join("\n") + footer
  );

  writeFileSync(
    join(outDir, "blinding_map.json"),
    JSON.stringify({ seed, result_dirs: resultDirs, map: blindingMap }, null, 2)
  );

  writeFileSync(join(outDir, "judgment_template.json"), JSON.stringify(template, null, 2));
  writeFileSync(join(outDir, "responses.json"), JSON.stringify(responses, null, 2));

  const totalResp = Object.values(blindingMap).reduce(
    (s, v) => s + Object.keys(v).length,
    0
  );
  console.log(`[judge] tasks:      ${foundIds.length} (${foundIds.join(", ")})`);
  console.log(
    `[judge] responses:  ${totalResp} total  (${foundIds.length ? Math.floor(totalResp / foundIds.length) : 0} per task)`
  );
  console.log(`[judge] seed:       ${seed}`);
  console.log(`[judge] out dir:    ${outDir}`);
  console.log();
  console.log("Next steps:");
  console.log(
    `  1. pnpm judge --judge ${outDir} --judge-model <model> --judge-base-url <url>`
  );
  console.log(`     — or manually submit ${join(outDir, "judge_input.txt")} to an LLM`);
  console.log(
    `  2. Fill    ${join(outDir, "judgment_template.json")}  with the scores (if manual)`
  );
  console.log(`  3. Run:    pnpm judge --summary ${outDir}`);
}

function renderPlot(title: string, spec: Plot.PlotOptions): string {
  const { document } = new JSDOM("").window;
  // Strip options that cause Plot to wrap output in <figure> (title, caption, legend).
  // color.legend in particular forces an HTML legend div which can't live in SVG.
  const { title: _t, subtitle: _s, caption: _c, color: colorOpt, ...rest } = spec as Plot.PlotOptions & { subtitle?: string; caption?: string };
  const sanitizedColor = colorOpt ? { ...colorOpt, legend: false } : undefined;
  const plotSpec = { ...rest, ...(sanitizedColor ? { color: sanitizedColor } : {}), style: { background: "white" }, document };
  const el = Plot.plot(plotSpec as Plot.PlotOptions & { document: Document });
  const svg = el as unknown as SVGSVGElement;
  (svg as unknown as Element).setAttribute("xmlns", "http://www.w3.org/2000/svg");
  const titleEl = document.createElementNS("http://www.w3.org/2000/svg", "title");
  titleEl.textContent = title;
  svg.prepend(titleEl);
  return '<?xml version="1.0" encoding="utf-8"?>\n' + svg.outerHTML;
}

function generateCharts(judgeDir: string, chartRows: ChartRow[], condStats: CondStats[]): void {
  // Chart 1: per-task score % faceted by task, condition on x-axis (no HTML legend needed)
  const scoresSvg = renderPlot("Score % by task and condition", {
    width: 800,
    height: 360,
    fx: { label: "Task" },
    x: { label: "Condition" },
    y: { label: "Score (%)", domain: [0, 100], grid: true },
    color: { legend: false },
    marks: [
      Plot.barY(chartRows, Plot.groupX({ y: "mean" }, {
        fx: "task",
        x: "condition",
        y: "pct",
        fill: "condition",
        tip: true,
      })),
      Plot.ruleY([0]),
    ],
  });

  // Chart 2: condition stats faceted by metric, condition on x-axis
  const statRows: { condition: string; metric: string; value: number }[] = [];
  for (const s of condStats) {
    statRows.push(
      { condition: s.condition, metric: "score %", value: s.score_pct },
      { condition: s.condition, metric: "tools", value: s.tools },
      { condition: s.condition, metric: "time (s)", value: s.secs },
    );
  }

  const statsSvg = renderPlot("Condition stats comparison", {
    width: 800,
    height: 360,
    fx: { label: "Metric" },
    x: { label: "Condition" },
    y: { label: "Value", grid: true },
    color: { legend: false },
    marks: [
      Plot.barY(statRows, {
        fx: "metric",
        x: "condition",
        y: "value",
        fill: "condition",
        tip: true,
      }),
      Plot.ruleY([0]),
    ],
  });

  writeFileSync(join(judgeDir, "chart_scores.svg"), scoresSvg);
  writeFileSync(join(judgeDir, "chart_stats.svg"), statsSvg);
  console.log(`[judge] Written: ${join(judgeDir, "chart_scores.svg")}`);
  console.log(`[judge] Written: ${join(judgeDir, "chart_stats.svg")}`);
}

function cmdSummary(judgeDir: string): void {
  const bmFile = join(judgeDir, "blinding_map.json");
  const tmplFile = join(judgeDir, "judgment_template.json");
  for (const f of [bmFile, tmplFile]) {
    if (!existsSync(f)) {
      console.error(`[judge] Missing ${f}`);
      process.exit(1);
    }
  }

  const bmData = JSON.parse(readFileSync(bmFile, "utf-8")) as {
    seed: number;
    result_dirs: string[];
    map: Record<string, Record<string, { run: string; condition: string }>>;
  };
  const tmpl: Record<string, Record<string, TemplateScore>> = JSON.parse(
    readFileSync(tmplFile, "utf-8")
  );
  const blindingMap = bmData.map;

  const resultDirs = bmData.result_dirs ?? [];
  const statsByTask = loadStats(resultDirs);

  const judgment: Record<string, unknown> = {};
  const condAgg: Record<
    string,
    { score: number; max: number; count: number; in_tok: number; out_tok: number; tools: number; secs: number }
  > = {};

  const colW = { task: 6, fn: 36, run: 24, cond: 8, score: 10, wscore: 12 };
  const header =
    `${"Task".padEnd(colW.task)}  ${"Function".padEnd(colW.fn)}  ` +
    `${"Run".padEnd(colW.run)}  ${"Cond".padEnd(colW.cond)}  ` +
    `${"Score".padStart(colW.score)}  ${"Weighted".padStart(colW.wscore)}`;
  const rows: string[] = [];
  const chartRows: ChartRow[] = [];

  for (const [tid, labelsData] of Object.entries(tmpl)) {
    const task = TASK_BY_ID[tid];
    if (!task) continue;
    const maxPts = task.rubric.reduce((s, r) => s + r.points, 0);
    const dw = task.difficulty_weight;
    const mapTask = blindingMap[tid] ?? {};
    judgment[tid] = {};

    for (const [label, scoresData] of Object.entries(labelsData)) {
      const info = mapTask[label] ?? {};
      const condition = (info as { condition?: string }).condition ?? "?";
      const run = (info as { run?: string }).run ?? "?";
      const rawScores = scoresData.criteria_scores;
      let total: number | null = null;
      if (rawScores && rawScores.every((s) => s !== null)) {
        total = (rawScores as number[]).reduce((a, b) => a + b, 0);
      } else if (scoresData.total !== null) {
        total = Math.round(scoresData.total!);
      }

      (judgment[tid] as Record<string, unknown>)[label] = {
        condition,
        run,
        criteria_scores: rawScores,
        total,
        difficulty_weight: dw,
        notes: scoresData.notes,
      };

      const stat = (statsByTask[tid] ?? []).find(
        (s) => s.run === run && s.condition === condition
      );

      if (total !== null) {
        const agg = (condAgg[condition] ??= {
          score: 0,
          max: 0,
          count: 0,
          in_tok: 0,
          out_tok: 0,
          tools: 0,
          secs: 0,
        });
        agg.score += total * dw;
        agg.max += maxPts * dw;
        agg.count++;
        if (stat) {
          agg.in_tok += stat.input_tokens;
          agg.out_tok += stat.output_tokens;
          agg.tools += stat.num_tool_calls;
          agg.secs += stat.duration_sec;
        }
      }

      const scoreStr = total !== null ? `${total}/${maxPts}` : `?/${maxPts}`;
      const wscoreStr =
        total !== null
          ? `${(total * dw).toFixed(1)}/${(maxPts * dw).toFixed(1)}`
          : "?";
      rows.push(
        `${tid.padEnd(colW.task)}  ${task.function.padEnd(colW.fn)}  ` +
        `${run.padEnd(colW.run)}  ${condition.padEnd(colW.cond)}  ` +
        `${scoreStr.padStart(colW.score)}  ${wscoreStr.padStart(colW.wscore)}`
      );
      if (total !== null) {
        chartRows.push({ task: tid, condition, total, max: maxPts, pct: (total / maxPts) * 100 });
      }
    }
  }

  judgment["_meta"] = {
    seed: bmData.seed,
    result_dirs: bmData.result_dirs,
    condition_totals: condAgg,
  };
  writeFileSync(join(judgeDir, "judgment.json"), JSON.stringify(judgment, null, 2));

  const sep = "-".repeat(header.length);
  console.log(header);
  console.log(sep);
  for (const row of rows) console.log(row);
  console.log(sep);
  console.log();
  console.log("CONDITION TOTALS  (weighted score | tokens in/out | tool calls | time)");
  console.log();
  const condStats: CondStats[] = [];
  for (const [cond, data] of Object.entries(condAgg).sort()) {
    const pct = data.max ? (data.score / data.max) * 100 : 0;
    const n = data.count;
    console.log(
      `  ${cond.padEnd(10)}  score ${data.score.toFixed(1).padStart(6)}/${data.max.toFixed(1)} (${pct.toFixed(1)}%)` +
      `  [${n} resp]`
    );
    console.log(
      `             in=${data.in_tok.toLocaleString()}  out=${data.out_tok.toLocaleString()}` +
      `  tools=${data.tools}  time=${data.secs.toFixed(1)}s`
    );
    condStats.push({ condition: cond, score_pct: pct, in_tok: data.in_tok, out_tok: data.out_tok, tools: data.tools, secs: data.secs });
  }
  console.log();
  console.log(`[judge] Written: ${join(judgeDir, "judgment.json")}`);

  if (chartRows.length > 0) {
    generateCharts(judgeDir, chartRows, condStats);
  }
}

async function cmdRun(opts: {
  model: string;
  baseUrl: string;
  questionIds: string[];
  nRuns: number;
  condition: string;
  apiKey?: string;
  cofeGraphBin?: string;
  projectPath?: string;
  outDir?: string;
  seed?: number;
}): Promise<string> {
  const tsxBin = join(HERE, "node_modules", ".bin", "tsx");
  const benchTs = join(HERE, "bench.ts");
  const resultDirs: string[] = [];

  for (let i = 0; i < opts.nRuns; i++) {
    console.log(`[judge] ── run ${i + 1}/${opts.nRuns} `);
    const cmd = [
      tsxBin,
      benchTs,
      "--model",
      opts.model,
      "--base-url",
      opts.baseUrl,
      "--condition",
      opts.condition,
      "--question",
      ...opts.questionIds,
    ];
    if (opts.apiKey) cmd.push("--api-key", opts.apiKey);
    if (opts.cofeGraphBin) cmd.push("--cofe-graph-bin", opts.cofeGraphBin);
    if (opts.projectPath) cmd.push("--project-path", opts.projectPath);

    let runDir: string | undefined;
    await new Promise<void>((resolve) => {
      const proc = spawn(cmd[0], cmd.slice(1), {
        stdio: ["ignore", "pipe", "pipe"],
        cwd: HERE,
      });

      let buffer = "";
      const onData = (chunk: Buffer) => {
        const text = buffer + chunk.toString();
        const lines = text.split("\n");
        buffer = lines.pop() ?? "";
        for (const line of lines) {
          process.stdout.write(line + "\n");
          const m = line.match(/\[bench\] run dir\s*:\s*(\S+)/);
          if (m) runDir = join(HERE, m[1]);
        }
      };

      proc.stdout?.on("data", onData);
      proc.stderr?.on("data", onData);

      proc.on("close", (code: number | null) => {
        if (buffer) process.stdout.write(buffer + "\n");
        if (code !== 0) {
          console.error(
            `[judge] bench.ts exited with code ${code} — skipping run`
          );
        }
        resolve();
      });
    });

    if (runDir && existsSync(runDir)) {
      resultDirs.push(runDir);
    } else {
      console.error("[judge] Could not determine run dir from bench.ts output");
    }
  }

  if (resultDirs.length === 0) {
    console.error("[judge] No successful bench runs — aborting.");
    process.exit(1);
  }

  console.log(
    `\n[judge] Collected ${resultDirs.length} run(s): ${JSON.stringify(resultDirs.map((d) => d.split("/").pop()))}`
  );

  const rngSeed =
    opts.seed !== undefined
      ? opts.seed
      : Math.floor(Math.random() * 2 ** 32);

  const finalOut =
    opts.outDir ??
    join(
      HERE,
      "results",
      `judge_${new Date()
        .toISOString()
        .replace(/[-:]/g, "")
        .replace("T", "_")
        .slice(0, 15)}`
    );

  cmdGenerate(resultDirs, finalOut, rngSeed);
  return finalOut;
}

async function main(): Promise<void> {
  const program = new Command();
  program
    .description("Blind LLM judge for cofe-graph benchmark")
    .addHelpText(
      "after",
      `
Examples:
  # Full pipeline: run bench N times, auto-judge, summarise:
  pnpm judge --run --model gemma4:e4b --base-url http://host/v1 \\
                  --question Q1 Q2 --runs 3 \\
                  --judge-model qwen2.5-coder:32b --judge-base-url http://host/v1
  # Auto-judge an existing judge dir:
  pnpm judge --judge results/judge_01 \\
                  --judge-model qwen2.5-coder:32b --judge-base-url http://host/v1
  # Generate judge files from existing result directories:
  pnpm judge results/run1 results/run2 [--out results/judge_01]
  # Deanonymise and report after filling judgment_template.json manually:
  pnpm judge --summary results/judge_01`
    );

  program
    .option("--run", "Run bench.ts N times then generate (and optionally auto-judge) files")
    .option("--judge", "Auto-judge an existing judge dir using an LLM")
    .option("--summary", "Deanonymise scores and print per-condition report");

  program
    .argument("[paths...]", "Result directories (generate mode) or judge dir (--summary/--judge)")
    .option("--model <model>", "Model name passed to bench.ts")
    .option("--base-url <url>", "Base URL passed to bench.ts")
    .option("--api-key <key>", "API key passed to bench.ts")
    .option("--condition <cond>", "Condition(s) to run: with | without | both", "both")
    .option("--question <ids...>", "Question IDs to benchmark, e.g. --question Q1 Q2")
    .option("--runs <n>", "Number of bench.ts runs", "1")
    .option("--cofe-graph-bin <path>", "cofe-graph binary path")
    .option("--project-path <path>", "Project path for cofe-graph")
    .option("--judge-model <model>", "LLM model for auto-judging responses")
    .option("--judge-base-url <url>", "Base URL for judge LLM (defaults to --base-url)")
    .option("--judge-api-key <key>", "API key for judge LLM (defaults to --api-key)")
    .option("--out <dir>", "Output directory for judge files")
    .option("--seed <n>", "Random seed for label assignment");

  program.parse(process.argv);

  const opts = program.opts<{
    run?: boolean;
    judge?: boolean;
    summary?: boolean;
    model?: string;
    baseUrl?: string;
    apiKey?: string;
    condition: string;
    question?: string[];
    runs: string;
    cofeGraphBin?: string;
    projectPath?: string;
    judgeModel?: string;
    judgeBaseUrl?: string;
    judgeApiKey?: string;
    out?: string;
    seed?: string;
  }>();

  const paths = program.args as string[];

  if (opts.run) {
    if (!opts.model || !opts.baseUrl) {
      program.error("--run requires --model and --base-url");
    }
    if (!opts.question || opts.question.length === 0) {
      program.error("--run requires --question QN [QN ...]");
    }
    const judgeDir = await cmdRun({
      model: opts.model!,
      baseUrl: opts.baseUrl!,
      questionIds: opts.question!,
      nRuns: parseInt(opts.runs, 10),
      condition: opts.condition,
      apiKey: opts.apiKey,
      cofeGraphBin: opts.cofeGraphBin,
      projectPath: opts.projectPath,
      outDir: opts.out,
      seed: opts.seed !== undefined ? parseInt(opts.seed, 10) : undefined,
    });
    if (opts.judgeModel) {
      console.log();
      await cmdAutoJudge(
        judgeDir,
        opts.judgeModel,
        opts.judgeBaseUrl ?? opts.baseUrl ?? "",
        opts.judgeApiKey ?? opts.apiKey
      );
    }
  } else if (opts.judge) {
    if (paths.length !== 1) {
      program.error("--judge requires exactly one PATH (the judge output directory)");
    }
    if (!opts.judgeModel) {
      program.error("--judge requires --judge-model");
    }
    await cmdAutoJudge(
      resolve(paths[0]),
      opts.judgeModel!,
      opts.judgeBaseUrl ?? opts.baseUrl ?? "",
      opts.judgeApiKey ?? opts.apiKey
    );
  } else if (opts.summary) {
    if (paths.length !== 1) {
      program.error("--summary requires exactly one PATH (the judge output directory)");
    }
    cmdSummary(resolve(paths[0]));
  } else {
    if (paths.length === 0) {
      program.help();
    }
    const seed =
      opts.seed !== undefined
        ? parseInt(opts.seed, 10)
        : Math.floor(Math.random() * 2 ** 32);
    const outDir =
      opts.out ??
      join(
        "results",
        `judge_${new Date()
          .toISOString()
          .replace(/[-:]/g, "")
          .replace("T", "_")
          .slice(0, 15)}`
      );
    cmdGenerate(paths.map((p) => resolve(p)), outDir, seed);
  }
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
