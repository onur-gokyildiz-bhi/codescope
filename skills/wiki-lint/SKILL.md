---
name: wiki-lint
description: >
  Health check the knowledge graph. Finds orphan nodes, missing cross-references,
  stale claims, unresolved contradictions, and knowledge gaps.
  Triggers on: "lint the wiki", "knowledge health", "check knowledge base",
  "wiki lint", "find orphans".
allowed-tools: mcp__codescope__knowledge_search mcp__codescope__knowledge_lint mcp__codescope__raw_query mcp__codescope__graph_stats
user-invocable: true
---

# wiki-lint: Knowledge Graph Health Check

Audit the knowledge graph and report issues. Fix what you can automatically, flag what needs human attention.

## Checks

### 1. Orphan nodes
Knowledge entities with no incoming or outgoing edges:
```
knowledge_lint(check: "orphans")
```
**Auto-fix:** search for related entities and suggest links.

### 2. Unresolved contradictions
Claims flagged as contradictions that haven't been resolved:
```
knowledge_search(query: "contradiction")
```
**Report:** list each with both sides and ask human to resolve.

### 3. Low-confidence clusters
Groups of claims all at "low" confidence:
```
knowledge_lint(check: "low_confidence")
```
**Suggest:** "/autoresearch [topic]" to find corroborating sources.

### 4. Code-knowledge gaps
Code entities (functions, classes) with no knowledge context:
```
knowledge_lint(check: "unlinked_code")
```
**Report:** "These 5 most-called functions have no knowledge context. Consider ingesting their design docs."

### 5. Stale sources
Sources older than 6 months with no recent corroboration:
```
knowledge_lint(check: "stale")
```
**Report:** "These claims may be outdated — consider re-researching."

## Output

```
## Knowledge Graph Health Report

**Total nodes:** 142 (67 entities, 35 concepts, 25 sources, 15 claims)
**Total edges:** 384

### Issues Found

| Check | Count | Severity |
|-------|-------|----------|
| Orphan nodes | 3 | low |
| Unresolved contradictions | 1 | high |
| Low-confidence clusters | 2 | medium |
| Code-knowledge gaps | 8 | medium |
| Stale sources | 0 | — |

### Actions Needed
1. **[high]** Resolve contradiction: "token expiry" — Source A vs Source B
2. **[medium]** Run `/autoresearch OAuth2` to corroborate 4 low-confidence claims
3. **[medium]** Ingest design docs for: handle_auth, parse_config, db_connect...
4. **[low]** 3 orphan knowledge nodes — suggest links below

**Health score: 7/10**
```
