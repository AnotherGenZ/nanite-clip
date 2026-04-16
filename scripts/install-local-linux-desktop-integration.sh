#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
APPLICATIONS_DIR="${XDG_DATA_HOME:-$HOME/.local/share}/applications"
ICONS_DIR="${XDG_DATA_HOME:-$HOME/.local/share}/icons/hicolor/512x512/apps"

install -Dm644 \
    "$ROOT_DIR/assets/linux/nanite-clip.desktop" \
    "$APPLICATIONS_DIR/nanite-clip.desktop"
install -Dm644 \
    "$ROOT_DIR/assets/NaniteClips.png" \
    "$ICONS_DIR/nanite-clip.png"

echo "installed desktop integration into ${XDG_DATA_HOME:-$HOME/.local/share}"
