---
name: cs-file
description: List all entities (functions, classes, imports, configs) in a specific file. Use when user asks what's in a file, wants a file overview, or asks about file contents.
user-invocable: true
argument-hint: "<file-path>"
---

# File Entities

Show all entities in a file using the `file_entities` MCP tool.

File path: **$ARGUMENTS**

If no arguments given, ask which file to analyze.

## Display Format

Group by type:

### Functions
- `function_name` (line X-Y)

### Classes / Structs
- `ClassName` (line X-Y)

### Imports
- `import_name` (line X)

Show total count at the end: "12 entities in src/main.rs"
