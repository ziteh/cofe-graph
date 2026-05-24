#!/usr/bin/env tsx
/**
 * bench.ts — RAG vs no-RAG benchmark for cofe-graph MCP server
 *
 * Measures AI agent response quality on Flipper Zero firmware C code understanding,
 * comparing results with and without cofe-graph as a RAG tool via MCP.
 *
 * Usage:
 *   npx tsx bench.ts --model <model> --base-url <url> [options]
 *
 * Examples:
 *   # All questions, both conditions, via local Ollama:
 *   npx tsx bench.ts --model qwen2.5-coder:7b --base-url http://localhost:11434/v1
 *
 *   # Only Q2, without-MCP only:
 *   npx tsx bench.ts --model qwen2.5-coder:7b --base-url http://localhost:11434/v1 \
 *       --condition without --question Q2
 *
 *   # Via OpenRouter:
 *   npx tsx bench.ts --model openai/gpt-4o --base-url https://openrouter.ai/api/v1 \
 *       --api-key $OPENROUTER_API_KEY
 */

import "dotenv/config";
import {
  cpSync,
  existsSync,
  mkdirSync,
  mkdtempSync,
  readdirSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { readFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, relative, resolve } from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { performance } from "node:perf_hooks";
import { Command } from "commander";

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);

const SCRIPT_DIR = resolve(__dirname);
const REPO_ROOT = resolve(SCRIPT_DIR, "..");

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

interface ToolCallLog {
  tool: string;
  args: unknown;
  result_preview: string;
}

interface BenchRecord {
  timestamp: string;
  task_id: string;
  function_name: string;
  condition: string;
  model: string;
  base_url: string;
  question: string;
  response: string;
  tool_calls_log: ToolCallLog[];
  num_tool_calls: number;
  input_tokens: number;
  output_tokens: number;
  duration_sec: number;
}

const TEST_CASES: TestCase[] = JSON.parse(
  readFileSync(join(SCRIPT_DIR, "test_cases.json"), "utf-8")
);

const OC_BUILTIN_TOOLS = new Set([
  "bash",
  "edit",
  "write",
  "read",
  "list_directory",
  "grep",
  "glob",
  "webfetch",
]);

function findCofeBin(override?: string): string {
  if (override) {
    if (!existsSync(override)) {
      throw new Error(`cofe-graph binary not found: ${override}`);
    }
    return override;
  }
  const exe = process.platform === "win32" ? "cofe-graph.exe" : "cofe-graph";
  for (const candidate of [
    join(REPO_ROOT, "target", "release", exe),
    join(REPO_ROOT, "target", "debug", exe),
  ]) {
    if (existsSync(candidate)) return candidate;
  }
  throw new Error(
    "cofe-graph binary not found. Build it first:\n  cargo build --release"
  );
}

function flushCondition(
  runDir: string,
  condition: string,
  records: BenchRecord[]
): void {
  const out = join(runDir, `${condition}.json`);
  writeFileSync(out, JSON.stringify(records, null, 2) + "\n", "utf-8");
  console.log(`  → ${relative(SCRIPT_DIR, out)}`);
}

