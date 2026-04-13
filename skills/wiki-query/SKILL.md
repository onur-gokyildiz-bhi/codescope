---
name: wiki-query
description: >
  Query the knowledge graph for answers. Searches knowledge entities, claims,
  sources, and cross-references with code. Cites specific graph nodes, not
  training data. Triggers on: "what do we know about", "query knowledge",
  "check the wiki", "knowledge search".
allowed-tools: mcp__codescope__knowledge_search mcp__codescope__semantic_search mcp__codescope__explore mcp__codescope__find_callers mcp__codescope__impact_analysis Read
user-invocable: true
argument-hint: "<question>"
---

# wiki-query: Knowledge Graph Query

Answer the user's question using ONLY the knowledge graph. Cite specific nodes. If the knowledge graph doesn't have the answer, say so — don't fall back to training data.

## Query Strategy

1. **Search knowledge graph first:**
   ```
   knowledge_search(query: "[user's question]")
   ```

2. **If knowledge results reference code**, drill into it:
   ```
   find_callers(function_name: "[referenced function]")
   explore(entity_name: "[referenced entity]")
   ```

3. **If knowledge graph is empty for this topic**, say:
   > "No knowledge found on this topic. Run `/autoresearch [topic]` or `/wiki-ingest [source]` to add it."

## Answer Format

```
## [Question rephrased as title]

[Answer in 2-5 paragraphs]

### Sources
- [[Entity/Concept name]] — [what it contributed to the answer]
- [[Source name]] — [key claim from this source]

### Related Code
- `function_name` (file.rs:42) — [how it relates]

### Confidence: [high/medium/low]
[Why this confidence level — how many sources, any contradictions]
```

## Rules

- **NEVER** answer from training data when knowledge graph has relevant nodes
- **ALWAYS** cite the specific knowledge node (title + kind)
- **FLAG** if you find contradictions between knowledge nodes
- If answer requires both knowledge AND code, combine them
