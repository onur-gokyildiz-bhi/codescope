#!/bin/bash
# Codescope Installer for Linux/macOS
# Usage: curl -fsSL https://raw.githubusercontent.com/onur-gokyildiz-bhi/codescope/main/install.sh | bash

REPO="onur-gokyildiz-bhi/codescope"

echo ""
echo "  Codescope Installer"
echo "  ==================="
echo ""

# Error handler — show what failed instead of silent exit
trap 'echo ""; echo "  ERROR: Installation failed at line $LINENO"; echo "  If this persists, build from source:"; echo "    git clone https://github.com/$REPO && cd codescope && cargo build --release"; echo ""' ERR
set -eo pipefail

# Detect install directory
EXISTING=$(command -v codescope 2>/dev/null || true)
if [ -n "$EXISTING" ] && [ -f "$EXISTING" ]; then
    INSTALL_DIR="$(dirname "$EXISTING")"
    echo "  Existing install detected: $INSTALL_DIR"
else
    INSTALL_DIR="${HOME}/.local/bin"
    echo "  Fresh install to: $INSTALL_DIR"
fi

# Detect OS and architecture
OS="$(uname -s)"
ARCH="$(uname -m)"

echo "  Detected: $OS $ARCH"

case "$OS" in
    Linux)
        case "$ARCH" in
            x86_64)  TARGET="x86_64-unknown-linux-gnu" ;;
            aarch64) TARGET="aarch64-unknown-linux-gnu" ;;
            arm64)   TARGET="aarch64-unknown-linux-gnu" ;;
            *) echo "  Error: Unsupported architecture: $ARCH"; exit 1 ;;
        esac
        ;;
    Darwin)
        case "$ARCH" in
            x86_64)
                echo "  Error: Intel Mac (x86_64) prebuilt binaries are not available."
                echo "  ONNX Runtime does not provide x86_64-apple-darwin builds."
                echo ""
                echo "  Build from source instead:"
                echo "    git clone https://github.com/$REPO"
                echo "    cd codescope && cargo build --release"
                echo "    cp target/release/codescope target/release/codescope-mcp ~/.local/bin/"
                exit 1
                ;;
            arm64)  TARGET="aarch64-apple-darwin" ;;
            *) echo "  Error: Unsupported architecture: $ARCH"; exit 1 ;;
        esac
        ;;
    MINGW*|MSYS*|CYGWIN*)
        echo "  Detected Windows (via $OS)."
        echo "  Use the PowerShell installer instead:"
        echo ""
        echo "    irm https://raw.githubusercontent.com/$REPO/main/install.ps1 | iex"
        echo ""
        exit 1
        ;;
    *)
        echo "  Error: Unsupported OS: $OS"
        echo "  Use install.ps1 for Windows"
        exit 1
        ;;
esac

echo "  Target: $TARGET"

# Check for curl
if ! command -v curl &>/dev/null; then
    echo "  Error: curl is required but not found."
    echo "  Install it with: sudo apt-get install curl"
    exit 1
fi

# Get latest release version
echo "  Fetching latest release..."
RELEASE_JSON=$(curl -fsSL "https://api.github.com/repos/$REPO/releases/latest" \
    -H "User-Agent: codescope-installer" 2>&1) || {
    echo "  Error: Could not reach GitHub API."
    echo "  Check your internet connection or try:"
    echo "    curl -I https://api.github.com"
    exit 1
}

VERSION=$(echo "$RELEASE_JSON" | grep '"tag_name"' | head -1 | sed 's/.*"tag_name": *"\([^"]*\)".*/\1/' || true)

if [ -z "$VERSION" ]; then
    echo "  Error: Could not determine latest version."
    echo "  GitHub API response (first 200 chars):"
    echo "  ${RELEASE_JSON:0:200}"
    echo ""
    echo "  Check https://github.com/$REPO/releases"
    exit 1
fi

echo "  Latest version: $VERSION"

# Download
ARCHIVE="codescope-${VERSION}-${TARGET}.tar.gz"
DOWNLOAD_URL="https://github.com/$REPO/releases/download/${VERSION}/${ARCHIVE}"

