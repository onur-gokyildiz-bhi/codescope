# Stop RAG-ing Your Codebase. Graph It.

So, you installed Cursor. Or Claude Code. Or Windsurf. You asked it a perfectly reasonable question:

> *"If I change `User::email`, what breaks?"*

And it read 40 files, burned 150,000 tokens, gave you a confident answer that missed three callers in a handler you didn't touch. You found them two hours later, in production.

That is not a model problem. Models are getting better every month. That is an **architecture** problem. And it is the thing this article is about.

I'll go through why the dominant "embed everything, nearest-neighbor search it" approach breaks on code, what the alternative looks like, and how we built one — open source, Rust, sub-millisecond, 99%+ cheaper than the tool you're probably using right now.

I'm telling you one thing: this article is long. But you're reading it because you already feel what I'm about to describe.

---

## The problem with embeddings-first

Here is how every major AI coding assistant works today:

1. Walk your repo.
2. Chunk each file into ~500 token windows.
3. Embed each chunk as a vector with some text embedding model.
4. Store the vectors in a vector database.
5. When the user asks a question, embed the question, nearest-neighbor search for similar chunks, stuff those chunks in the context.

This is RAG. It's the same pattern that works great for "find me a document about X" and lands somewhere between useless and dangerous for "find me the blast radius of this change."

Why? Because **embeddings capture semantic similarity**. They don't capture **structural reasoning**.

Consider this question: *"What functions transitively depend on `parse_config`?"*

A vector search might find:
- Other files that mention "config parsing"
- A README section about configuration
- A test helper with a similar name

What a vector search cannot find:
- The function that calls the function that calls `parse_config` three layers deep, but uses completely different vocabulary
- The trait implementor that overrides the behavior without ever mentioning `parse_config` in its text
- The HTTP handler that depends on a struct that depends on `parse_config`'s output

Code is a graph. Vectors are a bag-of-words. The mismatch is fundamental.

---

## The mental model

Every question you ask about a codebase is one of two flavors:

**Semantic questions** — "Where do we handle authentication?" "Show me code related to rate limiting." "Is there a config parser here?"

**Structural questions** — "Who calls this?" "What breaks if I change this signature?" "Which trait implementations exist?" "Does this function touch the database?"

Vector search is genuinely good at the first. It's genuinely bad at the second. And when you ask an AI coding assistant a structural question, it answers using vector search, because that's what it has. So it *simulates* graph traversal by reading 40 files and hoping.

That's expensive. That's slow. That's wrong more often than anyone wants to admit.

---

## What a code graph actually looks like

Parse your code with tree-sitter. Extract every function, class, trait, import, struct field. That's your node set.

Now extract edges:

- `function A` — **calls** → `function B`
- `class C` — **contains** → `function D`
- `file F` — **imports** → `package P`
- `struct S` — **implements** → `trait T`
- `class Sub` — **inherits** → `class Super`

Put the whole thing in a graph database. Now *"who calls `parse_config` transitively within 3 hops"* is a BFS. Milliseconds. Exact answer.

```
Question:       "Who calls parse_config transitively, depth 3?"
RAG approach:   Search 12,000 vectors, return top-k chunks, read 40 files.
                148,000 tokens. ~12 seconds. Fuzzy confidence.
Graph approach: Walk 3 edges backwards from parse_config node.
                542 tokens. 0.8 ms. Deterministic.
```

That is not a different flavor of the same system. It is a different system.

---

## Why nobody did this before (well, almost)

Three reasons, mostly historical.

**First**, the embeddings era started around 2022 with ChatGPT plugins and the first wave of RAG libraries. Everyone who built a code assistant during that window reached for the same toolkit: OpenAI embeddings, Pinecone or Weaviate, a chunking pipeline, a retrieval function. It worked. Good enough. So that became the default.

**Second**, building a proper code graph is hard. You need tree-sitter (or a commercial equivalent) for every language you support. You need a graph database that does sub-millisecond multi-hop traversal without breaking. You need to handle incremental updates because codebases change. You need schema migration, because the graph shape changes when you add features. Most teams don't have the patience.

