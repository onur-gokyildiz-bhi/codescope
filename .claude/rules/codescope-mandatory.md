---
description: MANDATORY — Use codescope MCP for exploration AND knowledge tracking
globs: **/*
alwaysApply: true
---

# Codescope MCP: MANDATORY

## Tool Rules (exploration)

Use codescope tools for ALL code exploration. Do NOT fall back to Read/Grep/Glob.

1. BEFORE reading a file → `context_bundle(file_path)`
2. BEFORE grepping callers → `find_callers(name)` or `find_callees(name)`
3. BEFORE searching functions → `search_functions(query)` or `find_function(name)`
4. BEFORE tracing impact → `impact_analysis(name, depth=3)`
5. BEFORE exploring connections → `explore(name)` or `backlinks(name)`
6. BEFORE git history → `file_churn(path)` or `hotspot_detection()`

## Knowledge Tracking (auto-triggers)

These are NOT optional. Claude calls them without being asked:

1. **BEFORE starting any non-trivial task** → `knowledge_search(topic)`. If `status:done` already exists, ask the user if they want changes to the existing implementation instead of reimplementing.

2. **AFTER completing a feature/fix/refactor** → `knowledge_save` with `kind: "decision"`, tags `["status:done", "vX.Y.Z", "<area>", "shipped:YYYY-MM-DD"]`. Title is the feature name, content explains what changed and where.

3. **AFTER user correction** ("no, do it this way") → `capture_insight` with `kind: "correction"`.

4. **AFTER finding a bug that took >5 minutes** → `capture_insight` with `kind: "problem"` so next session finds it instantly.

5. **BEFORE architectural decisions** → `knowledge_search` for prior `decision` entries on the same topic.

Save only significant moments. Skip: variable renames, obvious fixes, formatting. Less is more.

## Status Tags

| Tag | When |
|-----|------|
| `status:done` | Shipped |
| `status:planned` | Roadmap |
| `status:in-progress` | Active |
| `status:blocked` | Waiting |
| `shipped:YYYY-MM-DD` | Absolute ship date |
| `vX.Y.Z` | Release version |

## When Read/Grep IS OK

- Reading function **body** AFTER codescope pinpointed the exact file:line
- Non-code files (README, Cargo.toml, pubspec.yaml, configs)
- Literal string search in non-code content
