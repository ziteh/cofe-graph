# Cofe Graph

A GraphRAG MCP server for C language.

## Tools

| Tool            | Description                                                       |
| --------------- | ----------------------------------------------------------------- |
| `index_project` | Parse all `.c`/`.h` files in a directory and build the call graph |
| `find_function` | Case-insensitive substring search for functions                   |
| `get_callers`   | BFS upward — who calls this function?                             |
| `get_callees`   | BFS downward — what does this function call?                      |

## Usage

todo...

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
