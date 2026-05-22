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
