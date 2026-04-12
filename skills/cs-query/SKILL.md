---
name: cs-query
description: Execute a raw SurrealQL query on the knowledge graph. Use for advanced queries, custom reports, or when other tools are not specific enough.
user-invocable: true
disable-model-invocation: true
argument-hint: "<surql-query>"
---

# Raw SurrealQL Query

Execute a custom SurrealQL query using the `raw_query` MCP tool.

Query: **$ARGUMENTS**

## Important

- `function` is a reserved word — always use backticks: `` `function` ``
- String search: `string::contains(string::lowercase(name), "pattern")`

## Common Queries

```sql
-- All functions
SELECT name, file_path FROM `function` ORDER BY name

-- Largest functions by line count
SELECT name, file_path, (end_line - start_line) AS lines FROM `function` ORDER BY lines DESC LIMIT 10

-- All structs
SELECT name, file_path FROM class WHERE kind = "Struct"

-- Functions in a specific file
SELECT name, start_line FROM `function` WHERE file_path CONTAINS "main"

-- Call graph for a function
SELECT ->calls->`function`.name AS calls FROM `function` WHERE name = "main"

-- Most imported modules
SELECT name, count() AS usage FROM import_decl GROUP BY name ORDER BY usage DESC LIMIT 10

-- Files with most functions
SELECT file_path, count() AS fn_count FROM `function` GROUP BY file_path ORDER BY fn_count DESC LIMIT 10
```

Display results as a formatted table. Explain the results briefly.

## Full SurrealQL Reference

See [references/SURREALQL.md](references/SURREALQL.md) for the complete syntax guide including graph traversal, anti-patterns, and parameterized queries.
