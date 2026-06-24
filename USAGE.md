# Cofe Graph — Agent Usage

cofe-graph is a MCP server for C codebases. It answers structural questions (who calls what, where is X defined) without grepping source files. The index is built at startup and refreshed automatically on file changes.

## Tools

### `symbol_lookup`

List all symbols in a file. Use this to orient yourself in an unfamiliar file before reading source.

```json
{ "file": "drivers/uart.c", "kind": "function" }
```

`kind` is optional. Without it, all symbol kinds are returned. Valid values:

| kind | covers |
| --- | --- |
| `function` | function definitions |
| `variable` | file-scope variable declarations |
| `typedef` | `typedef T name` |
| `struct` | named struct definitions |
| `union` | named union definitions |
| `enum` | named enum definitions |
| `macro` | `#define` constants and function-like macros |
| `enum_value` | enum member constants |

`file` is a substring match against the full path — `"uart"` matches `drivers/uart.c` and `drivers/uart.h`.

Each result includes `name`, `kind`, `line`, and `signature`.

### `get_source`

Return the complete source of a symbol by exact name.

```json
{ "name": "uart_init" }
{ "name": "uart_init", "file": "drivers/uart.c" }
```

`file` is only needed when the same name appears in multiple files. Without it, if there are multiple matches the response is:

```json
{ "ambiguous": true, "matches": [ { "name": "...", "kind": "...", "file": "...", "signature": "..." } ] }
```

Call again with the `file` field set to one of the returned paths to get the source.

A successful response is plain text:

```text
// file: drivers/uart.c
// line: 42

void uart_init(uint32_t baud) { ... }
```

### `query_call_graph`

Walk the call graph from a function.

```json
{ "name": "uart_init", "direction": "callers" }
{ "name": "uart_init", "direction": "callees", "depth": 2 }
{ "name": "main", "file": "app/main.c", "direction": "both" }
```

`direction` is required: `callers`, `callees`, or `both`. `depth` defaults to 1.

At `depth=1`, each entry includes `name`, `file`, and `line` (call-site line number). At `depth>1`, only `name` and `file` are returned.

If the same function name exists in multiple files (e.g. `main` in both `app/` and `bootloader/`), the response without a `file` hint is:

```json
{ "ambiguous": true, "matches": [ { "name": "main", "file": "app/main.c", "signature": "..." }, ... ] }
```

Provide `file` to resolve. Note: call edges are indexed by function name, so `callers`/`callees` lists reflect edges from all same-name definitions combined.

## When to use each tool

| Question | Tool |
| --- | --- |
| What symbols are in `uart.c`? | `symbol_lookup(file="uart.c")` |
| Where is `uart_init` defined? What does it look like? | `get_source(name="uart_init")` |
| Who calls `uart_init`? | `query_call_graph(name="uart_init", direction="callers")` |
| What does `uart_init` call? | `query_call_graph(name="uart_init", direction="callees")` |
| Trace the call chain two levels deep | `query_call_graph(..., depth=2)` |
| Find all macros in a header | `symbol_lookup(file="config.h", kind="macro")` |

## Rules of thumb

- **Start with `symbol_lookup`** when exploring an unfamiliar file. It gives you line numbers and signatures without reading the whole file.
- **Trust the results.** They come from a full AST parse — do not re-verify with grep.
- **Use `file` to disambiguate**, not to search. `file` is a substring filter; if you know the exact path, pass a unique fragment like `"app/main"` rather than just `"main"`.
- **`get_source` before reading the file.** For a single function, `get_source` returns exactly the function body without loading the whole file into context.
- **`depth=1` is almost always enough** for understanding direct dependencies. Use `depth=2` only when you need to see one more level of the chain.
