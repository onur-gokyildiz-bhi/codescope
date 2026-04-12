#!/bin/bash
# Codescope Installer for Linux/macOS
# Usage: curl -fsSL https://raw.githubusercontent.com/onur-gokyildiz-bhi/codescope/main/install.sh | bash

set -euo pipefail

REPO="onur-gokyildiz-bhi/codescope"

# Detect install directory: if codescope is already on PATH, update in-place.
# Otherwise fall back to ~/.local/bin (standard XDG convention).
EXISTING=$(command -v codescope 2>/dev/null || true)
if [ -n "$EXISTING" ] && [ -f "$EXISTING" ]; then
    INSTALL_DIR="$(dirname "$EXISTING")"
    echo "  Existing install detected: $INSTALL_DIR"
else
    INSTALL_DIR="${HOME}/.local/bin"
    echo "  Fresh install to: $INSTALL_DIR"
fi

echo ""
echo "  Codescope Installer"
echo "  ==================="
echo ""

# Detect OS and architecture
OS="$(uname -s)"
ARCH="$(uname -m)"

case "$OS" in
    Linux)
        case "$ARCH" in
            x86_64) TARGET="x86_64-unknown-linux-gnu" ;;
            aarch64|arm64) TARGET="aarch64-unknown-linux-gnu" ;;
            *) echo "  Error: Unsupported architecture: $ARCH"; exit 1 ;;
        esac
        ;;
    Darwin)
        case "$ARCH" in
            x86_64) TARGET="x86_64-apple-darwin" ;;
            arm64)  TARGET="aarch64-apple-darwin" ;;
            *) echo "  Error: Unsupported architecture: $ARCH"; exit 1 ;;
        esac
        ;;
    *)
        echo "  Error: Unsupported OS: $OS"
        echo "  Use install.ps1 for Windows"
        exit 1
        ;;
esac

echo "  Platform: $OS $ARCH ($TARGET)"

# Get latest release version
echo "  Fetching latest release..."
RELEASE_JSON=$(curl -fsSL "https://api.github.com/repos/$REPO/releases/latest" \
    -H "User-Agent: codescope-installer")
VERSION=$(echo "$RELEASE_JSON" | grep '"tag_name"' | head -1 | sed 's/.*"tag_name": *"\([^"]*\)".*/\1/')

if [ -z "$VERSION" ]; then
    echo "  Error: Could not determine latest version."
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

curl -fsSL "$DOWNLOAD_URL" -o "$TEMP_DIR/$ARCHIVE"

# Extract
echo "  Extracting..."
tar xzf "$TEMP_DIR/$ARCHIVE" -C "$TEMP_DIR"

# Install
mkdir -p "$INSTALL_DIR"
for bin in codescope codescope-mcp codescope-web; do
    if [ -f "$TEMP_DIR/$bin" ]; then
        cp "$TEMP_DIR/$bin" "$INSTALL_DIR/$bin"
        chmod +x "$INSTALL_DIR/$bin"
    fi
done

# Check PATH
if [[ ":$PATH:" != *":$INSTALL_DIR:"* ]]; then
    echo ""
    echo "  WARNING: $INSTALL_DIR is not in your PATH."
    echo ""

    # Detect shell and suggest
    SHELL_NAME=$(basename "$SHELL")
    case "$SHELL_NAME" in
        bash)
            RC_FILE="$HOME/.bashrc"
            ;;
        zsh)
            RC_FILE="$HOME/.zshrc"
            ;;
        fish)
            RC_FILE="$HOME/.config/fish/config.fish"
            ;;
        *)
            RC_FILE="$HOME/.profile"
            ;;
    esac

    echo "  Add this to $RC_FILE:"
    echo "    export PATH=\"\$HOME/.local/bin:\$PATH\""
    echo ""

    # Offer to add automatically
    if [ -t 0 ]; then
        read -p "  Add to $RC_FILE now? [Y/n] " -n 1 -r
        echo
        if [[ ! $REPLY =~ ^[Nn]$ ]]; then
            echo "" >> "$RC_FILE"
            echo "# Codescope" >> "$RC_FILE"
            echo 'export PATH="$HOME/.local/bin:$PATH"' >> "$RC_FILE"
            echo "  Added to $RC_FILE"
        fi
    fi
fi

echo ""
echo "  Installation complete!"
echo ""
echo "  Installed:"
echo "    codescope     -> $INSTALL_DIR/codescope"
echo "    codescope-mcp -> $INSTALL_DIR/codescope-mcp"
echo "    codescope-web -> $INSTALL_DIR/codescope-web"
echo ""
echo "  Quick start:"
echo "    cd your-project"
echo "    codescope init        # indexes + sets up MCP for Claude Code"
echo ""
echo "  That's it! Open the project in Claude Code and"
echo "  Codescope starts automatically with 52 MCP tools."
echo ""
