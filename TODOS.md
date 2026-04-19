# TODOS

Deferred work. Each entry: **What / Why / Pros / Cons / Depends / Context**.

Captured during active sessions so ideas don't evaporate. Check the heading
dates to gauge staleness — anything 4+ weeks old should be re-evaluated
before picking up.

---

## From RTK study — 2026-04-19

Inspired by [rtk-ai/rtk](https://github.com/rtk-ai/rtk). RTK is complementary
(compresses bash output) not competing — same mission, different angle.
These items defer launch-prep polish; nothing blocks R1–R6.

### TODO-RTK-01: Multi-agent init (`codescope init --agent <name>`)

- **What:** Extend `codescope init` to generate the right config for Cursor,
  Windsurf, Cline, Kilocode, Antigravity, Copilot, Gemini, Codex — not just
  Claude Code. Each agent has its own MCP config file format/location.
- **Why:** Launch reach. Today codescope only installs for Claude Code
  users; everyone else writes config by hand.
- **Pros:** Reach multiplier for OSS launch. Directly addresses "does it
  work with my tool?" friction.
- **Cons:** Maintenance: 8 config shapes to track. Each agent's MCP support
  evolves on its own calendar.
- **Depends:** R5 (path-based MCP routing) — the HTTP URL shape varies by
  agent, and `/mcp/{repo}` needs to be stable first.
- **Context:** RTK's `src/main.rs` has `AgentTarget` enum + per-agent
  template files under `hooks/{claude,cursor,windsurf,cline,kilocode,
  antigravity,opencode,codex,copilot}`. Good reference layout.

### TODO-RTK-02: `codescope gain` — cumulative token-savings counter

- **What:** Every tool call writes a small analytics record (tool name,
  tokens_in, tokens_out_before_compression, tokens_out_after). `codescope
  gain` reads the log, shows "since install you saved X tokens / $Y at
  Claude Opus rates".
- **Why:** Dopamine + marketing. Users share screenshots → organic
  growth. Also real data: which tools actually save? Where's the tail?
- **Pros:** Tangible proof of value. Feeds the launch pitch.
- **Cons:** Privacy question — store locally only (mirror RTK), never
  phone home. Disk footprint.
- **Depends:** none technically; best paired with R2 error contract so
  tool outputs have stable shapes to measure.
- **Context:** RTK has `analytics/` module. Similar disk layout under
  `~/.codescope/analytics/gain.jsonl` with one record per call.

### TODO-RTK-03: Bash-hook transparent rewrite

- **What:** Ship a Claude Code `bashPre` hook that rewrites common Read /
  Grep invocations into their codescope MCP equivalents automatically.
  User types `cat src/lib.rs` → hook silently runs `codescope read
  src/lib.rs` (returns context_bundle-style compressed view). Claude
  never sees the rewrite.
- **Why:** We currently tell users "prefer codescope tools over Read/Grep"
  via CLAUDE.md rules. A hook enforces it mechanically. Same pattern
  RTK uses.
- **Pros:** Removes the "remember to use the right tool" burden. Zero
  cognitive cost.
- **Cons:** Surprising the user with silent rewrites. Some commands
  shouldn't be rewritten (e.g. `cat log/output.txt` for raw dump).
  Need a conservative allowlist + escape hatch.
- **Depends:** Full MCP tool surface stable (most already is).
- **Context:** RTK `hooks/claude/` — each hook is a small JSON config.
  Mirror layout for codescope hooks per supported agent.

### TODO-RTK-04: Localized READMEs (fr, zh, ja, ko, es, tr)

- **What:** Translate README into 5–6 languages. Maintain via side files
  `README_fr.md`, etc., linked from the top of the English README.
- **Why:** OSS global reach. GitHub search + HN comments in non-EN
  communities.
- **Pros:** Measurable traffic bump in non-EN markets. Low ongoing cost
  (translate on each major release, not per PR).
- **Cons:** Translation drift — old non-EN versions linger as English
  evolves. Mitigate with a single "last updated" line and a pointer to
  canonical English.
- **Depends:** English README stable (post R1–R6).
- **Context:** RTK ships `README_{fr,zh,ja,ko,es}.md`. Turkish is free
  for us (Onur).

### TODO-RTK-05: Homebrew formula

- **What:** Submit a Homebrew formula for codescope + surreal bundle so
  macOS users can `brew install codescope`.
- **Why:** macOS Claude Code users are the largest OSS segment. Today
  we ask them to run `curl ... | sh` (friction).
- **Pros:** One-command install. Homebrew community is actively
  curated — passing review also signals legitimacy.
- **Cons:** Formula review cycles can take weeks. Need to keep the
  formula in sync with release cadence.
- **Depends:** R8 (release bundling) — formula needs a stable release
  artifact layout to depend on.
- **Context:** RTK ships a `Formula/` directory in-repo, which is the
  standard Homebrew tap pattern. Mirror it.

### TODO-RTK-06: `--ultra-compact` / `--compact` output mode

- **What:** Add a global flag that makes every codescope CLI command
  (and optionally MCP tool output) emit the most token-dense form —
  ASCII icons instead of emoji, inline instead of table, truncated
  body fields. Useful for low-budget LLM contexts.
- **Why:** Even graph-native output has noise; a compact mode saves
  another 30–50 % on top of the current wins.
- **Pros:** Composable with the graph approach (our savings × RTK's
  savings). Cheap to implement per-tool.
- **Cons:** Two output paths to test per tool.
- **Depends:** None.
- **Context:** RTK has `--ultra-compact` as a global CLI flag. Same
  ergonomics.

---

## From main arc — 2026-04-18

### TODO-PHASE3: Dream feature scoping

- **What:** Phase 3 "Dream" — offline background agent that replays
  ingested conversations + knowledge + graph into refined knowledge
  entities during idle time.
- **Why:** Leverage the 867 JSONL chat transcripts already in the DB.
  Differentiates codescope from every other code-context tool.
- **Depends:** Autoresearch agent output (still running at time of
  write). R1–R6 stability.
- **Context:** `project_phase2_phase3.md` in memory. Architecture
  still open.
