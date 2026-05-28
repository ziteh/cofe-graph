# Common Workflows

## Understand an unfamiliar file

1. `get_file_context` — one call: functions with call stats, globals, types, includes, annotation
2. `get_source` on individual functions of interest to read the implementation
3. `traverse direction="callers"` to see how key functions are triggered from outside the file
4. `annotate_file` + `annotate_symbol` to record findings for future sessions

## Trace a bug through the call chain

1. `search` to locate the entry point or suspect function
2. `traverse direction="callers" depth=2` to see who triggers it
3. `get_path from=X to=Y` for the exact chain between two points
4. `get_source` on functions in the chain to read the implementation

## Review dead code

1. `find_dead_code` — get all classified results
2. Focus on the `suspicious` bucket
3. `get_source` on each candidate — confirm it is truly unneeded or find the implicit reference that was missed
4. `find_users kind="global"` if the function operates on a shared global that might serve as an implicit entry point

## Annotate an entire codebase

1. `list_unannotated` (no params) — files sorted by function count; work largest first
2. For each file: `get_file_context` → `annotate_file` → `list_unannotated file=<path>` → `annotate_symbol` per unannotated function
3. Repeat until `list_unannotated` returns an empty list
