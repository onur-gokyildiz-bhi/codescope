---
name: ship
description: Cut a release — bump version, update CHANGELOG, tag, push, create GitHub release, install local binaries.
---

# /ship

Full release workflow. Use when the user says "ship", "release X.Y.Z", "tag and push", or after a sprint concludes.

## When to invoke

- All P0/high-priority planned work is `status:done` in the knowledge graph
- `cargo clippy --workspace -- -D warnings` clean
- `cargo fmt --all -- --check` clean
- `cargo test --workspace` green (or explicitly waived)
- Tool count ≤ 40

## Protocol

1. **Pre-flight (hand off to Ada for signoff):**
   - Run clippy, fmt check, test
   - `grep -c '#\[tool(' crates/mcp-server/src/tools/*.rs | awk -F: '{s+=$2}END{print s}'`
   - If any check fails, stop and report

2. **Version bump:**
   ```bash
   # Edit workspace Cargo.toml version = "X.Y.Z"
   cargo check --workspace  # updates Cargo.lock
   ```

3. **Commit:**
   ```bash
   git add Cargo.toml Cargo.lock
   git commit -m "release: vX.Y.Z — <one-line summary>"
   ```

4. **Tag and push:**
   ```bash
   git tag -a vX.Y.Z -m "vX.Y.Z: <summary>"
   git push origin main vX.Y.Z
   ```

5. **GitHub release:**
   ```bash
   gh release create vX.Y.Z \
     --title "vX.Y.Z — <title>" \
     --notes "<markdown notes with features, fixes, breaking changes, migration>"
   ```

6. **Local binary install:**
   ```bash
   taskkill /F /IM codescope.exe 2>&1 | head -2  # Windows
   # or: pkill -f codescope                       # Unix
   cargo build --release
   # Copy all 4 binaries to ~/.local/bin/
   ```

7. **Knowledge entry:**
   ```
   knowledge(action="save",
     title="Release vX.Y.Z",
     kind="decision",
     tags=["status:done", "release", "shipped:YYYY-MM-DD", "vX.Y.Z"])
   ```

8. **Update planned items to done:**
   Update knowledge entries that were `status:planned` and shipped in this release — replace tag with `status:done` + `shipped:YYYY-MM-DD` + `vX.Y.Z`.

## Semver

- **patch (X.Y.Z+1)**: bug fix only, no new MCP tools, no tool description changes
- **minor (X.Y+1.0)**: new features, tool additions, backwards-compatible
- **major (X+1.0.0)**: breaking changes to MCP/LSP/CLI surface, tool removals or renames

## Guardrails

- Never `--force` on tag push
- Never skip CHANGELOG for minor/major releases
- Never release if `cargo test --workspace` fails (waive explicitly in commit message if needed)
- Never break `.mcp.json` compatibility without clear migration

## Codescope-first rule

- `knowledge(action="search", query="status:planned priority:high")` before starting — confirm scope
- `knowledge(action="search", query="shipped:YYYY-MM-DD")` after — confirm you didn't miss anything
