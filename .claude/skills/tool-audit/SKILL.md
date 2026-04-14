---
name: tool-audit
description: Audit MCP tool set — count, descriptions, consolidation candidates, budget compliance.
---

# /tool-audit

Verify the MCP tool surface is within budget and well-described. Dewey's periodic audit.

## When to invoke

- Before a release (Hopper checks in with Dewey)
- After adding new tools
- Periodically to catch description creep
- When an agent complains "too many tools, can't pick"

## Checks

### 1. Tool count ≤ 40

```bash
grep -c '#\[tool(' crates/mcp-server/src/tools/*.rs | awk -F: '{sum+=$2} END {print "Total tools:", sum}'
```

Budget: **40 max**. Current target: **≤ 32**.

If over budget:
- List all tools: `grep -B1 'async fn ' crates/mcp-server/src/tools/*.rs | grep 'async fn' | sed 's/.*async fn //; s/(.*//' | sort`
- Identify consolidation candidates (similar semantics, overlapping params)
- Consult Dewey agent for consolidation plan

### 2. Description length ≤ 100 chars per tool

```bash
grep -B5 'async fn ' crates/mcp-server/src/tools/*.rs | grep 'description = "' | awk -F'description = "' '{print length($2)-2, $2}' | sort -rn | head -10
```

Any line starting with a number > 100 is over budget.

### 3. Description keyword quality

For each tool description, check:
- First 50 chars answer "WHAT does it do?"
- Has concrete domain keywords the model can match
- Disambiguates from similar tools

### 4. Consolidation candidates

Look for tool pairs where descriptions differ in <3 keywords — those are merge candidates.

## Output format

Report markdown:
```
## Tool Audit (YYYY-MM-DD)

**Total tools:** N / 40 ✅
**Descriptions over 100 chars:** 0 ✅

### Tools by file
...

### Consolidation candidates
- tool_a + tool_b — both search-related, could be merged

### Breaking change budget
Next release: patch / minor / major
```

## Codescope-first rule

- `search(mode="fuzzy", query="#\\[tool(")` to see all tool declarations in context
- `knowledge(action="search", query="tool consolidation")` for historical decisions