**Third**, SCIP and LSIF — the index formats Sourcegraph built — do part of the job, but they're designed for *read-only navigation*, not interactive querying by an AI agent. Nobody exposed them as an MCP or LSP interface that Claude Code or Cursor could actually call.

The opening was there. We took it.

---

## Building the graph: the parser layer

Tree-sitter is the unsung hero here. It parses 47 languages into concrete syntax trees, fast enough to reparse a file on every keystroke, robust enough to handle syntactically invalid code (you're mid-edit — of course the file doesn't parse yet).

For each language, we run three passes:

```
Pass 1 — Entity extraction:
  Walk the tree, emit a node for each function, class, trait, import.
  Capture start_line, end_line, qualified_name, signature.

Pass 2 — Relation extraction:
  For each call expression, emit a calls edge.
  For each import statement, emit an imports edge.
  For each trait impl block, emit an implements edge.

Pass 3 — Content entities:
  Parse .toml/.yaml/.json/.md/.sql/.proto files for config,
  docs, SQL queries, API schemas. Emit them as first-class nodes.
```

The interesting part is pass 3. Every tool in this space stops at code. But a codebase is more than code. Your OpenAPI schema tells you what endpoints exist. Your Dockerfile tells you what services you run. Your `.env` tells you what secrets you need. If the graph doesn't have them, the agent can't reason about them.

We emit nodes for all of it. Same graph. Same query surface.

For CUDA, we go further. `__global__` and `__device__` qualifiers on functions, kernel launch sites (`kernel<<<grid, block>>>`), launch configurations — all indexed. Because somebody is going to ask "which kernel does this host function launch?" and we'd like the answer to take a millisecond.

---

## The storage layer: SurrealDB

We needed a graph database that runs embedded, supports multi-hop traversal, and doesn't require Java.

SurrealDB fits. It's Rust-native, has its own query language (SurrealQL) with graph traversal primitives baked in, and it runs as a library — no separate daemon, no port, no Kubernetes.

A multi-hop traversal looks like this in SurrealQL:

```sql
SELECT <-calls<-`function`<-calls<-`function`.name
FROM `function`
WHERE name = 'parse_config';
```

That's "starting from `parse_config`, walk the `calls` edge backwards twice, return the names." The database turns it into two indexed edge walks. Sub-millisecond on a graph of 50K entities.

