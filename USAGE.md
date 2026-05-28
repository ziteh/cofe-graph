# Using cofe-graph

cofe-graph is a RAG MCP server that builds a **call graph + symbol index** for C codebases. It answers structural questions (who calls what, where is X defined, what would break) without grepping source files.

The codebase is indexed automatically at startup.

## Quick Reference

| Question                                             | Tool                              |
| ---------------------------------------------------- | --------------------------------- |
| Where is function / type / macro X defined?          | `search`                          |
| Who calls function X?                                | `traverse` `direction="callers"`  |
| What does function X call?                           | `traverse` `direction="callees"`  |
| Show the call path from X to Y                       | `get_path`                        |
| Show me the source of function X                     | `get_source`                      |
| What functions are defined in `foo.c`?               | `find_functions_in_file`          |
| What global variables does `foo.c` declare?          | `get_globals`                     |
| Which functions reference global variable X?         | `find_users` `kind="global"`      |
| Which functions use struct / type X?                 | `find_users` `kind="type"`        |
| What does `foo.c` `#include`?                        | `includes` `direction="outbound"` |
| Which files include `bar.h`?                         | `includes` `direction="inbound"`  |
| Find functions that are never called                 | `find_dead_code`                  |
| Give me the full analysis bundle for `foo.c`         | `get_file_context`                |
| Source files changed — rebuild the index             | `index_project`                   |
| Read stored annotations for `foo.c`                  | `get_annotations`                 |
| Write a semantic annotation for a file               | `annotate_file`                   |
| Write a semantic annotation for a function or symbol | `annotate_symbol`                 |
| What hasn't been annotated yet?                      | `list_unannotated`                |

## Rules of Thumb

- **Search first, don't grep.** `search` is blazing fast and returns file + line. Reaching for read-file or grep before `search` wastes context for no gain.
- **Batch `traverse` calls.** Pass multiple names: `{ "names": ["a", "b", "c"], ... }` — one BFS pass per call, not one call per name.
- **Start shallow, deepen if needed.** Default `depth=1` for `traverse`. Go to `depth=2` or `3` only when the first-level results don't answer the question.
- **Use `get_file_context` before annotating.** It replaces a chain of `get_source` + `get_globals` + `get_annotations` with a single call.
- **`get_source` includes the symbol annotation.** No need to also call `get_annotations` unless you want the file-level annotation.
- **`stale: true` in annotation output** means source changed after the insight was written — treat it as potentially outdated and re-read the source before relying on it.
- **`find_dead_code` classes other than `suspicious`** are not actually dead — do not delete them based on this tool alone.
- **After any source change**, call `index_project` once before further tool calls.

## Common Workflows

### Understand an unfamiliar file

1. `get_file_context` — one call: functions with call stats, globals, types, includes, annotation
2. `get_source` on individual functions of interest
3. `traverse direction="callers"` to see how key functions are triggered from outside the file
4. `annotate_file` + `annotate_symbol` to record findings for future sessions

### Trace a bug through the call chain

1. `search` to locate the entry point or suspect function
2. `traverse direction="callers" depth=2` to see who triggers it
3. `get_path from=X to=Y` for the exact chain between two points
4. `get_source` on functions in the chain to read the implementation

### Review dead code

1. `find_dead_code` — get all classified results
2. Focus on the `suspicious` bucket
3. `get_source` on each candidate — confirm it is truly unneeded or find the implicit reference that was missed
4. `find_users kind="global"` if the function operates on a shared global that might serve as an implicit entry point

### Annotate an entire codebase

1. `list_unannotated` (no params) — files sorted by function count; work largest first
2. For each file: `get_file_context` → `annotate_file` → `list_unannotated file=<path>` → `annotate_symbol` per unannotated function
3. Repeat until `list_unannotated` returns an empty list
