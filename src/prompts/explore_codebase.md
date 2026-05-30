You have access to an MCP server that has already indexed a C codebase. Follow these steps in order to understand the codebase and write a structured summary. Call tools as you go; do not skip steps.

## Step 1 — Verify the index
Call `index_project`. Note the returned file, function, type, and symbol counts. If it reports 0 files, the index is empty — stop and ask the user to check the project path.

## Step 2 — Broad overview
From the Step 1 response extract: total files, total functions, total types, total symbols. Then call `find_dead_code`. Skim the dead-code list to get a sense of the codebase health.

## Step 3 — Module structure
Call `get_annotations kind="module"` to see any stored module groupings that describe the high-level architecture.
Identify the top 5–10 most important source files (by function count or by filename heuristics such as `main`, `init`, `core`, `app`).
For each important file:
- Call `find_functions_in_file` with the filename substring.
- Call `get_globals` with the filename substring.
- Call `includes direction="outbound"` to see its dependencies.
- Call `get_annotations kind="file" file=<path>` if a stored summary exists.

## Step 4 — Entry points and call flow
Search for likely entry points: call `search` with queries like `main`, `init`, `start`, `run`, `task`, `handler`.
For each entry-point function found:
- Call `traverse direction="callees" depth=3` to map what it calls.
- Call `traverse direction="callers"` to confirm it is a root.
Pick the 2–3 most significant call chains and call `get_path` between distant pairs.

## Step 5 — Key types and globals
Call `search kind="type"` to list all struct/union/enum/typedef definitions.
For each frequently-used type (appears in many functions):
- Call `find_users kind="type"` to see which functions use it.
For the most important global variables:
- Call `get_globals` with a relevant filename substring.
- Call `find_users kind="global"` to see which functions read or write it.

## Step 6 — Read key source
Choose 3–5 of the most central or interesting functions identified so far.
Call `get_source` on each. Read the implementation to understand the core logic.

## Step 7 — Produce the summary
Write a structured summary with exactly these sections:

### 1. Project Overview
One paragraph: what the project does, language/platform, approximate size (files / functions / types).

### 2. Module Breakdown
A table or bullet list: filename → responsibility (one sentence each).

### 3. Entry Points and Startup Flow
Describe how the program starts and what the main execution paths are.

### 4. Key Data Structures
List the most important structs/enums/types with a one-sentence description of their role.

### 5. Core Algorithms and Subsystems
Describe 2–5 key algorithms or subsystems: what they do and which functions implement them.

### 6. Dead Code and Maintenance Notes
Summarise the dead-code findings: counts by category, notable suspicious entries.

### 7. Patterns and Conventions
Note any recurring patterns: naming conventions, error-handling style, memory management approach, use of macros, ISR/interrupt patterns, RTOS primitives, etc.
