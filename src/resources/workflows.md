# Common Workflows

## Understand an unfamiliar file

1. `find_functions_in_file` — list all functions defined in the file
2. `get_globals` — see file-scope variables
3. `includes direction="outbound"` — see what it depends on
4. `get_source` on functions of interest to read the implementation
5. `traverse direction="callers"` to see how key functions are triggered from outside the file
6. `annotate` the file to record findings for future sessions

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

## Annotate a codebase

1. `list_unannotated_files` — see which files have no annotation; work in dependency order (leaf files first)
2. For each file:
   - `find_functions_in_file` → read key functions with `get_source`
   - `get_globals` and `includes direction="outbound"` to understand dependencies
   - `annotate file=<path> summary=<...>` to record a file-level summary
3. Repeat until `list_unannotated_files` returns empty
