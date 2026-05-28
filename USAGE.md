# Cofe Graph

cofe-graph is a RAG MCP server that builds a **call graph + symbol index** for C codebases. It answers structural questions (who calls what, where is X defined, what would break) without grepping source files.

Detailed usage guidance is available as MCP resources at:

- `cofe://quick-reference`: question-to-tool lookup table
- `cofe://rules-of-thumb`: best practices for tool usage
- `cofe://workflows`: step-by-step guides for common tasks

The project is indexed automatically at startup. Use the `index_project` tool to re-index after source changes.
