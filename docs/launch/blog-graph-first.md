# From semantic search to graph traversal: the missing fourth pattern in agentic context engineering

**Status:** blog post draft. Target: personal blog or dev.to, publish within 24 hours of the HN launch so referrers can link to a long-form explanation instead of just the repo.

**Suggested tags:** `rust`, `ai`, `developer-tools`, `claude`, `agentic-search`, `context-engineering`

---

## Intro — the three patterns everyone already knows

Leonie Monigatti runs an excellent [workshop on agentic search for context engineering](https://github.com/iamleonie/workshop-agentic-search) that walks through three retrieval patterns an LLM agent can use:

1. **Vanilla semantic search** — an agent calls a `semantic_search()` tool over a vector store
2. **Database query tool** — the agent generates and executes SQL/ESQL against a real database
3. **Shell + smart grep** — the agent shells out with `grep` (for exact matches) or `jina-grep` (for fuzzy semantic matches over files)

Each one is a real and useful pattern for a specific shape of retrieval. The workshop makes the trade-offs clear: semantic search is the easy default, database tools give the agent more expressive power at the cost of schema knowledge, and shell tools hand the agent everything but also the foot-gun.

What the workshop does not cover — and what I think is the missing fourth pattern — is **graph traversal**. The reason it's missing is that nobody had a code-aware graph to traverse. I spent the last year building one, and when I read the workshop I realized it's the exact frame I needed.

This post is the argument for why graph traversal belongs in that taxonomy, what questions it can answer that the other three can't, and how sub-millisecond multi-hop traversal works in practice.

## The questions the first three patterns cannot answer

Take any real codebase — let's say `ripgrep`. Imagine you're an agent and your user says "I'm changing the signature of `parse_args`; what's the blast radius?"

Pattern 1, **semantic search**, returns a list of functions that look similar to `parse_args` by embedding distance. Cosine similarity gives you fuzzy matches like `parse_config`, `parse_env`, `handle_args`. Useful for discovering related code. Useless for the actual question because cosine similarity is not a relationship, it's a geometric distance in a vector space. The fact that `parse_config` is nearby in embedding space says nothing about whether it calls `parse_args`.

Pattern 2, **DB query tool**, depends entirely on what's in the database. If your database is "sessions of the AI Engineer Europe conference", the agent can slice and dice metadata but it can't talk about code relationships. If your database is a commercial code-intelligence backend like Sourcegraph's SCIP, you get single-step references ("what does this symbol point to") — better, but still not multi-hop. A three-hop traversal would take three round trips, each of which is another SQL query the agent has to generate, each of which can fail in its own way.

Pattern 3, **shell + grep**, can find exact text matches of `parse_args` across the codebase. That gives you references but not call relationships. `parse_args` might appear in a docstring, a comment, a test name, a logging format string, a vendored library, and ten places where it's the name of a local variable that has nothing to do with the function. Then the agent has to LLM-classify each match to figure out which ones are actual calls, which defeats the speed advantage of grep.

None of the three can answer "who transitively calls `parse_args`, three hops out, deduplicated, with file paths". And yet that's exactly the question you need answered before you refactor anything real.

## The graph pattern

A **code knowledge graph** stores functions, classes, and types as nodes, and calls, imports, and inheritance edges as, well, edges. Parsing is done upfront with tree-sitter, which handles 47+ languages. The result lives in a graph database (in codescope's case, SurrealDB with the SurrealKV embedded backend — zero cluster, zero network, single binary).

Once the graph exists, the interesting question becomes *how the agent walks it*.

There are two naive ways to walk it that don't work well:

### Naive walk 1: iterative application-level BFS

The agent (or a Rust backend wrapper) queries for direct callers of `parse_args`, then queries for direct callers of each of those, and so on. This is what codescope's production `impact_analysis` tool does, because it's predictable and easy to reason about. It's fine — microseconds per hop on a warm database. But it's three to five round trips to the database plus some deduping overhead, and every round trip adds latency.

### Naive walk 2: nested subquery

You write a SurrealQL query that says "select functions where their out-edge is in a set of functions whose out-edge is in..." and so on. It looks right in SQL. It's catastrophically slow because the inner query re-evaluates for every row in the outer query. I benchmarked this at over five minutes on a 12k-entity graph for a 2-hop query. Don't do this. It's also not a graph walk at all — it's a quadratic set intersection pretending to be one.

### The good walk: native graph traversal syntax

SurrealDB recognizes a specific pattern as a graph walk and optimizes it as a single indexed edge traversal:

```sql
SELECT name, <-calls<-`function`<-calls<-`function`.name AS hop2_callers
FROM `function`
WHERE name = 'parse_args'
LIMIT 1
```

Read this right-to-left starting from `WHERE`: find the function named `parse_args`, walk backwards through `<-calls` edges to reach its direct callers, walk backwards again to reach their callers, and project the names.

What makes this fast is that SurrealDB's query planner recognizes the chained `<-edge<-table` pattern and walks indexed edges instead of evaluating subqueries. On real codebases this is sub-millisecond regardless of depth up to 3-5 hops. (Past 5 hops you start hitting graph fan-out limits that aren't SurrealDB's fault — it's that you're asking for a lot of data.)

**A note on getting the syntax exactly right:** the hops chain *directly*, not with dots between them. `<-calls<-function<-calls<-function.name` is correct. `<-calls<-function.<-calls<-function.name` is a parse error. The `.` is only for the final field projection at the end. I shipped a bogus benchmark once that used the dotted form; SurrealDB's parser returned an error which my wrapper function silently swallowed, and the query reported as running in 0.05ms. It was actually failing in 0.05ms. I've since fixed the wrapper to propagate parse errors and documented the correct syntax at every place that matters. A cautionary tale: low latency alone is not proof of correctness.

## Benchmarks on real codebases

Here are the numbers from four open-source projects, cold start (no query cache), measured with `std::time::Instant`. The impact target is dynamically discovered as the function with the highest fan-in in each repo — hardcoding `main` returns zero results because main is the *root* of the call graph, not an interior node.

| Repo | Entities | Edges | 2-hop impact | 3-hop impact | WHERE-filter equivalent |
|------|----------|-------|--------------|--------------|-------------------------|
| ripgrep | 4,623 | 16,535 | 0.64 ms | 0.92 ms | 59.01 ms |
| axum | 5,278 | 15,068 | 0.56 ms | 0.52 ms | 49.05 ms |
| tokio | 13,600 | 44,675 | 0.68 ms | 0.49 ms | 118.21 ms |
| gin (Go) | 2,400 | 11,324 | 1.44 ms | 1.29 ms | 40.56 ms |

Two things worth noting:

1. **Native traversal stays sub-millisecond from 11k edges all the way to 45k edges.** It is bounded by graph fan-out at the target function, not by the total size of the corpus. This is the property that makes it viable for agent use: the latency cost of answering "what's the blast radius?" is the same whether you're working on a 4k-line toy or the tokio codebase.

2. **Multi-hop is *faster* than single-hop with a WHERE clause.** The third column is the same question (find callers) posed as `SELECT in.name FROM calls WHERE out.name = 'X'`. That's a full scan of the calls table because there's no graph index on the WHERE path. It scales linearly with edge count: 40 ms on gin (11k edges), 118 ms on tokio (45k edges). Three hops of indexed traversal beats one hop of table scan. Graph-first is not a marketing term; it's a query-planner property.

## So what does this look like as a tool?

In codescope, the agent calls:

```
impact_analysis(function_name="parse_args", depth=3)
```

and gets back a structured tree:

```
## Impact Analysis: parse_args

### Direct Callers
- `main` (crates/core/src/bin/rg/main.rs)
- `parse_from_args` (crates/core/src/args.rs)
...

### Indirect Callers (2 hops)
- `run` (crates/core/src/bin/rg/main.rs)
...

### Indirect Callers (3 hops)
...
```

That response is typically a few hundred tokens. The equivalent "read the whole file" approach through grep-and-LLM-classify is 30,000+ tokens and many more round trips.

The tool description the agent sees is written to disambiguate it from the naive option:

> TRANSITIVE blast radius of changing a function. Walks the call graph backwards via BFS to a configurable `depth` (default 3, max 5)... Sub-millisecond on real codebases (graph-first traversal walks indexed edges, not text scans). For just the immediate (1-hop) callers use `find_callers`. For type-level inheritance impact use `type_hierarchy` instead.

This framing — "when to use X vs Y" — is something I learned from Leonie's workshop prompts, which are exemplary in that regard. The workshop's `grep vs jina-grep` decision rule and the explicit "don't know" gating are both patterns I lifted into codescope's MCP tool descriptions this week.

## What graph-first doesn't solve

Graph-first is not a replacement for embeddings. It's the primary index; embeddings are a secondary fallback. Some questions are structural (multi-hop, relational, deterministic) and some are semantic (fuzzy, content-based, probabilistic). A question like "find me all the config parsing functions" is semantic — you don't know what they're called, you know what they *do*. For that, codescope falls back to cosine similarity over 384-dim binary-quantized embeddings, which is 32x more memory-efficient than float32 vectors and runs in under 30 ms on real codebases.

The mental split is: **graph walks for structure, embeddings for meaning.** Use both. Don't pretend one is the other.

## Where this fits in Leonie's taxonomy

To close the loop: agentic search for context engineering has at least four useful patterns, and your retrieval tool choice depends on the question shape.

| Pattern | Good for | Bad for |
|---------|----------|---------|
| 1. Semantic search | "find content that *means* X" | relationships, exact structure |
| 2. DB query | "aggregate, filter, join rows" | multi-step graph walks, schema discovery |
| 3. Shell + grep | exact text, filename navigation | fuzzy matching, non-text relationships |
| 4. **Graph traversal** | "who depends on X, how, across N hops" | content-based fuzzy queries |

Codescope is the fourth pattern for code. If you're an agent developer working on code context, pattern 4 is how you answer the questions grep and embeddings can't touch. It's the tool your agent reaches for when the user asks a structural question — "what breaks", "who calls", "what inherits from" — and you want an answer in milliseconds, not tokens.

## Try it

```bash
# Linux / macOS
curl -fsSL https://raw.githubusercontent.com/onur-gokyildiz-bhi/codescope/main/install.sh | bash
# Windows
irm https://raw.githubusercontent.com/onur-gokyildiz-bhi/codescope/main/install.ps1 | iex
```

Then `cd <your-project> && codescope init` and open your MCP-aware agent. Fifty-two tools, 59 supported formats, 22 MB DB for a ~5k-entity repo, everything runs locally.

Repo, benchmarks, and quickstart: https://github.com/onur-gokyildiz-bhi/codescope

Thanks to Leonie Monigatti for the taxonomy frame, to the SurrealDB team for getting the native graph traversal syntax right, and to the tree-sitter maintainers for the parsing layer.

---

**Blog follow-up ideas (for post-launch, if the post lands well):**

1. "The SurrealDB parse-error swallow trap" — deep-dive on the raw_query bug, how it enabled bogus benchmarks, and what the fix teaches about trusting low latency as correctness
2. "Building an MCP server in Rust with rmcp" — code walkthrough of the tool_router macro, tool description patterns, and agent disambiguation rules
3. "Why embeddings are a secondary index, not a primary one" — longer treatment of the structural vs semantic split, with real examples
4. "Cross-repo knowledge graphs" — the harder version of this problem when your "codebase" is actually 30 services in a monorepo
