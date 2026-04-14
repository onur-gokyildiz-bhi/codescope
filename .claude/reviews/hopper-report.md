# Hopper's Release Readiness — 2026-04-14

> "Tagged and pushed is shipped. Everything else is WIP."

## Current state

- **Version (workspace Cargo.toml):** `0.7.7`
- **Last tag:** `v0.7.7` (2026-04-14 19:10 UTC)
- **Commits since tag:** 3
  - `7b7c038` feat: /mcp-test skill — end-to-end MCP server verification
  - `3ca2b5b` feat: codescope agent + skill infrastructure
  - `d5568c1` docs: launch article — 'Stop RAG-ing Your Codebase. Graph It.'

All three are agent/skill/docs artifacts under `.claude/` + `docs/`. No Rust source, no MCP surface changes, no schema changes. **Not release-worthy on their own** — they ride along on the next tag.

## Release cadence snapshot

Last 5 tags all shipped TODAY (2026-04-14):

| Tag | Time (UTC) | Theme |
|---|---|---|
| v0.7.3 | 14:29 | knowledge_search fix + project rules + network access |
| v0.7.4 | 15:20 | File watcher + daemon init + tool consolidation |
| v0.7.5 | 16:14 | CUDA + LSP bridge + consolidation round 3 |
| v0.7.6 | 17:21 | Schema migrations + cross-project knowledge + PR review |
| v0.7.7 | 19:10 | OpenTelemetry + scalable graph clustering |

Five patch/minor releases in ~5 hours. Ship discipline is hot. Just remember: each tag forces users to `/mcp` reconnect.

## CI/CD

- **Release workflow:** present — `.github/workflows/release.yml`, triggers on `v*` tags
- **CI workflow:** present — `.github/workflows/ci.yml`
- **Cross-platform build matrix:**
  - `x86_64-pc-windows-msvc` (windows-latest) → `.zip`
  - `x86_64-unknown-linux-gnu` (ubuntu-latest) → `.tar.gz`
  - `aarch64-apple-darwin` (macos-latest) → `.tar.gz`
  - `aarch64-unknown-linux-gnu` (ubuntu-latest, multiarch cross-compile) → `.tar.gz`
  - `x86_64-apple-darwin` (Intel Mac) — **intentionally excluded** (ort-sys / ONNX Runtime has no prebuilt x86_64 darwin binary). Documented in release notes + CHANGELOG v0.6.1 entry is now stale (claimed Intel Mac was added back).
- **Binaries in each archive:** `codescope`, `codescope-mcp`, `codescope-web` (+ README, LICENSE). **Missing `codescope-lsp`** — release-captain.md step 7 says to install 4 binaries locally, but release.yml only builds 3. Discrepancy: does the `lsp` crate ship via release or only via `cargo install`?
- **Binary artifacts on v0.7.7:** ✅ all 4 target archives + `SHA256SUMS.txt` present

```
codescope-v0.7.7-aarch64-apple-darwin.tar.gz
codescope-v0.7.7-aarch64-unknown-linux-gnu.tar.gz
codescope-v0.7.7-x86_64-pc-windows-msvc.zip
codescope-v0.7.7-x86_64-unknown-linux-gnu.tar.gz
SHA256SUMS.txt
```

## CHANGELOG

- **Exists:** ✅ `C:\Users\onurg\OneDrive\Documents\graph-rag\CHANGELOG.md`
- **Current:** ❌ **SEVERELY STALE**
  - Latest entry: `## [0.7.0] - 2026-04-13`
  - Missing entries: **v0.7.1, v0.7.2, v0.7.3, v0.7.4, v0.7.5, v0.7.6, v0.7.7** — 7 tagged releases have no CHANGELOG section
  - `## [Unreleased]` is empty (no in-flight notes)
  - Release-captain protocol step 3 says "Append user-visible changes to CHANGELOG.md under a new `## [vX.Y.Z] — YYYY-MM-DD` section." That step was skipped for the entire 0.7.x line from .1 onward.

