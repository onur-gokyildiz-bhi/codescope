---
name: codescope
description: Code intelligence engine — query knowledge graphs, find functions, trace callers, analyze impact. Use when user asks about code structure, dependencies, function relationships, or wants to understand a codebase.
user-invocable: true
argument-hint: "[command] [args...]"
---

# Codescope — Code Intelligence Engine

You have access to a Codescope MCP server that indexes codebases into a SurrealDB knowledge graph. **Always prefer Codescope tools over reading raw files** when answering code structure questions — it saves 99%+ tokens.

## Available MCP Tools

| Tool | When to Use |
|------|-------------|
| `search_functions` | Find functions by name pattern |
| `find_function` | Get exact function details (signature, body, location) |
| `find_callers` | Who calls this function? |
| `find_callees` | What does this function call? |
| `file_entities` | List all symbols in a file |
| `graph_stats` | Codebase overview (counts by type) |
| `raw_query` | Custom SurrealQL queries |
| `impact_analysis` | What breaks if I change X? |
| `supported_languages` | List supported languages |
| `ask` | Natural language → graph query |
| `index_codebase` | Re-index the codebase |

## When to Auto-Use Codescope

Automatically use these tools when the user asks (in any language):
- "Bu fonksiyonu kim cagiriyor?" / "Who calls this function?" → `find_callers`
- "X fonksiyonunu bul" / "Find function X" → `find_function`
- "Auth ile ilgili fonksiyonlari goster" / "Show auth functions" → `search_functions`
- "Bu dosyada ne var?" / "What's in this file?" → `file_entities`
- "Bunu degistirsem ne etkilenir?" / "Impact of changing X?" → `impact_analysis`
- "Projede kac fonksiyon var?" / "How many functions?" → `graph_stats`
- "En buyuk fonksiyon hangisi?" / "Largest function?" → `raw_query`
- "Struct'lari listele" / "List all structs" → `raw_query`

## SurrealQL Tips (for raw_query)

- `function` is a reserved word — always use backticks: `` `function` ``
- String search: `string::contains(string::lowercase(name), string::lowercase("pattern"))`
- All structs: `SELECT * FROM class WHERE kind = "Struct"`
- Largest functions: `SELECT name, file_path, array::len(string::split(body, "\n")) AS lines FROM \`function\` ORDER BY lines DESC LIMIT 10`
- Count all: `SELECT count() FROM \`function\` GROUP ALL`

## Slash Commands

- `/cs-search <pattern>` — Search functions by name
- `/cs-index` — Re-index current project
- `/cs-stats` — Show codebase stats
- `/cs-ask <question>` — Natural language query (TR/EN)
- `/cs-impact <function>` — Impact analysis
- `/cs-callers <function>` — Who calls this function?
- `/cs-file <path>` — All entities in a file
- `/cs-query <surql>` — Raw SurrealQL query

## Arguments

If called as `/codescope`, route based on first argument:
- `/codescope search auth` → use `search_functions` with pattern "auth"
- `/codescope stats` → use `graph_stats`
- `/codescope ask "en buyuk fonksiyon"` → use `ask` tool
- `/codescope impact handleRequest` → use `impact_analysis`
