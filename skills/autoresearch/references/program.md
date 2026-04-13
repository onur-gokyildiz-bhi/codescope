# Research Program Configuration

Customize these parameters to control the autoresearch loop behavior.

## Search Constraints

- **max_rounds:** 3
- **max_fetches_per_round:** 5
- **max_total_fetches:** 12
- **preferred_sources:** academic papers, official docs, reputable tech blogs
- **avoid_sources:** SEO spam, AI-generated listicles, paywalled content

## Confidence Scoring

| Level | Criteria |
|-------|----------|
| **high** | Multiple independent sources agree, primary source available |
| **medium** | Single reputable source, or two less authoritative sources |
| **low** | Single source, blog post, or unverified claim |

## Filing Rules

- Every source gets its own knowledge node (kind: "source")
- Every entity mentioned in 2+ sources gets a node (kind: "entity")
- Claims that appear in 3+ sources get high confidence
- Contradictions always get flagged (kind: "contradiction")
- Always attempt to link findings to existing code entities

## Stop Conditions

Stop researching when ANY of:
1. All decomposed angles have at least 2 sources each
2. Max rounds reached (3)
3. User interrupts
4. No new information found in last round (diminishing returns)

## Domain-Specific Overrides

If the research topic is about:
- **Security/crypto**: prefer academic papers and RFCs over blog posts
- **Programming languages**: prefer official documentation and specs
- **Architecture patterns**: prefer Martin Fowler, ThoughtWorks, ACM papers
- **Product/company info**: prefer official announcements and SEC filings
