# Common Workflows

## Understand an unfamiliar file

1. `find_functions_in_file` — list all functions defined in the file
2. `get_globals` — see file-scope variables
3. `includes direction="outbound"` — see what it depends on
4. `get_source` on functions of interest to read the implementation
5. `traverse direction="callers"` to see how key functions are triggered from outside the file
6. `annotate` the file and key functions to record findings for future sessions

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

1. `list_unannotated kind="file"` — files with no annotation; work largest (by function count) first
2. For each file:
   - `find_functions_in_file` → read key functions with `get_source`
   - `annotate file=<path> summary=<...>` to record a file-level summary
   - `list_unannotated kind="function" filename_filter=<basename>` to find unannotated functions in that file
   - `annotate file=<path> summary=<...> symbol=<name>` for each
3. `annotate_module` to define logical groupings across files once individual files are covered
4. Repeat until `list_unannotated` returns empty

## Orient with module-level context

1. `get_annotations kind="module"` — read all module groupings to understand the high-level architecture
2. For a specific module, use the `files` list to drive further `find_functions_in_file` and `get_source` calls
3. `get_annotations kind="file" file=<path>` for any file's stored summary
