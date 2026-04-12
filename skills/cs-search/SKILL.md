---
name: cs-search
description: Search functions in the Codescope knowledge graph. Use when user wants to find functions, methods, or symbols by name pattern.
user-invocable: true
argument-hint: "<pattern>"
---

# Search Functions

Search the knowledge graph for functions matching a pattern.

Use the `search_functions` MCP tool with the pattern: **$ARGUMENTS**

If no arguments given, ask the user what to search for.

## Display Format

Show results as a table:
| Function | File | Line |
|----------|------|------|

If many results, group by file. Show total count at the end.
