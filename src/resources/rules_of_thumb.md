# Rules of Thumb

- **Search first, don't grep.** `search` is blazing fast and returns file + line. Reaching for read-file or grep before `search` wastes context for no gain.
- **Batch `traverse` calls.** Pass multiple names: `{ "names": ["a", "b", "c"], ... }` — one BFS pass per call, not one call per name.
- **Start shallow, deepen if needed.** Default `depth=1` for `traverse`. Go to `depth=2` or `3` only when the first-level results don't answer the question.
- **Use `get_file_context` before annotating.** It returns functions with call stats, globals, types, and annotations (without source). Call `get_source` separately to fetch individual function source.
- **`get_source` includes the symbol annotation.** No need to also call `get_annotations` unless you want the file-level annotation.
- **`stale: true` in annotation output** means source changed after the insight was written — treat it as potentially outdated and re-read the source before relying on it.
- **`find_dead_code` classes other than `suspicious`** are not actually dead — do not delete them based on this tool alone.
- **After any source change**, call `index_project` once before further tool calls.