function runOpencode(
  question: string,
  model: string,
  baseUrl: string,
  apiKey: string,
  projectPath: string,
  withMcp: boolean,
  cofeBin?: string
): [string, ToolCallLog[], { input_tokens: number; output_tokens: number }] {
  let tmpDir: string | undefined;
  try {
    let actualDir: string;
    if (!withMcp) {
      tmpDir = mkdtempSync(join(tmpdir(), "bench_without_"));
      actualDir = tmpDir;
      cpSync(projectPath, tmpDir, {
        recursive: true,
        filter: (src) => !src.includes(".cofe-graph"),
      });
    } else {
      actualDir = projectPath;
    }

    const config: Record<string, unknown> = {
      provider: {
        bench: {
          npm: "@ai-sdk/openai-compatible",
          name: "bench",
          options: { baseURL: baseUrl, apiKey },
          models: { [model]: {} },
        },
      },
    };

    if (withMcp && cofeBin) {
      config["mcp"] = {
        cg: {
          type: "local",
          command: [cofeBin, actualDir, "--quiet"],
          enabled: true,
        },
      };
      config["permission"] = {
        read: "deny",
        edit: "deny",
        write: "deny",
        glob: "deny",
        grep: "deny",
        bash: "deny",
        list: "deny",
        webfetch: "deny",
      };
    }

    const context = withMcp
      ? "You are analyzing a C firmware codebase.\n" +
      "Use the cg_* graph-RAG tools to look up functions and source code.\n" +
      "To retrieve a function's source: call cg_get_source(name).\n" +
      "Do not rely on prior knowledge about this specific project.\n\n"
      : "You are analyzing a C firmware codebase at the current directory.\n" +
      "ALWAYS use tools to read actual source files before answering.\n" +
      "Do not rely on prior knowledge about this specific project.\n" +
      'When searching for C source files, use include="**/*.c" (double asterisk, for recursive search).\n\n';

    const prompt = context + question;

    const env = { ...process.env, OPENCODE_CONFIG_CONTENT: JSON.stringify(config) };
    const cmd = [
      "opencode",
      "run",
      "--model",
      `bench/${model}`,
      "--format",
      "json",
      "--dir",
      actualDir,
      prompt,
    ];

    const result = spawnSync(cmd[0], cmd.slice(1), {
      encoding: "utf-8",
      timeout: 300_000,
      env,
    });

    const textParts: string[] = [];
    const toolCallsLog: ToolCallLog[] = [];
    let totalIn = 0;
    let totalOut = 0;

    for (const line of (result.stdout ?? "").split("\n")) {
      const trimmed = line.trim();
      if (!trimmed) continue;
      let event: Record<string, unknown>;
      try {
        event = JSON.parse(trimmed);
      } catch {
        continue;
      }

      const etype = event["type"] as string | undefined;
      const part = (event["part"] ?? {}) as Record<string, unknown>;

      if (etype === "text") {
        textParts.push((part["text"] as string) ?? "");
      } else if (etype === "tool_use") {
        const state = (part["state"] ?? {}) as Record<string, unknown>;
        const toolName = (part["tool"] as string) ?? "";
        const toolArgs = (state["input"] ?? {}) as Record<string, unknown>;
        const tag = OC_BUILTIN_TOOLS.has(toolName) ? "fs " : "mcp";
        console.log(
          `    [${tag}] ${toolName}(${JSON.stringify(toolArgs).slice(0, 100)})`
        );
        toolCallsLog.push({
          tool: toolName,
          args: toolArgs,
          result_preview: String(state["output"] ?? "").slice(0, 300),
        });
      } else if (etype === "step_finish") {
        const tokens = (part["tokens"] ?? {}) as Record<string, unknown>;
        totalIn += (tokens["input"] as number) ?? 0;
        totalOut += (tokens["output"] as number) ?? 0;
      }
    }

    const response = textParts.join("").trim();
    return [response, toolCallsLog, { input_tokens: totalIn, output_tokens: totalOut }];
  } finally {
    if (tmpDir) {
      rmSync(tmpDir, { recursive: true, force: true });
    }
  }
}