Compare that with doing the same thing via SQL self-joins (what you'd do on Postgres):

```sql
SELECT c2.caller
FROM calls c1
JOIN calls c2 ON c1.caller = c2.target
WHERE c2.target = 'parse_config';
```

Two joins, full scan on the `calls` table if you don't have the right composite index, and you still have to hand-craft the recursion for depth > 2.

Graph database isn't a buzzword. It's a data structure that fits the problem.

---

## Making it usable: the MCP layer

A graph in a database is not useful to an AI agent. An agent talks to you via tool calls. It needs something to call.

Enter MCP — Model Context Protocol. Anthropic published it in late 2024; it's the JSON-RPC-over-stdio interface that any MCP-compatible agent (Claude Code, Cursor, Zed, Codex) can plug into. Expose your tools, the agent lists them, the model picks one, the server runs a query, returns a string. Done.

We expose 32 tools. Some examples:

```
search(mode="fuzzy", query="parse")           # substring function search
search(mode="neighborhood", query="User")      # callers + callees + siblings
impact_analysis("handler", depth=3)            # transitive blast radius, BFS
knowledge(action="search", query="status:done") # cross-session memory
code_health(mode="hotspots")                   # high-churn + high-complexity
refactor(action="safe_delete", name="foo")     # zero-callers check
```

Key design insight we learned the hard way: **fewer tools is better.** Research — and then our own experience — shows that when the tool list exceeds 30, the model starts to degrade at picking the right one. We originally had 57 tools. Consolidated to 32. Claude's accuracy at selecting the correct tool went up measurably. Token overhead on the schema dropped too.

We consolidated by collapsing variants into dispatching tools with an `action` or `mode` parameter. `memory_save` + `memory_search` + `memory_pin` all became `memory(action=...)`. Nine tools for "code health" became `code_health(mode=...)`. Four knowledge ops became `knowledge(action=...)`.

This is a small thing that turns out to matter a lot.

---

## The second brain: cross-session memory

Here is the thing that actually changes how AI coding assistants feel.

Every AI coding session today starts cold. The model has no memory of what you decided last week. It doesn't know why you use `Arc<RwLock>` instead of `Mutex` in that one spot. It will suggest the pattern you explicitly rejected three weeks ago.

Because the graph is persistent and per-repo, we use it as the agent's long-term memory. The `knowledge` tool stores arbitrary decisions, patterns, rejections, and links them to code entities:

```
knowledge(action="save",
  title="Why we use Arc<RwLock> in session store",
  content="Mutex caused deadlocks under burst load. See issue #412.",
  kind="decision",
  tags=["status:done", "concurrency"])

knowledge(action="link",
  from="Arc<RwLock> decision",
  to="session::Store",
  relation="implemented_by")
```

Next session, when the agent opens `session.rs`, codescope tells it: "there's a pinned decision about Arc<RwLock> here." No more re-explaining. No more "hey Claude, remember we talked about..."

And because it's a graph, `knowledge_search` can find things by tag, content, OR relationship. "What decisions did we ship in v0.7?" is a one-line query. "What's still planned?" same. "Which code implements this pattern?" walk the `implemented_by` edges.

We also let the graph span projects. A `knowledge(action="save", scope="global", ...)` writes to `~/.codescope/db/_global/` — a DB that any project can search. Patterns, conventions, architectural decisions that transcend a single repo live there. Your second brain, but actually shared across everything you work on.

---

## Delta-mode context: the thing that saved our token budget

One more trick worth mentioning because it's the kind of thing you only realize you need after using the system for real.

The most common query in any AI coding session is "show me what's in this file." The agent calls `context_bundle(file_path)`, we return a structured map: functions, callers, imports, past decisions. First call: maybe 2,000 tokens.

Here's the thing: the agent calls this *multiple times per session* on the same file. The second, third, fourth calls return the exact same data unless the file changed. We were spending 8,000 tokens to say "yeah, nothing changed."

Now we cache per-session. On the second call, if the output would be identical, we return a single line: "Context: foo.rs (UNCHANGED). 8 functions, same callers, same imports." If it changed, we return only the diff — the added/removed functions.

Token savings on repeat calls: 97%. No code changes needed by the agent. It still calls `context_bundle` the same way. The backend just got smart about what's actually different.

The same pattern generalizes. For large tool outputs (`impact_analysis` at depth 5, `explore` on a high-fan-in function), we archive the full output locally and return a summary + retrieval ID. If the agent needs the full data, it calls `retrieve_archived(id)`. Most of the time it doesn't. 95% of tokens saved on the long-tail queries.

None of this is groundbreaking. It's a small delta (no pun intended) on top of existing tools. It adds up.

---

## Auto-reindex: the file watcher you forgot you needed

Someone writes code. The graph goes stale. The agent's next answer is based on the world as it was five minutes ago.

Fine in theory. Catastrophic in practice. You refactor, agent suggests the old signature, you apply it, test fails, agent "fixes" it with more of the old signature. Death spiral.

We start a file watcher when the MCP server boots. Debounced 2s — no thrashing on save-save-save. For each changed file, hash it, skip if identical to what's indexed, otherwise reparse only that file and swap its entities in the graph.

Most changes reindex in milliseconds. The graph is always fresh by the time the agent asks. The agent doesn't know the watcher exists. That's the point.

---

## Honest limitations

This isn't a sales pitch. Here's what codescope doesn't do well:

**Natural language questions that are genuinely semantic.** "Where's the user-facing error handling?" might not match any tag, any function name, any comment. Embeddings still win there. We run `fastembed-rs` as a secondary index for `semantic_search`, but it's a fallback, not the main act.

**Languages we don't have a parser for.** 47 is a lot, but not everything. Mojo, Gleam, Odin — not supported yet.

**Dynamic dispatch that isn't visible in static code.** If you call a function through a heavy reflection layer, runtime plugin loader, or a string lookup in a hashmap, the graph can't see it. Nobody's graph can. You need runtime tracing for that class of problem.

**Very large monorepos.** Graphs with 2M+ entities work but get slow on some queries. We added auto-clustering (group by top-level folder, collapse dense clusters) to keep the 3D viz usable, but the query layer on mega-repos is still a rough edge.

**The first index.** On a large repo (tokio, FastAPI) the first `codescope init` takes 1–2 minutes. Incremental reindex after that is milliseconds, but the first hit is real.

Trade-offs. We made them on purpose. Your mileage will vary.

---

## Benchmarks

Seven repos, five languages. Same questions, measured twice — traditional RAG-style (read files, stuff in context) versus graph traversal.

| Question | Traditional | Codescope | Saved |
|---|---:|---:|---:|
| Find function + callers | 148K tok | 542 tok | 99.6% |
| List all structs | 1.4M tok | 1.2K tok | 99.9% |
| Impact of changing X | 142K tok | 278 tok | 99.8% |
| Largest 20 functions | 454K tok | 289 tok | 99.9% |

Query latency across all seven projects: p50 between 0.3 ms and 4 ms. 3-hop transitive traversal at 50K entities: ~1 ms.

Token numbers aren't a parlor trick. They're the bill you pay Anthropic or OpenAI every month. Drop 99.6% of it and suddenly everything else about how you interact with AI code tools changes.

---

## The protocol thing

I want to end on a point that's maybe more important than any specific feature.

Codescope is not an editor. It's not an agent. It doesn't compete with Cursor or Claude Code. It's a **context layer** — the brain behind whatever tool you already use.

- Claude Code connects via MCP (stdio).
- Cursor connects via MCP (they added support).
- Any LSP-speaking editor (VS Code, Zed, Neovim, Helix) connects via our LSP bridge. Your "Go to Definition" becomes graph-backed with no editor extension.
- You can even use it from a shell script: `codescope search "parse" --mode fuzzy`.

The point is: your agent choice is orthogonal to your context layer choice. You should be able to pick Claude Code because you like its UI, and layer a graph-first brain underneath, regardless of what Anthropic ships in their built-in retrieval.

Embeddings-first RAG became the default because everyone solved the same problem the same way at the same time. That's not a permanent state of affairs. The way we query knowledge shouldn't be locked to whoever sells you the editor.

A graph of your code should belong to you, run locally, and be usable from any tool. That's what codescope is.

---

## Try it

```bash
# Linux/macOS:
curl -fsSL https://raw.githubusercontent.com/onur-gokyildiz-bhi/codescope/main/install.sh | bash

# Windows:
irm https://raw.githubusercontent.com/onur-gokyildiz-bhi/codescope/main/install.ps1 | iex

# Then:
cd your-project
codescope init
```

Restart Claude Code. Or Cursor. Or open VS Code with codescope as an LSP server. Ask it "who calls `main` in this repo?" and watch the response come back in a millisecond with the real answer.

If it saves you an afternoon of context-switching, star the repo. That's what keeps this project going.

---

### Repo
[github.com/onur-gokyildiz-bhi/codescope](https://github.com/onur-gokyildiz-bhi/codescope) — MIT licensed, fully local, 32 MCP tools, 47 languages.

### Further reading
- Karpathy's LLM Wiki pattern — the inspiration for treating the knowledge graph as the product
- Graph of Skills (ICLR 2026) — Personalized PageRank over typed graph edges for tool retrieval
- Sourcegraph's SCIP format — the precursor to what we do, exposed differently
- Anthropic's MCP specification — the protocol that makes this plug-and-play

### Notes
Written after shipping v0.7.7. Benchmarks measured on an M2 Max; your machine may be faster or slower, but the ratios hold.