This is the single biggest red flag. Users, distributors, and AI agents rely on CHANGELOG to know what changed; right now the file lies about current state.

## Post-release reconnect flow

- Documented in `.claude/agents/release-captain.md` step 9: "After release, user must `/mcp` reconnect in Claude Code to pick up new binary."
- **Not yet surfaced in release_notes.md** — the generated GitHub release body advertises install and quick-start but does not mention `/mcp` reconnect. Every user who auto-updates from v0.7.6 → v0.7.7 and doesn't reconnect gets a stale tool surface silently.

## Ship-ready?

- **Can cut another release right now:** YES, but nothing to ship. The 3 unreleased commits are agent/docs only — no reason to bump version. Recommend holding until next code-level change.
- **If a release is cut:** it will be **blocked on CHANGELOG** per release-captain protocol. Ada's pre-flight + Hopper's step 3 should refuse to tag until CHANGELOG is backfilled.

## Red flags

1. **CHANGELOG 7 releases behind.** Highest priority. Every release from v0.7.1 onward skipped step 3 of the release protocol. Cannot reconstruct "what shipped in v0.7.4" from the changelog alone — only from commit messages and GitHub release notes.
2. **Five releases in one day** with no apparent cooldown between them. Each forces `/mcp` reconnect. Fine for a solo pre-launch sprint, but if any real users are on v0.7.3 from this morning, they're 4 versions behind already. Worth rate-limiting post-launch.
3. **`codescope-lsp` binary is not shipped in releases.** release-captain.md step 7 references installing it locally from `cargo build --release`, but release.yml only builds `-p codescope -p codescope-mcp -p codescope-web`. LSP users must build from source or the binary exists but isn't wired up. Inconsistency between protocol and workflow.
4. **Intel Mac (`x86_64-apple-darwin`) drifted.** CHANGELOG v0.6.1 claims Intel Mac was added. release.yml has it removed again with comment "ort-sys has no prebuilt binaries for that target." Release notes surface this but CHANGELOG contradicts.
5. **No `/mcp reconnect` reminder in generated release_notes.md.** Users who auto-update get silent tool-surface drift.
6. **No pre-flight automation gate.** release-captain.md step 1 lists clippy/fmt/test/tool-count/schema checks, but none of them are blocking in release.yml — the workflow fires on tag push regardless of CI state. If a broken tag is pushed, binaries will still build and publish.

## Action items

1. **Backfill CHANGELOG.md** for v0.7.1 → v0.7.7. Mine commit messages (`release: v0.7.X — <summary>`) + GitHub release notes. Group into Features / Fixes / Breaking / Internal per protocol. Blocking — do this before the next tag.
2. **Add `/mcp` reconnect reminder** to release_notes.md template in `.github/workflows/release.yml` (new section after Quick Start, before Supported Platforms).
3. **Decide on `codescope-lsp`:** either add `-p codescope-lsp` to the cargo build line in release.yml and bundle the binary, or update release-captain.md step 7 to drop it from the "install 4 binaries" list. Pick one.
4. **Resolve Intel Mac discrepancy:** either re-add `x86_64-apple-darwin` via macos-13 runner (what CHANGELOG v0.6.1 promised), or amend the CHANGELOG v0.6.1 entry to note it was subsequently removed when ort-sys prebuilt binaries were lost.
5. **Pre-flight gate:** add a job in release.yml that runs `cargo fmt --all -- --check` + `cargo clippy --workspace -- -D warnings` + `cargo test --workspace` and is a `needs:` dependency of `build`. Today a broken tag ships regardless.
6. **No new tag until (1) is done.** Current `HEAD` has 3 unreleased commits but none warrant a version bump — safe to hold.
7. **Post-launch:** throttle release cadence. Five tags in one day is a sprint-mode pattern; once external users exist, each tag is a user-visible event and should batch ≥ 1 day of soak time.

---

*Ship the compiler, ship the release. But don't ship a lying CHANGELOG.* — Hopper
