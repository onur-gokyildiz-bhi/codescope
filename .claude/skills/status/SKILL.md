---
name: status
description: Sprint / project status from knowledge graph. Show what's planned, in-progress, done.
---

# /status

Read the knowledge graph, show the project state. No guessing from memory.

## When to invoke

- Start of a new session, "where were we?"
- Before deciding what to work on next
- Before a release (what's shipped vs planned?)
- When a new contributor asks "what's the roadmap?"

## Output

```
## Codescope Status (YYYY-MM-DD)

### In Progress
<list of status:in-progress entries, grouped by area>

### Planned (by priority)
**High:**
- <title> — <one-line summary>
**Medium:**
- ...
**Low:**
- ...

### Shipped this week
<entries with shipped:YYYY-MM-DD within last 7 days>

### Current version
<from Cargo.toml>
```

## Queries

```
# In progress
knowledge(action="search", query="status:in-progress")

# Planned by priority
knowledge(action="search", query="status:planned priority:high")
knowledge(action="search", query="status:planned priority:medium")
knowledge(action="search", query="status:planned priority:low")

# Recently shipped (last 7 days — tag contains "shipped:YYYY-MM-DD")
knowledge(action="search", query="shipped:2026-04")
```

## Rules

- **Never fabricate a roadmap.** If `knowledge(action="search", query=...)` returns nothing, say so. Don't invent plans.
- **Group by priority tag**, not by the order entries were created
- **Show item count** per group for quick scanning
- **Current version** from `Cargo.toml`, not from memory

## Codescope-first rule

This skill is a pure read skill — it reads the graph, doesn't modify anything. Safe to invoke any time.
