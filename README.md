# Cofe Graph ☕

A code graph RAG MCP server for embedded C. Inspired by [CodeGraph](https://github.com/colbymchenry/codegraph).

## MCP Tools

| Tool                     | Description                                                   |
| ------------------------ | ------------------------------------------------------------- |
| `index_project`          | Re-index the project directory                                |
| `search`                 | Find functions, types, and symbols by name                    |
| `traverse`               | BFS callers or callees of one or more functions               |
| `get_source`             | Get the full source code of a function                        |
| `get_path`               | Find the call path from one function to another               |
| `find_dead_code`         | List functions never called                                   |
| `find_functions_in_file` | List all functions in files                                   |
| `get_globals`            | List file-scope global variable declarations in files         |
| `find_users`             | Find all functions referencing a global variable or type name |
| `includes`               | Query `#include` relationships                                |
| `annotate`               | Write an annotation for a file or symbol                      |
| `annotate_module`        | Define a logical module grouping across files                 |
| `get_annotations`        | Read annotations for a module, file, or symbol                |
| `list_unannotated`       | List files, functions, or globals that have no annotation yet |

## Usage

`.mcp.json`

```json
{
  "mcpServers": {
    "cofe-graph": {
      "type": "stdio",
      "command": "/path/to/cofe-graph",
      "args": ["/path/to/codebase", "--toon"]
    }
  }
}
```

### CLI

| Flag                 | Description                                                      |
| -------------------- | ---------------------------------------------------------------- |
| `--toon`             | Responses with [TOON](https://toonformat.dev/) instead of JSON   |
| `--quiet`            | Suppress log output to stderr                                    |
| `--max-l1-cache <N>` | Maximum number of full-graph snapshots to keep in the L1 cache   |
| `--max-l2-cache <N>` | Maximum number of per-file graph entries to keep in the L2 cache |

## Development

Build:

```bash
cargo build --release
```

Manual testing with MCP Inspector:

```bash
npx @modelcontextprotocol/inspector cargo run
```

## Benchmark

Measures response quality with vs. without cofe-graph.

Prerequisites:

- Node.js and pnpm
- cofe-graph binary

### Setup

```bash
cd benchmark && pnpm install
```

### Run

```bash
# Full pipeline — run bench N times, auto-judge, summarise
pnpm judge --run \
  --model gemma4:e4b --base-url http://localhost:11434/v1 \
  --judge-model gemma4:26b --judge-base-url http://localhost:11434/v1 \
  --question Q1 Q2 --runs 2

# Bench only (no judging)
pnpm bench --model <model> --base-url <url>
```

For OpenRouter, set `OPENROUTER_API_KEY` in `benchmark/.env` (copy from `benchmark/.env.example`).

Results are saved to `benchmark/results/<timestamp>/`.
