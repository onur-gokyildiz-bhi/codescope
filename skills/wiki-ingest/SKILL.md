---
name: wiki-ingest
description: >
  Ingest any source (file, URL, image) into the codescope knowledge graph.
  Extracts entities, concepts, claims, and cross-references them with existing
  code entities. Triggers on: "ingest this", "add to knowledge base", "read and file",
  "process this source", "ingest [url]", "wiki ingest".
allowed-tools: Read Write Edit Glob Grep Bash WebFetch mcp__codescope__knowledge_save mcp__codescope__knowledge_link mcp__codescope__knowledge_search mcp__codescope__semantic_search mcp__codescope__memory_save
user-invocable: true
argument-hint: "<file-path or URL>"
---

# wiki-ingest: Source Ingestion into Knowledge Graph

You are a knowledge architect. Read the source, extract structured knowledge, and file it into the codescope knowledge graph. Every source should touch 5-15 graph nodes.

## Ingestion Flow

### 1. Read the source

- **File path**: Read the file directly
- **URL**: Use WebFetch to retrieve content
- **Image**: Read the image, describe contents (OCR + visual analysis)

### 2. Extract structured knowledge

From the source, identify:

**Entities** (people, orgs, products, repos, technologies):
```
For each entity:
  - name (canonical form)
  - kind: person | org | product | repo | technology | standard | concept
  - description (1-2 sentences)
  - source: the file/URL this came from
```

**Claims** (factual assertions with confidence):
```
For each claim:
  - statement (one sentence, falsifiable)
  - confidence: high | medium | low
  - source: where this claim comes from
  - entities: which entities this claim is about
```

**Concepts** (ideas, patterns, frameworks):
```
For each concept:
  - name
  - description
  - related_concepts: other concepts this links to
  - related_code: function/class names in the codebase this maps to
```

### 3. File into the knowledge graph

For each extracted item, call the appropriate MCP tool:

```
knowledge_save(
  title: "OAuth2 Authorization Flow",
  content: "OAuth2 uses authorization codes...",
  kind: "concept",
  source_url: "https://...",
  tags: ["auth", "security"]
)
```

### 4. Cross-reference with code

After filing knowledge entities, link them to existing code:

```
knowledge_link(
  from_entity: "OAuth2 Authorization Flow",
  to_entity: "handle_auth_callback",
  relation: "implemented_by"
)
```

Use `search_functions` or `semantic_search` to find related code entities.

### 5. Check for contradictions

Search existing knowledge for claims that conflict with new ones:

```
knowledge_search(query: "OAuth2 token expiry")
```

If contradictions found, flag them:
```
knowledge_save(
  title: "Contradiction: token expiry time",
  content: "Source A says 1 hour, Source B says 24 hours",
  kind: "contradiction",
  tags: ["needs-resolution"]
)
```

### 6. Log the operation

```
memory_save(
  content: "Ingested [source]. Created N entities, M claims, L links.",
  kind: "operation"
)
```

## Batch Mode

If user says "ingest all files in .raw/":

1. List all files in the directory
2. Process each sequentially
3. After all done, report summary: files processed, entities created, links made

## Output Format

After ingestion, report:

```
## Ingested: [source name]

**Entities created:** 5
- OAuth2 (technology)
- Google Auth (product)
- handle_auth_callback (code link)
...

**Claims filed:** 3
**Cross-references:** 7 (4 to code, 3 to existing knowledge)
**Contradictions found:** 0

Knowledge graph updated.
```
