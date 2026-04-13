---
name: autoresearch
description: >
  Autonomous research loop. Takes a topic, runs web searches, fetches sources,
  synthesizes findings, and files everything into the knowledge graph.
  Based on Karpathy's autoresearch pattern. Triggers on: "/autoresearch",
  "research [topic]", "deep dive into [topic]", "investigate [topic]",
  "find everything about [topic]", "go research".
allowed-tools: Read Write Edit Glob Grep WebFetch WebSearch mcp__codescope__knowledge_save mcp__codescope__knowledge_link mcp__codescope__knowledge_search mcp__codescope__semantic_search mcp__codescope__memory_save
user-invocable: true
argument-hint: "<topic>"
---

# autoresearch: Autonomous Research Loop

You are a research agent. Take a topic, run iterative web searches, synthesize findings, and file everything into the codescope knowledge graph. The user gets structured knowledge nodes, not a chat response.

## Research Program

**Input:** topic from user

**Constraints:**
- Max 3 rounds of search
- Max 5 web fetches per round
- Stop when: all major angles covered OR contradictions resolved OR max rounds hit
- File everything — don't summarize in chat, put it in the graph

## Research Loop

### Round 1: Broad Survey

1. Decompose the topic into 3-5 distinct search angles
2. For each angle: WebSearch with 2-3 queries
3. For top 2-3 results per angle: WebFetch the content
4. Extract from each source:
   - Key claims (with confidence)
   - Entities (people, orgs, technologies)
   - Concepts (patterns, frameworks, approaches)
   - Open questions (what's still unclear)

### Round 2: Gap Fill

5. Review what's missing or contradicted from Round 1
6. Run targeted searches for each gap (max 5 queries)
7. Fetch and extract from top results
8. Update confidence on claims that are now corroborated or contradicted

### Round 3: Synthesis (if needed)

9. Only if major contradictions or gaps remain
10. Final targeted pass, then proceed to filing regardless

## Filing into Knowledge Graph

After each round, file immediately (don't wait until all rounds finish):

**For each source found:**
```
knowledge_save(
  title: "[Source title]",
  content: "[Summary + key contributions]",
  kind: "source",
  source_url: "[URL]",
  tags: ["[topic]", "[subtopic]"]
)
```

**For each entity discovered:**
```
knowledge_save(
  title: "[Entity name]",
  content: "[Who/what, why relevant to topic]",
  kind: "entity",
  tags: ["[topic]"]
)
```

**For each concept/pattern:**
```
knowledge_save(
  title: "[Concept name]",
  content: "[Explanation, how it relates to the topic]",
  kind: "concept",
  tags: ["[topic]"]
)
```

**Cross-link everything:**
```
knowledge_link(from: "Entity A", to: "Concept B", relation: "uses")
knowledge_link(from: "Source X", to: "Claim Y", relation: "supports")
```

**Link to code if relevant:**
```
# Search for related code in the current project
search_functions(query: "[technology name]")
# If found, link:
knowledge_link(from: "OAuth2", to: "handle_auth", relation: "implemented_by")
```

## Output

After all rounds complete, provide a synthesis report:

```
## Research Complete: [topic]

**Rounds:** 2 (stopped: all angles covered)
**Sources processed:** 8
**Knowledge nodes created:** 15
  - 3 entities
  - 5 concepts
  - 4 sources
  - 3 claims
**Code cross-references:** 4
**Contradictions found:** 1 (flagged)
**Open questions:** 2 (filed for follow-up)

### Key Findings
1. [Most important finding]
2. [Second finding]
3. [Third finding]

### Contradictions
- [Claim A] vs [Claim B] — needs human resolution

All findings filed to knowledge graph. Query with:
  knowledge_search(query: "[topic]")
```

## References

See [references/program.md](references/program.md) for customizable research constraints.
