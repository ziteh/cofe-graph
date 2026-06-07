# Cofe Graph

cofe-graph is a RAG MCP server that builds a **call graph + symbol index** for C codebases. It answers structural questions (who calls what, where is X defined, what would break) without grepping source files.

Detailed usage guidance is available as MCP resources at:

- `graph://quick-reference`: question-to-tool lookup table
- `graph://rules-of-thumb`: best practices for tool usage
- `graph://workflows`: step-by-step guides for common tasks

The project is indexed automatically at startup and re-indexed whenever source files change. Use `index_project` to force a re-index or to view indexing statistics.
