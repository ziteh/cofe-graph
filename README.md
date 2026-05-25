# Cofe Graph ☕

A code graph RAG MCP server for embedded C.

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
