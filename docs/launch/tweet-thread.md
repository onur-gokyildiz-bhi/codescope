# Launch tweet thread — draft

**Status:** draft. Final review before launch day. Plan to post roughly 2 hours after the HN post goes up so traffic sources don't collide.

---

## Thread

**1 / 11**
Most AI code context tools are embeddings-first: chunk your code, embed as vectors, nearest-neighbor lookup.

Vectors are great for "find code that *means* X" but they can't tell you "if I change this function, what breaks three hops out?"

That's a graph question. Here's what I built →

**2 / 11**
Codescope: Rust-native code knowledge engine.

tree-sitter → SurrealDB knowledge graph → 52 MCP tools for AI agents.

Graph as the primary index. Embeddings as a secondary fallback for fuzzy queries.

Fully local. Single binary. MIT. 👇

**3 / 11**
The differentiator is multi-hop traversal that vector DBs can't do at all.

Benchmarks on real repos (ripgrep, axum, tokio, gin) with dynamically-picked high-fan-in targets:

3-hop transitive impact analysis in **0.5–1.3 milliseconds**, regardless of repo size.

**4 / 11**
Why sub-millisecond at any size?

Because native graph traversal walks indexed edges in a single statement. The equivalent WHERE-filter on the calls table scales linearly (40ms → 118ms across 11k–45k edges).

Multi-hop is actually *faster* than single-hop with WHERE. Counterintuitive but here are the numbers ↓

**5 / 11**
[Screenshot of BENCHMARKS.md "Graph-First Multi-Hop Traversal" table]

Same data, four open-source repos, dynamic targets. ripgrep 0.92ms. axum 0.52ms. tokio 0.49ms. gin 1.29ms.

This is the property that makes graph-first viable for agents: bounded by graph fan-out, not corpus size.

**6 / 11**
Try one of these questions on your own codebase through Claude Code / Cursor / Zed:

• "What functions transitively depend on parse_config?"
• "If I change User::email, what breaks?"
• "Show me the type hierarchy of Matcher."

Your agent calls impact_analysis, type_hierarchy, context_bundle. You get answers in milliseconds.

**7 / 11**
Install is one line:

```bash
curl -fsSL https://raw.githubusercontent.com/onur-gokyildiz-bhi/codescope/main/install.sh | bash
```

Then `cd your-project && codescope init`. That writes .mcp.json and exposes 52 code intelligence tools to any MCP-aware agent.

No cloud calls. No telemetry. No signup.

**8 / 11**
Things that delighted me while building this:

• The SurrealDB team's native graph syntax (`<-calls<-function<-calls<-function.name`) walks indexed edges like one big optimized traversal
• rmcp crate makes MCP servers trivial in Rust
• Binary quantization gives you 32x memory-efficient semantic search with ~97% accuracy

**9 / 11**
Things that bit me:

• I shipped a "6.4 million× speedup" claim that turned out to be a SurrealQL parse error silently swallowed. Fixed. The raw_query tool now documents the correct syntax and warns you away from the anti-pattern. Humbling.
• "main" is the worst target for impact analysis (it's the ROOT of the call graph). The bench now dynamically picks the highest fan-in function.

**10 / 11**
Inspired by @iamleonie's agentic search workshop which covers 3 retrieval patterns (semantic, DB query, shell+grep).

Codescope is the missing 4th pattern: **graph traversal**. None of the other three can answer "who transitively depends on X". That's the wedge.

Workshop: github.com/iamleonie/workshop-agentic-search

**11 / 11**
Solo maintainer, OSS, MIT licensed. Repo, docs, benchmarks, quickstart:

github.com/onur-gokyildiz-bhi/codescope

Feedback, issues, PRs welcome. Language coverage is at 59 formats today (47 tree-sitter + 12 content parsers) — if yours is missing and has a tree-sitter grammar, adding it is usually a weekend.

Thanks for reading 🙏

---

## Alt versions

**Short version (for 280-character reply/repost):**

Built codescope: Rust code graph for AI agents. 3-hop impact analysis in <1ms across repos from 11k–45k edges. Sub-ms regardless of size because it walks indexed edges, not scans them. 52 MCP tools. Single binary. MIT. github.com/onur-gokyildiz-bhi/codescope

**Ultra short (for BlueSky / Mastodon):**

codescope — graph-first code intel for AI agents. Sub-millisecond multi-hop impact analysis your vector DB can't do. Rust, SurrealDB, 52 MCP tools. github.com/onur-gokyildiz-bhi/codescope

---

## Hashtags (use sparingly)

Primary: #rustlang #opensource
Secondary (for code + AI audience): #AIengineering #ClaudeCode #MCP
Tertiary (sparingly): #graphdb #SurrealDB

Do not spam all five. Pick 2-3 max per tweet.

---

## People to tag (soft pings, NOT in the main thread)

- @iamleonie — workshop inspiration, tagged in tweet 10
- @surrealdb — they'll likely quote-tweet since we use their native graph syntax
- @AnthropicAI / @claude_code — MCP integration, soft ping in a follow-up reply
- @tree_sitter — parsing layer acknowledgment

Post thread first, then a reply chain tagging each of these with a specific thank-you.
