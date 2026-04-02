---
name: cs-index
description: Re-index the current codebase with Codescope. Use when the user wants to refresh the knowledge graph after code changes.
user-invocable: true
disable-model-invocation: true
---

# Index Codebase

Re-index the current project using Codescope's `index_codebase` MCP tool.

Steps:
1. Call the `index_codebase` MCP tool
2. Then call `graph_stats` to show what was indexed
3. Report: number of files, functions, classes, relations

Show a summary like:
```
Indexed: 245 files
  Functions: 1,203
  Classes:   89
  Imports:   567
  Relations: 3,401
```
