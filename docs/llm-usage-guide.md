# Codescope LLM Usage Guide

Codescope is an MCP (Model Context Protocol) server that gives LLMs structured access to a code knowledge graph. Instead of reading raw files, you query a graph database that knows about functions, classes, imports, call chains, type hierarchies, and knowledge entities.

## Quick Start

When codescope MCP is available in your session, **always prefer codescope tools over Read/Grep**. They return structured context with cross-file connections in a single call.

## Tool Reference

### Code Exploration

| Tool | When to Use | Example |
|------|-------------|---------|
| `context_bundle(file_path)` | Before reading any file | Understand a file's role: functions, callers, imports, decisions |
| `explore(entity_name)` | Understand how code connects | Full neighborhood: callers + callees + siblings + file context |
| `find_function(name)` | Know the exact function name | Get signature, file path, line numbers |
| `search_functions(query)` | Know roughly what it's called | Fuzzy substring search, ranked by graph importance (caller count) |
| `file_entities(file_path)` | List what's in a file | All functions and classes with line ranges |

### Call Graph Analysis

| Tool | When to Use | Example |
|------|-------------|---------|
| `find_callers(name)` | Who calls this? (1 hop) | Immediate callers with file paths |
| `find_callees(name)` | What does this call? (1 hop) | Immediate callees with file paths |
| `impact_analysis(name, depth)` | What breaks if I change this? | BFS up to 5 hops + importing files + trait implementors |

### Type System

| Tool | When to Use | Example |
|------|-------------|---------|
| `type_hierarchy(name)` | Inheritance and implementations | Parents, subtypes, interfaces, implementors |

### Knowledge Graph

| Tool | When to Use | Example |
|------|-------------|---------|
| `knowledge_search(query)` | Find documented knowledge | Concepts, decisions, claims, sources |
| `knowledge_save(title, content, kind)` | File new knowledge | Save findings for future sessions |
| `knowledge_link(from, to, relation)` | Connect knowledge to code | Link a concept to its implementing function |
| `knowledge_lint()` | Check knowledge health | Find orphans, stale claims, missing links |

### Other

| Tool | When to Use | Example |
|------|-------------|---------|
| `graph_stats()` | Project overview | Count of files, functions, classes, knowledge |
| `semantic_search(query)` | Natural language code search | Embedding-based similarity search |
| `raw_query(query)` | Escape hatch | Custom SurrealQL queries (last resort) |
| `file_churn(path)` | Which files change most | Git-based change frequency |
| `hotspot_detection()` | Where are the problems | High churn + high complexity = hotspot |

## Token-Saving Patterns

### Delta Mode (context_bundle)

`context_bundle` has built-in delta detection. The first call returns the full file map. Subsequent calls for the same file in the same session return only structural changes:

- **No changes**: Returns a single-line "UNCHANGED" message instead of the full output
- **Changes detected**: Returns only added/removed lines (functions, callers, imports)

This saves ~80-97% tokens on repeat calls. You don't need to do anything special — just call `context_bundle` normally and the server handles caching.

### Graph-Ranked Search

`search_functions` automatically ranks results by graph importance (caller count). Functions with more callers appear first. Each result shows its caller count: `[12 callers]`. This means you're more likely to find the "important" function first without scrolling through leaf functions.

### Multi-Edge Impact Analysis

`impact_analysis` goes beyond call chains. After traversing the call graph (up to 5 hops), it also reports:

- **Files importing the module** — who depends on this file at the import level
- **Types implementing traits** — if the function's file defines a trait, who implements it

This gives a complete blast radius without needing separate `find_callers` + `type_hierarchy` + manual import tracing.

## Decision Flow: Which Tool to Use

```
Need to understand a file?
  → context_bundle(file_path)

Need to find a function?
  Know exact name → find_function(name)
  Know rough name → search_functions(query)
  Know the concept → semantic_search(query)

Need to understand connections?
  Who calls X? → find_callers(name)
  What does X call? → find_callees(name)
  Full neighborhood? → explore(name)

Need to assess change impact?
  → impact_analysis(name, depth=3)

Need documented knowledge?
  → knowledge_search(query)

Need project overview?
  → graph_stats()
```

## SurrealQL Tips (for raw_query)

- `function` is a reserved word — always backtick it: `` `function` ``
- Multi-hop traversal chains directly: `<-calls<-\`function\`<-calls<-\`function\`.name`
- Do NOT put dots between hops (that's a parse error silently swallowed)
- The dot is only for the final field projection

## Knowledge Entity Kinds

| Kind | Use For |
|------|---------|
| `concept` | Patterns, architectural ideas, domain concepts |
| `entity` | People, organizations, technologies |
| `source` | Papers, repos, articles, documentation |
| `claim` | Assertions that may be verified or refuted |
| `decision` | Architectural or design decisions with rationale |
| `question` | Open questions needing investigation |

## Knowledge Link Relations

| Relation | Direction | Example |
|----------|-----------|---------|
| `implemented_by` | knowledge → code | "Cache pattern" → `build_cache()` |
| `supports` | knowledge → knowledge | "Benchmark results" → "Performance claim" |
| `contradicts` | knowledge → knowledge | "New finding" → "Old assumption" |
| `related_to` | any → any | General association |
| `uses` | knowledge → knowledge | "Sprint plan" → "Design pattern" |

Note: `knowledge_link` prevents duplicate (from, to, relation) triples. Use `implemented_by` for the single canonical link; fall back to `related_to` or `uses` for additional connections.

## Web UI (port 9876)

The web visualization (`codescope web <path> --auto-index`) shows code and knowledge entities together:

- **Code nodes**: Spheres colored by type (blue=function, green=class, gray=file)
- **Knowledge nodes**: Octahedrons colored by kind (orange=concept, purple=entity, red=decision)
- **Code edges**: Solid lines (calls, contains, imports)
- **Knowledge edges**: Dashed lines (supports, contradicts, related_to)
- **Ctrl+K**: Search both code and knowledge
- **Click**: Node details panel with callers, callees, tags, confidence, linked entities
