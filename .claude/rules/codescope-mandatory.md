---
description: MANDATORY — Use codescope MCP tools instead of Read/Grep
globs: **/*
alwaysApply: true
---

# Codescope MCP: MANDATORY

Use codescope tools for ALL code exploration. Do NOT fall back to Read/Grep/Glob.

## Rules

1. BEFORE reading a file → `context_bundle(file_path)`
2. BEFORE grepping callers → `find_callers(name)` or `find_callees(name)`
3. BEFORE searching functions → `search_functions(query)` or `find_function(name)`
4. BEFORE tracing impact → `impact_analysis(name, depth=3)`
5. BEFORE exploring connections → `explore(name)` or `backlinks(name)`
6. BEFORE git history → `file_churn(path)` or `hotspot_detection()`

## When Read/Grep IS OK

- Reading function **body** AFTER codescope pinpointed the exact file:line
- Non-code files (README, Cargo.toml, configs)
- Literal string search in non-code content
