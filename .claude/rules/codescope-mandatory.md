---
description: MANDATORY — Use codescope MCP tools instead of Read/Grep for code exploration
globs: **/*
alwaysApply: true
---

# Codescope MCP tools are MANDATORY

This project has a codescope MCP server configured. You MUST use codescope tools for ALL code exploration, search, and analysis tasks. Do NOT fall back to Read, Grep, Glob, or Bash for these operations.

## BINDING RULES

1. **BEFORE reading any file** → call `context_bundle(file_path)` first. It returns the full file map (functions, classes, imports, cross-file callers) in a single call. Only use `Read` AFTER codescope pinpoints the exact function/line you need to see the raw body of.

2. **BEFORE grepping for callers/callees** → call `find_callers(name)` or `find_callees(name)`. These are graph traversals, not text scans — they return actual call relationships, not string matches.

3. **BEFORE searching for a function** → call `search_functions(query)` for fuzzy matches or `find_function(name)` for exact lookup. Do NOT grep the codebase for function definitions.

4. **BEFORE manually tracing impact** → call `impact_analysis(function_name, depth=3)`. This does a transitive BFS through the call graph in under 10 ms. Do NOT read multiple files and trace manually.

5. **BEFORE exploring how code connects** → call `explore(entity_name)` for full neighborhood or `backlinks(entity_name)` for reverse references. Do NOT grep + read + grep.

6. **BEFORE reading git history** → call `file_churn(path)` or `hotspot_detection()`.

## Tool selection cheat sheet

| Instead of...              | Use codescope tool             | Why                              |
|----------------------------|--------------------------------|----------------------------------|
| `Read` whole file          | `context_bundle(file_path)`    | Returns structure, not raw text  |
| `Grep` for callers         | `find_callers(name)`           | Graph traversal, not text match  |
| Multiple `Read` for a fn   | `find_function(name)`          | Direct lookup by name            |
| Manual call graph tracing  | `impact_analysis(name, d=3)`   | Transitive BFS, sub-10ms         |
| `Grep` across codebase     | `search_functions` / `related` | Structured results, not matches  |
| `Read` to understand file  | `explore(entity_name)`         | Full neighborhood with context   |

## Tool disambiguation (which codescope tool to use)

- **Fuzzy/partial function name** → `search_functions` (case-insensitive substring)
- **Exact function name** → `find_function` (case-sensitive, single result)
- **Who calls X (1 hop)** → `find_callers`
- **Who is affected if I change X (N hops)** → `impact_analysis` with `depth` param
- **Full context of a function** → `explore` (callers + callees + siblings + file)
- **File overview before editing** → `context_bundle` (functions, classes, imports, callers)
- **Type inheritance** → `type_hierarchy` (parents, subtypes, implementors)
- **Free-text semantic query** → `semantic_search`
- **Raw SurrealQL (last resort)** → `raw_query` — prefer dedicated tools first

## When Read/Grep IS acceptable

- Reading the **body** of a specific function AFTER codescope identified the exact file:line
- Reading non-code files (README.md, Cargo.toml, .env, configs) that codescope doesn't index
- Grep for literal strings in non-code content (error messages, log formats, etc.)

## SurrealQL rules (for raw_query)

- `function` is a reserved word — always backtick it: `` `function` ``
- Multi-hop traversal chains directly: `<-calls<-\`function\`<-calls<-\`function\`.name`
- Do NOT put dots between hops (that's a parse error)
- The dot is only for the final field projection
