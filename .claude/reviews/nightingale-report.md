# Nightingale's Install & Recovery Audit — 2026-04-14

> "A good error message is a patient who describes their own symptoms."
> Layer: `install.sh`, `install.ps1`, `setup-claude.*`, `crates/cli/src/db.rs`, `crates/cli/src/commands/doctor.rs`.

## Install scripts

| Script | Robustness | Notes |
|---|---|---|
| `install.sh` | PASS | ERR trap at line 13 + `set -eo pipefail`; MINGW/MSYS/CYGWIN detection (line 57) redirects to `install.ps1`; curl failure shows URL + 200-char response snippet; nested-tar binary search via `find`; pgrep/pkill running processes before overwrite; PATH auto-add with shell-rc detection (bash/zsh/fish/profile). Darwin x86_64 gives an explicit "not available, build from source" message. |
| `install.ps1` | PARTIAL | `$ErrorActionPreference = "Stop"` in place; arch check rejects ARM64 with an error; asset-name mismatch shows the full asset list; Stop-Process before overwrite. BUT: no global trap/try-catch wrapper — any exception past `Expand-Archive` dumps raw PowerShell stack to user. No verbose error on `Invoke-WebRequest` download failure (just rethrows). No verification that binary actually launched (`--version` is called but its output is only printed, not validated as non-empty). No nested-zip search — assumes flat layout at `$tempExtract\$bin`. |
| `setup-claude.sh` | PASS | Uninstall mode covers 6 steps (binaries, skills, `~/.claude.json`, `~/.codex.json`, rules, marketplace plugin, DB). Marketplace conflict detection at line 296 skips global MCP when `extraKnownMarketplaces.codescope` present. Interactive prompt when existing install detected. `set -euo pipefail`. |
| `setup-claude.ps1` | PARTIAL | Uninstall parity with .sh. Marketplace detection via regex at line 292 (looser than the .sh's double-grep). BUT: no `--auto` / non-interactive fallback — if `Read-Host` has no TTY (piped via `iex`), the script hangs. `$args` doesn't work when run via `irm | iex` (args lost through the pipe), so `--uninstall` is effectively unreachable via the one-liner. Also: skill installer doesn't check `Invoke-WebRequest` return — 404 on a SKILL.md leaves a zero-byte file with no warning. |

## Error messages

Counted user-facing error sites (install + setup + db + doctor):

- Messages WITH actionable "Fix"/next-step: **18 / 22**
- Offenders (no actionable next step):
  - `install.ps1:18` — "Only x86_64 Windows is supported" — tells user what's wrong, no fix. Should say: "Build from source: `git clone … && cargo build --release`" or point at ARM64 tracking issue.
  - `install.ps1:75` — `Invoke-WebRequest` download has no wrapping try/catch with URL + hint.
  - `install.ps1:81` — `Expand-Archive` failure is bare.
  - `setup-claude.ps1:81, 122, 272` — `catch { Write-Host "Could not parse … manually" }` tells the user *that* it failed but doesn't show which key, which line of JSON, or offer `jq`-equivalent command. No "open in notepad at line X" hint.
  - `db.rs:128` (generic DB failure branch) — says "rm -rf {path}" which is DESTRUCTIVE and not reversible; should suggest `mv {path} {path}.bak` first (recoverable), then retry.

## Recovery paths

- **Auto-lock-recovery (`db.rs:try_remove_stale_lock`)**: INTACT. Logic is:
  1. LOCK file exists AND `pgrep -f codescope` returns no process → remove LOCK, retry (line 29–43).
  2. LOCK + live process → bail with 2-option fix (re-index via running MCP, OR `pkill && codescope init`).
  3. Other error → generic bail with `rm -rf` suggestion.
  One GAP: `is_codescope_running()` calls `pgrep` which is **Unix-only**. On Windows `pgrep` fails → `unwrap_or(false)` → code assumes process is dead → **would remove a legitimately-held lock on Windows**. This is a latent footgun. See Red flags.
- **`codescope doctor`**: PRESENT and solid. 7 checks (binary, .mcp.json, rules, CLAUDE.md, DB, stale processes, .gitignore). `--fix` flag works for rules + .gitignore. Windows `\\?\` extended-length prefix stripped (line 45). `check_stale_processes` has a Windows branch using `tasklist`. Every failing check has a `fix:` hint.
- **Uninstall mode**: PRESENT in both .sh and .ps1. Removes binaries, skills, MCP entries, rules, marketplace plugin, optionally DB (with y/N prompt).

## Platform coverage

- Linux x86_64 (`x86_64-unknown-linux-gnu`): PASS — explicit target in `install.sh:35`.
- Linux aarch64 / DGX Spark: PASS — `install.sh:36-37` handles both `aarch64` and `arm64` uname outputs, maps to `aarch64-unknown-linux-gnu`.
- macOS ARM (`aarch64-apple-darwin`): PASS — `install.sh:53`.
- macOS Intel (`x86_64-apple-darwin`): **NOT SUPPORTED** — explicit block at `install.sh:43-51` with the message "ONNX Runtime does not provide x86_64-apple-darwin builds" and build-from-source fallback. Clear and actionable, but cuts off a nontrivial user base.
- Windows MSVC (`x86_64-pc-windows-msvc`): PASS — `install.ps1:21`.
- Windows ARM64: BLOCKED — `install.ps1:15-19` rejects non-X64.
- WSL: Treated as Linux (uname reports Linux). Works for x86_64. Edge case: WSL users who have Windows Claude Code installed will get skills dropped into `/home/<user>/.claude/skills/` but Claude Code reads from `%USERPROFILE%\.claude\skills\` — **skill install goes to the wrong home**. No detection code for WSL in either script.
- Git-Bash / MINGW on Windows: PASS — install.sh detects and redirects to .ps1 (line 57–64).

## Red flags

1. **`is_codescope_running()` is pgrep-only** (`db.rs:17-25`). On Windows it silently returns `false`, meaning `try_remove_stale_lock` will nuke a lock held by a live process. Needs a `tasklist`/`wmic` branch matching what `doctor.rs::check_stale_processes` does. High-impact footgun for Windows users running MCP + a parallel `codescope index` or `codescope doctor`.
2. **`pkill -f codescope` suggestion in db.rs:111 is Unix-only.** Windows users hitting the "live MCP" branch get no usable command. Add `taskkill /F /IM codescope-mcp.exe` in the same message.
3. **`install.ps1` lacks a global `try { … } catch` wrapper.** A failure mid-script (network blip, antivirus locking the tempZip, ACL denial on `%USERPROFILE%\.local\bin`) dumps a raw CLR stack. The .sh equivalent has the ERR trap that explains + offers build-from-source.
4. **`setup-claude.ps1` `$args` don't survive `irm | iex`.** Documentation says `irm … | iex` then `setup-claude.ps1 --uninstall`, but via the pipe, positional args aren't bound. Doc needs updating or script needs a $env:CODESCOPE_UNINSTALL check.
5. **`setup-claude.ps1` hangs on `Read-Host` when non-interactive.** `.sh` guards with `[ -t 0 ]`; PS1 has no equivalent `[System.Console]::IsInputRedirected` check.
6. **Skill downloader silently accepts 404 bodies.** Both scripts use curl/Invoke-WebRequest with 2>$null or 2>/dev/null, so a missing SKILL.md writes an HTML error page to disk and the verification step still passes (it only counts files).
7. **`install.sh:173` PATH check uses `[[`** (bashism) — works because shebang is `#!/bin/bash`, but if a user pipes to `sh` on Alpine/Debian it'd fail silently. Low priority.
8. **macOS Intel abandonment may be too aggressive.** Users on older Mac hardware can still run code-signed binaries via Rosetta if we shipped x86_64 with a CoreML backend. Worth documenting a Rosetta path or tracking issue in the error message.
9. **Doctor doesn't probe DB lock directly.** `check_stale_processes` reports counts, but doesn't actually attempt a read-only open of the DB to prove it's acquirable. A user could have 5 codescope processes and still have a healthy DB — or 0 processes and a stale lock. Doctor should try `surrealdb::Surreal::new` with a 200ms timeout on the DB path.
10. **`doctor --fix` doesn't regenerate `.mcp.json`** despite the agent description promising it does. Line 85 has a `// Will be handled by codescope init later` comment — the fix is not wired.
11. **`install.sh:150` pkill runs unconditionally before install.** No confirmation prompt — user-invoked `codescope query` or a long-running index gets killed silently. Consider `pkill` only for `codescope-mcp`, not `codescope` broadly.

## Action items

1. [P1] Add Windows branch to `is_codescope_running()` using `tasklist /FI "IMAGENAME eq codescope*"` (or reuse `doctor.rs::check_stale_processes` logic). Without this, auto-lock-recovery on Windows can corrupt a live DB.
2. [P1] Expand the "live MCP" error in `db.rs:101-113` to include `taskkill` command for Windows users.
3. [P2] Wrap `install.ps1` body in `try { … } catch { Write-Host "Install failed at step X. Build from source: …" }`.
4. [P2] Replace `setup-claude.ps1` `$args` parsing with `$env:CODESCOPE_UNINSTALL` env-var fallback so `irm | iex` path works. Document both.
5. [P2] Guard `Read-Host` calls in setup-claude.ps1 with `if ([System.Console]::IsInputRedirected) { $choice = "7" } else { Read-Host … }`.
6. [P2] Wire `doctor --fix` to regenerate `.mcp.json` (call the same code path as `codescope init`).
7. [P2] Validate downloaded SKILL.md files are non-zero and don't start with `<!DOCTYPE html` — fail the install if any asset 404'd.
8. [P3] Add live-DB probe to `doctor` (try to open DB for read, report lock state directly).
9. [P3] Change generic DB error fix from `rm -rf` to `mv {path} {path}.bak` (recoverable).
10. [P3] Add WSL detection in install.sh (check `/proc/version` for "microsoft") and warn if skill dir might need `$env:USERPROFILE` mapping via interop.
11. [P3] Tighten `install.sh:150` pkill to target only `codescope-mcp` and `codescope-web`, not plain `codescope` (user might be running it interactively).
12. [P3] Improve PS1 JSON-parse catch blocks — show the offending key and suggest opening the file in notepad.
13. [P3] Consider shipping x86_64-apple-darwin via an alternate backend (or at minimum, document Rosetta workaround in the `install.sh:43` error).
