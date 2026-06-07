# Rules of Thumb

- **Search first, don't grep.** `search` is blazing fast and returns file + line. Reaching for read-file or grep before `search` wastes context for no gain.
- **Batch `traverse` calls.** Pass multiple names: `{ "names": ["a", "b", "c"], ... }` — one BFS pass per call, not one call per name.
- **Start shallow, deepen if needed.** Default `depth=1` for `traverse`. Go to `depth=2` or `3` only when the first-level results don't answer the question.
- **`get_source` includes any symbol annotation.** No need to also call `get_annotations` unless you want the file-level annotation.
- **`stale: true` in annotation output** means the file changed after the insight was written — treat it as potentially outdated and re-read the source before relying on it.
- **`annotate` covers both files and symbols.** Omit `symbol` to annotate the file itself; include `symbol` to annotate a specific function, global, or macro. Both are keyed by git blob SHA and automatically disappear when the file is modified on a different commit.
- **`get_annotations kind="module"`** lists all logical groupings at once — use this for an overview before diving into individual files.
- **`find_dead_code` classes other than `suspicious`** are not actually dead — do not delete them based on this tool alone.
