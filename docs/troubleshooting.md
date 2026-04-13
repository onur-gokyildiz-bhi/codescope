# Troubleshooting

Common issues and their fixes. If nothing here helps, open a GitHub issue with the [reproduction details](../CONTRIBUTING.md#filing-issues).

## Install

### `codescope: command not found` after running the install script

The installer drops binaries into `~/.local/bin`. That directory is on your `$PATH` by default on most shells but not all. Add this to your shell rc file:

```bash
export PATH="$HOME/.local/bin:$PATH"
```

On Windows the installer drops binaries into `%USERPROFILE%\.local\bin`. Add that directory to your system PATH via System Properties → Environment Variables. Restart your terminal afterwards.

Verify:

```bash
which codescope
# or on Windows: where codescope
```

### Install script fails with a 404

The install script fetches binaries from the latest GitHub release. If the 404 mentions a specific filename like `codescope-v0.5.0-aarch64-unknown-linux-gnu.tar.gz`, your architecture is supported but the release may not have finished uploading. Retry in a minute. If the 404 is generic, you may be on a very new platform we don't yet build for — open an issue and we'll add it to the release matrix.

### Build from source: linker errors on Windows

You need the **MSVC toolchain**, not MinGW:

```powershell
rustup default stable-x86_64-pc-windows-msvc
```

Then install the Visual Studio 2022 Build Tools ("Desktop development with C++" workload). Restart your terminal.

### Build from source: `openssl-sys` fails on Linux

Install the dev headers:

```bash
sudo apt-get install -y pkg-config libssl-dev
```

On macOS with Homebrew:

```bash
brew install openssl@3 pkg-config
export OPENSSL_DIR=$(brew --prefix openssl@3)
```

## Indexing

### `codescope init` hangs or crashes part-way through

Indexing walks every file in the directory tree, respecting `.gitignore`. Very large repos with generated code (node_modules not in gitignore, build artifacts, minified JS) can balloon the file count.

First, check what's being seen:

```bash
codescope index . --repo myrepo --dry-run 2>&1 | head -20
```

If you see thousands of files from `node_modules` or `target/`, add them to `.gitignore`. Codescope respects gitignore by default.

If the crash is specific to one file, find it in the log (`RUST_LOG=codescope=debug codescope init`) and open an issue with the filename and language — a tree-sitter parser may be choking on unusual syntax.

### Indexing is unusually slow

The bench tool uses single-row inserts as a worst-case baseline. The `codescope-mcp` pipeline batches and is materially faster. If you're comparing numbers, make sure you're running the same path.

If real indexing is slow:

- **Windows Defender** scans every file codescope writes into the DB. Add an exclusion for `~/.codescope/db` in Windows Security → Virus & threat protection → Exclusions.
- **Encrypted filesystem** (ecryptfs, FileVault on some paths) slows writes significantly. The SurrealKV backend is write-heavy during indexing.
- **Network-mounted home directory** — DO NOT put `~/.codescope/db` on a network mount. Use a local disk.

### Indexing finishes but `graph_stats` shows zero calls edges

This means tree-sitter parsed your files but the call extractor did not find any function calls. Usually this is one of:

- The language isn't fully supported yet. Run `codescope languages` to see the support list.
- The language is supported but your code uses an unusual calling pattern that the extractor missed. Open an issue with a minimal reproduction; we tune extractors based on real-world examples.

## Queries

### `Parse error: Unexpected token...`

SurrealQL has a few gotchas:

1. **`function` is a reserved word.** Always wrap it in backticks:
   ```sql
   SELECT * FROM `function` WHERE name = 'main'
   ```
2. **Multi-hop graph traversal chains directly.** Do not put a dot between hops:
   ```sql
   -- Wrong (parse error)
   SELECT <-calls<-`function`.<-calls<-`function`.name FROM `function`
   -- Right
   SELECT <-calls<-`function`<-calls<-`function`.name FROM `function`
   ```
   The dot is only for the final field projection.
3. **Use parameterized bindings** for user input. Do not string-interpolate.

### Query returns empty array when you expect results (empty-graph)

Before assuming the query is wrong, verify the graph actually has the data:

```bash
codescope query "SELECT count() FROM calls GROUP ALL"
codescope query "SELECT count() FROM \`function\` GROUP ALL"
```

If `calls` is zero but `function` is non-zero, the call extractor did not run — re-index with `codescope index . --repo <name>` and watch the output for the "Calls edges: N" line at the end.

If both are zero, indexing did not complete. Check for errors in `codescope init` output.

### `IO error: The process cannot access the file because another process has locked a portion of the file`

SurrealKV holds an exclusive lock on the DB directory. If you see this error, another codescope process is already using it — typically `codescope-mcp` running as an MCP server. Either:

- Stop the MCP server (close the agent that spawned it)
- Or run `codescope-mcp` in daemon mode and have both processes connect to the same daemon

On Windows, the lock can sometimes stick around after a crash. In that case, delete the DB directory and re-index:

```bash
rm -rf ~/.codescope/db/<repo>
codescope index . --repo <repo>
```

## MCP server

### Claude Code doesn't see the codescope tools

1. Verify `.mcp.json` exists in your project root and has a `codescope` entry.
2. Restart Claude Code after creating `.mcp.json` — it only reads the file on startup.
3. Check Claude Code's MCP server logs for startup errors. A common one is "binary not found" if `codescope-mcp` isn't on PATH.

If the `.mcp.json` is missing, re-run `codescope init` in the project root.

### Duplicate tools / double MCP server (marketplace + bash install conflict)

If you installed codescope from both the Claude Code marketplace **and** from the bash install script (`install.sh` / `install.ps1`), you may end up with two MCP server instances running simultaneously. Symptoms:

- Duplicate tool names in the tool list
- Tools failing intermittently (DB lock contention between two instances)
- `IO error: The process cannot access the file because another process has locked a portion of the file`

**Fix — pick one method and remove the other:**

**Option A: Keep marketplace, remove bash MCP config**
```bash
# Remove the global MCP entry (if setup-claude added one)
# Edit ~/.claude.json and delete the "codescope" key from "mcpServers"
# Keep .mcp.json in projects only if the marketplace doesn't auto-configure it
```

**Option B: Keep bash install, remove marketplace**
```bash
# Edit ~/.claude/settings.json and remove the "codescope" key from "extraKnownMarketplaces"
# Keep .mcp.json in your project root (codescope init creates this)
```

The `setup-claude` scripts (v0.7.1+) now detect marketplace installs and skip global MCP config automatically. If you run the setup wizard after installing from the marketplace, it will not create a conflicting global entry.

### Tool descriptions look truncated in the agent

Codescope's tool descriptions use multi-line string literals. If your MCP client truncates them, the client isn't the latest version. Update to a recent Claude Code / Cursor / Zed build.

## Still stuck?

Open a GitHub issue with:

- Your platform (`uname -a` on Unix, `systeminfo | findstr /B /C:"OS"` on Windows)
- `codescope --version`
- The exact command that failed
- The output with `RUST_LOG=codescope=debug` enabled
- A minimal reproduction if possible

Solo maintainer — see [Support Expectations](../CONTRIBUTING.md#support-expectations) for response time guidance.