function main(): void {
  const program = new Command();
  program
    .description(
      "Benchmark AI agent with/without cofe-graph MCP RAG on Flipper Zero firmware C code understanding."
    )
    .requiredOption("--model <model>", "Model name (e.g. qwen2.5-coder:7b, openai/gpt-4o)")
    .requiredOption(
      "--base-url <url>",
      "OpenAI-compatible base URL (e.g. http://localhost:11434/v1)"
    )
    .option("--api-key <key>", "API key — defaults to OPENROUTER_API_KEY env var, then 'ollama'")
    .option(
      "--condition <cond>",
      "Which condition to run: with | without | both",
      "both"
    )
    .option("--question <ids...>", "Run only specific question IDs, e.g. --question Q1 Q3")
    .option(
      "--cofe-graph-bin <path>",
      "Path to cofe-graph binary (default: auto-detect release then debug)"
    )
    .option(
      "--project-path <path>",
      "Project path for cofe-graph indexing (default: tests/flipperzero-firmware-dev/applications)"
    )
    .parse(process.argv);

  const opts = program.opts<{
    model: string;
    baseUrl: string;
    apiKey?: string;
    condition: string;
    question?: string[];
    cofeGraphBin?: string;
    projectPath?: string;
  }>();

  const apiKey = opts.apiKey ?? process.env["OPENROUTER_API_KEY"] ?? "ollama";

  const projectPath = opts.projectPath
    ? resolve(opts.projectPath)
    : join(REPO_ROOT, "tests", "flipperzero-firmware-dev", "applications");

  if (!existsSync(projectPath)) {
    console.error(`[error] project-path not found: ${projectPath}`);
    process.exit(1);
  }

  const ts = new Date()
    .toISOString()
    .replace(/[-:]/g, "")
    .replace("T", "_")
    .slice(0, 15);
  const runDir = join(SCRIPT_DIR, "results", ts);
  mkdirSync(runDir, { recursive: true });

  const accumulated: Record<string, BenchRecord[]> = { with: [], without: [] };

  let tasks = TEST_CASES;
  if (opts.question) {
    const ids = new Set(opts.question.map((q) => q.toUpperCase()));
    tasks = TEST_CASES.filter((t) => ids.has(t.id));
    if (tasks.length === 0) {
      const available = TEST_CASES.map((t) => t.id);
      console.error(
        `[error] No matching IDs in ${JSON.stringify([...ids])}. Available: ${JSON.stringify(available)}`
      );
      process.exit(1);
    }
  }

  if (!["with", "without", "both"].includes(opts.condition)) {
    console.error(`[error] --condition must be with, without, or both`);
    process.exit(1);
  }

  const conditions: string[] =
    opts.condition === "both" ? ["with", "without"] : [opts.condition];

  let cofeBin: string | undefined;
  if (conditions.includes("with")) {
    try {
      cofeBin = findCofeBin(opts.cofeGraphBin);
      console.log(`[bench] cofe-graph binary : ${cofeBin}`);
    } catch (err) {
      console.error(`[error] ${(err as Error).message}`);
      process.exit(1);
    }
  }

  console.log(`[bench] model       : ${opts.model}`);
  console.log(`[bench] base_url    : ${opts.baseUrl}`);
  console.log(`[bench] conditions  : ${JSON.stringify(conditions)}`);
  console.log(`[bench] tasks       : ${JSON.stringify(tasks.map((t) => t.id))}`);
  console.log(`[bench] run dir     : ${relative(SCRIPT_DIR, runDir)}\n`);

  for (const task of tasks) {
    const question = task.question;
    for (const condition of conditions) {
      console.log(`── [${task.id}] condition=${condition}  fn=${task.function}`);
      const t0 = performance.now();

      const [response, tcLog, usage] = runOpencode(
        question,
        opts.model,
        opts.baseUrl,
        apiKey,
        projectPath,
        condition === "with",
        condition === "with" ? cofeBin : undefined
      );

      const duration = (performance.now() - t0) / 1000;
      console.log(
        `  done in ${duration.toFixed(1)}s  |  tool_calls=${tcLog.length}` +
        `  |  in=${usage.input_tokens} out=${usage.output_tokens}`
      );

      const record: BenchRecord = {
        timestamp: new Date().toISOString(),
        task_id: task.id,
        function_name: task.function,
        condition,
        model: opts.model,
        base_url: opts.baseUrl,
        question,
        response,
        tool_calls_log: tcLog,
        num_tool_calls: tcLog.length,
        input_tokens: usage.input_tokens,
        output_tokens: usage.output_tokens,
        duration_sec: Math.round(duration * 100) / 100,
      };
      accumulated[condition].push(record);
      flushCondition(runDir, condition, accumulated[condition]);
      console.log();
    }
  }
}

main();
