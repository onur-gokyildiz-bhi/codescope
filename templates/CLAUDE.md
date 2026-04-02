# Codescope Integration

This project uses Codescope for code intelligence. A knowledge graph of the codebase is available via MCP tools.

## Rules

1. **Always use Codescope MCP tools first** before reading raw source files for code structure questions
2. When the user asks about functions, classes, imports, dependencies — query the graph, don't grep files
3. The `function` table in SurrealQL is a reserved word — always use backticks: `\`function\``
4. Support both Turkish and English queries naturally

## Available Commands

- `/codescope` — Main menu
- `/cs-search <pattern>` — Find functions
- `/cs-index` — Re-index project
- `/cs-stats` — Project overview
- `/cs-ask <question>` — Natural language query (TR/EN)
- `/cs-impact <function>` — Impact analysis

## Quick MCP Tool Reference

| Question | Tool |
|----------|------|
| Find functions by name | `search_functions` |
| Get function details | `find_function` |
| Who calls X? | `find_callers` |
| What does X call? | `find_callees` |
| What's in this file? | `file_entities` |
| Project overview | `graph_stats` |
| Custom SurrealQL | `raw_query` |
| What breaks if I change X? | `impact_analysis` |
| Natural language question | `ask` |
