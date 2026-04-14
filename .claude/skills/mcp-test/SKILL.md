---
name: mcp-test
description: End-to-end MCP server verification — spawn stdio server, list tools, invoke each one, verify response shape.
---

# /mcp-test

Sanity-check the MCP server after a binary update, tool refactor, or schema migration. Catches the class of bugs that unit tests miss — protocol-level issues, parameter deserialization failures, tool dispatch errors.

## When to invoke

- After a tool consolidation or rename
- After a schema/parser change that affects tool output
- Before a release (part of `/lint-all`'s extended run)
- When a user reports "tool not found" after upgrade

## Protocol

### 1. Pick a test repo

Small, already-indexed, representative. Default: the codescope repo itself (indexes in ~10s, has all entity types).

```bash
CS_TEST_REPO="${1:-.}"
cd "$CS_TEST_REPO"
```

### 2. Spawn MCP server in stdio mode

```bash
codescope mcp "$CS_TEST_REPO" --auto-index &
MCP_PID=$!
sleep 6  # let the initial index settle
```

Or, cleaner: use an existing daemon at 9877 via HTTP. `curl -s http://127.0.0.1:9877/mcp/health` (if we add a health endpoint later).

### 3. Verify binary version matches source

```bash
INSTALLED=$(codescope --version | grep -oE '[0-9]+\.[0-9]+\.[0-9]+')
SOURCE=$(grep '^version' Cargo.toml | head -1 | grep -oE '[0-9]+\.[0-9]+\.[0-9]+')
[ "$INSTALLED" = "$SOURCE" ] && echo "OK $INSTALLED" || echo "MISMATCH: installed=$INSTALLED source=$SOURCE"
```

If mismatch: stop. Release captain didn't rebuild. `cargo build --release && cp target/release/codescope* ~/.local/bin/`.

### 4. Invoke each tool category via MCP

Use `codescope mcp` directly — but easier: invoke each tool from within Claude Code (via `/mcp-test`) and inspect results.

Manual invocation checklist (each must return non-empty, correctly-shaped output on a non-empty graph):

**Core search (5 modes):**
```
search(mode="fuzzy", query="main", limit=5)
search(mode="exact", query="main")
search(mode="file", query="Cargo.toml")
search(mode="cross_type", query="config")
search(mode="neighborhood", query="main")
search(mode="backlinks", query="main")
```

**Call graph:**
```
find_callers("main")
find_callees("main")
impact_analysis("main", depth=2)
```

**Code health:**
```
code_health(mode="hotspots")
code_health(mode="churn")
```

**Knowledge (round-trip test):**
```
# Save
knowledge(action="save", title="MCP Test Probe <timestamp>",
          content="ephemeral test entry", kind="concept",
          confidence="low", tags=["mcp-test", "ephemeral"])

# Search (should find what we just saved)
knowledge(action="search", query="MCP Test Probe")

# Cleanup — DELETE via raw_query
raw_query("DELETE knowledge WHERE 'mcp-test' IN tags")
```

**Memory:**
```
memory(action="save", text="mcp-test ephemeral")
memory(action="search", text="mcp-test ephemeral")
```

**Context bundle + delta mode:**
```
context_bundle("Cargo.toml")  # First call — full output
context_bundle("Cargo.toml")  # Second call — MUST return UNCHANGED
```

**Stats:**
```
graph_stats()
supported_languages()
```

### 5. Check response shapes

For each invocation above:
- **No error string** in the response (look for "Error:", "Parse error:", "tool not found")
- **Non-empty result** on a populated graph (unless the query is expected to return empty)
- **Delta mode confirms "UNCHANGED"** on the second context_bundle call — this is the most reliable signature that we're running the new binary

### 6. Output

```
## MCP Test Report — <date>

**Version:** installed <X.Y.Z>, source <X.Y.Z> ✅
**Tool count:** <N> ✅
**Categories tested:** search (6), callgraph (3), code_health (2),
                       knowledge (3), memory (2), context (2), stats (2)

**Failures:** 0 ✅
**Warnings:** <e.g. "knowledge_search parse error on bind() — known in v0.7.2, fixed v0.7.3">
```

## Known failure patterns

| Symptom | Likely cause | Next step |
|---|---|---|
| "tool not found: foo" | Old tool name, consolidated | Update caller to use new `mode=` or `action=` surface |
| `knowledge_search` returns `Parse error` | `ORDER BY updated_at` not in SELECT | Binary is pre-v0.7.3; upgrade |
| `tags CONTAINS` returns empty | SurrealDB bind bug | Binary is pre-v0.7.3; upgrade |
| Second `context_bundle` returns full output | Delta mode not wired | Check `context_cache` field on `GraphRagServer` |
| LOCK held by another process | Another codescope proc | `pkill -f codescope` |

## Codescope-first rule

This skill tests codescope itself — but STILL uses codescope tools for verification rather than shelling to grep/find. If a tool is broken, the test will surface it — don't bypass with text search.

- `knowledge(action="search", query="mcp-test")` to find past failures
- After test: `knowledge(action="save", ...)` if a new class of bug was found
