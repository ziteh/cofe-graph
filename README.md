# Cofe Graph

A GraphRAG MCP server for C language.

## Tools

| Tool                     | Description                                                              |
| ------------------------ | ------------------------------------------------------------------------ |
| `index_project`          | Parse all `.c`/`.h` files in a directory and build the call graph        |
| `find_function`          | Case-insensitive substring search for functions                          |
| `get_callers`            | BFS upward — who calls this function?                                    |
| `get_callees`            | BFS downward — what does this function call?                             |
| `get_source`             | Return the raw source code of a function by exact name                   |
| `get_path`               | Find the shortest call path between two functions                        |
| `find_dead_code`         | List functions that are never called (potential dead code)               |
| `get_stats`              | Overall graph statistics: function count, edge count, top fan-in/fan-out |
| `find_functions_in_file` | List all functions defined in files matching a filename substring        |
| `find_high_fan_in`       | Rank functions by number of callers — shared utilities and hotspots      |

## Usage

`.mcp.json`

```json
{
  "mcpServers": {
    "cofe-graph": {
      "type": "stdio",
      "command": "/path/to/cofe-graph"
    }
  }
}
```

## Development

### Build

```bash
cargo build
```

### Manual testing with MCP Inspector

```bash
npx @modelcontextprotocol/inspector cargo run
```

Opens a browser UI where you can call tools interactively.

### Automated integration tests

```bash
cargo test --test e2e
```

## Benchmark

Measures response quality with vs. without cofe-graph as a RAG tool.

Prerequisites:

- Python 3.10+
- Docker
- cofe-graph binary: `cargo build --release`

### Run

```bash
# Full pipeline — auto-creates .venv and installs deps on first run
python benchmark/judge.py --run \
  --model gemma4:e4b --base-url http://localhost:11434/v1 \
  --judge-model gemma4:26b --judge-base-url http://localhost:11434/v1 \
  --question Q1 Q15 --runs 2

# Bench only (no judging)
python benchmark/bench.py --model <model> --base-url <url>
```

For OpenRouter, set `OPENROUTER_API_KEY` in `benchmark/.env` (copy from `benchmark/.env.example`).

Results are saved to `benchmark/results/<timestamp>/`.
