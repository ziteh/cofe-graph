You have access to an MCP server that has indexed a C codebase. Your goal is twofold: **understand the codebase** (architecture, responsibilities, design) and then **annotate every source file** to preserve that understanding. Work through the steps below in order; do not skip steps.

---

## Phase 1 — Orient

### Step 1 — Verify the index

Call `index_project`. Note `files_ok` and function count. If `files_ok` is 0, stop and ask the user to check the project path.

### Step 2 — Top-level shape

Call `find_dead_code` to get a sense of codebase health. Then call `search name="" kind="type"` to see all struct/enum/typedef definitions — this gives an early picture of the domain model.

### Step 3 — Entry points and call flow

Call `search` with queries like `main`, `init`, `start`, `run`, `task`, `handler` to locate entry points.

For each entry point found:

- Call `traverse direction="callees" depth=3` to map what it drives.
- Call `traverse direction="callers"` to confirm it is a root (nothing calls it).

Pick the 2–3 most significant call chains and call `get_path` between distant pairs to trace the critical paths.

### Step 4 — File-by-file deep dive

Work through every source file returned by `list_unannotated_files`. For each file:

- Call `find_functions_in_file` — list all functions.
- Call `get_globals` — see file-scope variables.
- Call `includes direction="outbound"` — see its dependencies.
- Call `get_source` for each function in the file.
- Call `traverse direction="callers"` on the key functions to understand how they fit into the larger system.

Synthesise what you have learned into a one-paragraph file summary that captures: **what this file is responsible for**, **how it fits into the overall design**, and **any non-obvious constraints or patterns**.

### Step 5 — Cross-cutting types and globals

For the most important types identified in Step 2:
- Call `find_users kind="type"` to see which files and functions depend on them.

For the most important global variables:
- Call `find_users kind="global"` to see which functions read or write them.

---

## Phase 2 — Annotate

After completing Phase 1, annotate every file. Process the files in dependency order where possible (leaf files — those with few inbound `#includes` — before files that depend on them).

For each file:
1. Check `get_annotation file=<path>` — if a non-null annotation already exists and the file has not changed, skip it.
2. Otherwise call `annotate file=<path> summary=<your one-paragraph summary>`.

Continue until `list_unannotated_files` returns an empty list.

---

## Phase 3 — Summary

Provide a brief summary and explain your understanding.
