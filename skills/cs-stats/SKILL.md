---
name: cs-stats
description: Show codebase statistics from the Codescope knowledge graph. Use when user asks how many functions, classes, files, or wants a project overview.
user-invocable: true
---

# Codebase Statistics

Show the knowledge graph statistics using the `graph_stats` MCP tool.

Display a clean summary:

```
Project: <repo-name>
========================
Files:      XXX
Functions:  XXX
Classes:    XXX
Modules:    XXX
Imports:    XXX
Variables:  XXX
Relations:  XXX
========================
```

If the stats look empty (all zeros), suggest running `/cs-index` first.
