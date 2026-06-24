# Cofe Graph ☕

A code graph RAG MCP server for embedded C. Inspired by [CodeGraph](https://github.com/colbymchenry/codegraph).

## MCP Tools

- **`symbol_lookup`** — list all symbols in a file
- **`get_source`** — return the full source of a symbol by name
- **`query_call_graph`** — walk callers or callees of a function

See [USAGE.md](USAGE.md) for agent usage guidance.

## Usage

`.mcp.json`

```json
{
  "mcpServers": {
    "cofe-graph": {
      "type": "stdio",
      "command": "/path/to/cofe-graph",
      "args": ["/path/to/codebase", "mcp", "--toon"]
    }
  }
}
```

### CLI

| Flag                   | Description                                                                                      |
| ---------------------- | ------------------------------------------------------------------------------------------------ |
| `--toon`               | Encode responses with [TOON](https://toonformat.dev/) instead of JSON                            |
| `--quiet`              | Suppress log output to stderr                                                                    |
| `--cache-overhead <N>` | Extra cache slots as a percentage of the file count. E.g. `100` keeps two full branch snapshots. |

## Development

Build:

```bash
cargo build --release
```

Manual testing with MCP Inspector:

```bash
npx @modelcontextprotocol/inspector cargo run -- /path/to/codebase
```

## Benchmark

Measures response quality with vs. without cofe-graph.

Prerequisites:

- Python 3.11+ and [uv](https://docs.astral.sh/uv/)
- cofe-graph binary

### Setup

```bash
cd benchmark && uv sync
```

### Run

```bash
# Full pipeline — run bench N times, auto-judge, summarise
uv run judge.py --run \
  --model gemma4:e4b --base-url http://localhost:11434/v1 \
  --judge-model gemma4:26b --judge-base-url http://localhost:11434/v1 \
  --question Q1 Q2 --runs 2

# Bench only (no judging)
uv run bench.py --model <model> --base-url <url>
```

For OpenRouter, set `OPENROUTER_API_KEY` in `benchmark/.env` (copy from `benchmark/.env.example`).

Results are saved to `benchmark/results/<timestamp>/`.
