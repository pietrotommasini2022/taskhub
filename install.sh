#!/usr/bin/env sh
set -e

REPO="pietroairoldi/taskhub"
BIN="taskhub"
INSTALL_DIR="${INSTALL_DIR:-$HOME/.local/bin}"

# Detect OS and arch
OS=$(uname -s | tr '[:upper:]' '[:lower:]')
ARCH=$(uname -m)

case "$OS" in
  darwin)
    case "$ARCH" in
      arm64) FILE="taskhub-macos-aarch64.tar.gz" ;;
      *)     FILE="taskhub-macos-x86_64.tar.gz" ;;
    esac
    ;;
  linux)
    FILE="taskhub-linux-x86_64.tar.gz"
    ;;
  *)
    echo "Unsupported OS: $OS. Download manually from:"
    echo "  https://github.com/$REPO/releases/latest"
    exit 1
    ;;
esac

TAG=$(curl -fsSL "https://api.github.com/repos/$REPO/releases/latest" \
  | grep '"tag_name"' | sed 's/.*"tag_name": *"\([^"]*\)".*/\1/')

URL="https://github.com/$REPO/releases/download/$TAG/$FILE"

echo "Downloading TaskHub $TAG..."
TMP=$(mktemp -d)
curl -fsSL "$URL" -o "$TMP/taskhub.tar.gz"
tar -xzf "$TMP/taskhub.tar.gz" -C "$TMP"

mkdir -p "$INSTALL_DIR"
cp "$TMP/$BIN" "$INSTALL_DIR/$BIN"
chmod +x "$INSTALL_DIR/$BIN"
rm -rf "$TMP"

echo "Installed: $INSTALL_DIR/$BIN"

# Check PATH
case ":$PATH:" in
  *":$INSTALL_DIR:"*) ;;
  *)
    echo ""
    echo "Add to your shell profile:"
    echo "  export PATH=\"\$PATH:$INSTALL_DIR\""
    ;;
esac

echo "Run 'taskhub init' to get started."