echo "  Downloading $ARCHIVE..."
TEMP_DIR=$(mktemp -d)
trap 'rm -rf "$TEMP_DIR"' EXIT

if ! curl -fSL "$DOWNLOAD_URL" -o "$TEMP_DIR/$ARCHIVE" 2>&1; then
    echo ""
    echo "  Error: Download failed."
    echo "  URL: $DOWNLOAD_URL"
    echo ""
    echo "  This could mean:"
    echo "    - The release doesn't have binaries for your platform ($TARGET)"
    echo "    - GitHub is temporarily unavailable"
    echo ""
    echo "  Build from source instead:"
    echo "    git clone https://github.com/$REPO"
    echo "    cd codescope && cargo build --release"
    exit 1
fi

# Extract
echo "  Extracting..."
tar xzf "$TEMP_DIR/$ARCHIVE" -C "$TEMP_DIR"

# Find binaries (they might be in a subdirectory)
FOUND=0
for bin in codescope codescope-mcp codescope-web; do
    # Search in temp dir (flat or nested)
    BIN_PATH=$(find "$TEMP_DIR" -name "$bin" -type f 2>/dev/null | head -1 || true)
    if [ -n "$BIN_PATH" ]; then
        FOUND=$((FOUND + 1))
    fi
done

if [ "$FOUND" -eq 0 ]; then
    echo "  Error: No binaries found in archive."
    echo "  Archive contents:"
    ls -la "$TEMP_DIR"/ 2>/dev/null || true
    find "$TEMP_DIR" -type f 2>/dev/null || true
    exit 1
fi

# Stop running codescope processes before overwriting
if pgrep -x "codescope" >/dev/null 2>&1 || pgrep -x "codescope-mcp" >/dev/null 2>&1; then
    echo "  Stopping running codescope processes..."
    pkill -f "codescope" 2>/dev/null || true
    sleep 1
fi

# Install
mkdir -p "$INSTALL_DIR"
INSTALLED=0
for bin in codescope codescope-mcp codescope-web; do
    BIN_PATH=$(find "$TEMP_DIR" -name "$bin" -type f 2>/dev/null | head -1 || true)
    if [ -n "$BIN_PATH" ]; then
        rm -f "$INSTALL_DIR/$bin" 2>/dev/null || true
        cp "$BIN_PATH" "$INSTALL_DIR/$bin"
        chmod +x "$INSTALL_DIR/$bin"
        INSTALLED=$((INSTALLED + 1))
        echo "    + $bin"
    fi
done

echo "  Installed $INSTALLED binaries to $INSTALL_DIR"

# Check PATH
if [[ ":$PATH:" != *":$INSTALL_DIR:"* ]]; then
    echo ""
    echo "  WARNING: $INSTALL_DIR is not in your PATH."
    echo ""

    SHELL_NAME=$(basename "${SHELL:-bash}")
    case "$SHELL_NAME" in
        bash) RC_FILE="$HOME/.bashrc" ;;
        zsh)  RC_FILE="$HOME/.zshrc" ;;
        fish) RC_FILE="$HOME/.config/fish/config.fish" ;;
        *)    RC_FILE="$HOME/.profile" ;;
    esac

    echo "  Add this to $RC_FILE:"
    echo "    export PATH=\"\$HOME/.local/bin:\$PATH\""
    echo ""

    if [ -t 0 ]; then
        read -p "  Add to $RC_FILE now? [Y/n] " -n 1 -r
        echo
        if [[ ! $REPLY =~ ^[Nn]$ ]]; then
            echo "" >> "$RC_FILE"
            echo "# Codescope" >> "$RC_FILE"
            echo 'export PATH="$HOME/.local/bin:$PATH"' >> "$RC_FILE"
            echo "  Added to $RC_FILE — restart terminal or: source $RC_FILE"
        fi
    else
        echo "  (non-interactive mode — add PATH manually)"
    fi
fi

echo ""
echo "  Installation complete!"
echo ""
echo "  Quick start:"
echo "    cd your-project"
echo "    codescope init        # indexes + sets up MCP for Claude Code"
echo ""
echo "  Version: $VERSION"
echo ""
