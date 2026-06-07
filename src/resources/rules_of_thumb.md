# Rules of Thumb

- **Search first, don't grep.** `search` is blazing fast and returns file + line. Reaching for read-file or grep before `search` wastes context for no gain.
- **Batch `traverse` calls.** Pass multiple names: `{ "names": ["a", "b", "c"], ... }` — one BFS pass per call, not one call per name.
- **Start shallow, deepen if needed.** Default `depth=1` for `traverse`. Go to `depth=2` or `3` only when the first-level results don't answer the question.
- **`annotate` is file-level only.** Keyed by content hash — the annotation becomes invisible once the file changes. Use `get_annotation` to retrieve it.
- **`stale: true` in annotation output** means the file changed after the insight was written — treat it as potentially outdated and re-read the source before relying on it.
- **`find_dead_code` classes other than `suspicious`** are not actually dead — do not delete them based on this tool alone.
