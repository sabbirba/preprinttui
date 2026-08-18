#!/usr/bin/env bash
set -e

REPO="sabbirba/preprinttui"
INSTALL_DIR="${INSTALL_DIR:-/usr/local/bin}"

OS="$(uname -s | tr '[:upper:]' '[:lower:]')"
ARCH="$(uname -m)"

case "$OS" in
  darwin)
    if [ "$ARCH" = "arm64" ] || [ "$ARCH" = "aarch64" ]; then
      TARGET="macos-arm64"
    else
      echo "Unsupported macOS architecture: $ARCH"
      exit 1
    fi
    ;;
  linux)
    if [ "$ARCH" = "x86_64" ] || [ "$ARCH" = "amd64" ]; then
      TARGET="linux-musl-x86_64"
    else
      echo "Unsupported Linux architecture: $ARCH"
      exit 1
    fi
    ;;
  *)
    echo "Unsupported OS: $OS"
    exit 1
    ;;
esac

LATEST_RELEASE=$(curl -s "https://api.github.com/repos/$REPO/releases/latest" | grep '"tag_name":' | sed -E 's/.*"([^"]+)".*/\1/')
if [ -z "$LATEST_RELEASE" ]; then
  LATEST_RELEASE="v0.1.0"
fi

URL="https://github.com/$REPO/releases/download/$LATEST_RELEASE/preprinttui-$TARGET.tar.gz"

TMP_DIR=$(mktemp -d)
trap 'rm -rf "$TMP_DIR"' EXIT

echo "Downloading preprinttui ($LATEST_RELEASE) for $TARGET..."
curl -sL "$URL" -o "$TMP_DIR/preprinttui.tar.gz"

tar -xzf "$TMP_DIR/preprinttui.tar.gz" -C "$TMP_DIR"
chmod +x "$TMP_DIR/preprinttui"

if [ -w "$INSTALL_DIR" ]; then
  mv "$TMP_DIR/preprinttui" "$INSTALL_DIR/preprinttui"
else
  echo "Installing to $INSTALL_DIR (requires sudo)..."
  sudo mv "$TMP_DIR/preprinttui" "$INSTALL_DIR/preprinttui"
fi

echo "Successfully installed preprinttui to $INSTALL_DIR/preprinttui"
echo "Run 'preprinttui' to start."
