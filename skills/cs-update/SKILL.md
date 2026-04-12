---
name: cs-update
description: Check for Codescope updates and install the latest version. Self-update mechanism.
user-invocable: true
disable-model-invocation: true
---

# Update Codescope

Check for the latest version and update if available.

Steps:

1. Get the current installed version:
   ```bash
   codescope --version 2>/dev/null || codescope-mcp --version 2>/dev/null || echo "unknown"
   ```

2. Check the latest release from GitHub:
   ```bash
   curl -fsSL https://api.github.com/repos/onur-gokyildiz-bhi/codescope/releases/latest 2>/dev/null | grep -o '"tag_name": *"[^"]*"' | head -1 | sed 's/.*"v\?\([^"]*\)"/\1/'
   ```

3. Compare versions. If they match, report "Already up to date" and stop.

4. If an update is available, ask the user for confirmation, then run the appropriate installer:

   **Windows (PowerShell):**
   ```powershell
   powershell -Command "irm https://raw.githubusercontent.com/onur-gokyildiz-bhi/codescope/main/install.ps1 | iex"
   ```

   **Linux / macOS:**
   ```bash
   curl -fsSL https://raw.githubusercontent.com/onur-gokyildiz-bhi/codescope/main/install.sh | bash
   ```

5. Verify the update:
   ```bash
   codescope --version
   ```

6. Report the result:
   ```
   Codescope updated: v0.4.0 -> v0.5.0
   ```
