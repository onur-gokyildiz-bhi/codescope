---
name: lint-all
description: Run full lint + format + test + frontend build. Pre-commit / pre-release check.
---

# /lint-all

Run the complete quality gate locally. Everything CI checks plus a few extras.

## When to invoke

- Before any commit (especially before push)
- Before `/ship` (release workflow depends on it being clean)
- When CI fails and you want to reproduce locally

## Steps

```bash
# 1. Format (auto-fix in place)
cargo fmt --all

# 2. Clippy with strict warnings
cargo clippy --workspace -- -D warnings

# 3. Run all tests
cargo test --workspace

# 4. Build release binaries (catches release-only errors)
cargo build --release

# 5. Frontend build
cd crates/web/frontend && npm run build && cd ../../..

# 6. Check there are no unstaged changes (means fmt fixed something)
git diff --exit-code
```

## Interpretation

| Result | Meaning |
|---|---|
| All green, `git diff --exit-code` returns 0 | Ready to commit |
| fmt fixed something | Commit the formatting changes |
| clippy warning | Fix before commit (never allow warnings) |
| test failure | Stop; investigate |
| npm build warning about chunk size | Expected for three.js bundle, ignore |

## Common failures

- **`unnecessary_min_or_max`** — clippy suggests removing `.max(0)` on unsigned types
- **`ptr_arg`** — clippy prefers `&Path` over `&PathBuf` in function args
- **`needless_borrow`** — remove unnecessary `&` in refs
- **fmt on Windows** — CRLF warnings are normal; the line ending change is handled by `.gitattributes`

## Codescope-first rule

When clippy complains about a function:
- `search(mode="exact", query=<function_name>)` to see it in the graph
- Don't Read the whole file just to see the complaint location
