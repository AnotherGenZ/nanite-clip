#!/usr/bin/env bash
set -euo pipefail

APP_BINARY=${1:-}
VERSION_TAG=${2:-}
OUTPUT_DIR=${3:-dist}
APP_ID="dev.angz.NaniteClip"

MANIFEST="packaging/flatpak/${APP_ID}.yaml"
DESKTOP_FILE="packaging/flatpak/${APP_ID}.desktop"
METAINFO_FILE="packaging/flatpak/${APP_ID}.metainfo.xml"
ICON_FILE="assets/NaniteClips-512.png"

cleanup() {
    rm -rf "$BUILD_DIR" "$REPO_DIR" "$STAGING_DIR"
}

die() {
    echo "error: $*" >&2
    exit 1
}

[[ -n "$APP_BINARY" ]] || die "usage: $0 <app-binary> <version-tag> [output-dir]"
[[ -n "$VERSION_TAG" ]] || die "missing version tag"
[[ -f "$APP_BINARY" ]] || die "app binary not found: $APP_BINARY"
[[ -f "$MANIFEST" ]] || die "flatpak manifest not found: $MANIFEST"
[[ -f "$DESKTOP_FILE" ]] || die "flatpak desktop file not found: $DESKTOP_FILE"
[[ -f "$METAINFO_FILE" ]] || die "flatpak metainfo file not found: $METAINFO_FILE"
[[ -f "$ICON_FILE" ]] || die "icon asset not found: $ICON_FILE"
command -v flatpak >/dev/null || die "flatpak is required"
command -v flatpak-builder >/dev/null || die "flatpak-builder is required"

mkdir -p "$OUTPUT_DIR"
BUILD_DIR=$(mktemp -d "$OUTPUT_DIR/flatpak-build.XXXXXX")
REPO_DIR=$(mktemp -d "$OUTPUT_DIR/flatpak-repo.XXXXXX")
STAGING_DIR="$(pwd)/flatpak-staging"
trap cleanup EXIT

rm -rf "$STAGING_DIR"
mkdir -p "$STAGING_DIR"
install -m755 "$APP_BINARY" "$STAGING_DIR/nanite-clip"
install -m644 "$DESKTOP_FILE" "$STAGING_DIR/${APP_ID}.desktop"
install -m644 "$METAINFO_FILE" "$STAGING_DIR/${APP_ID}.metainfo.xml"
install -m644 "$ICON_FILE" "$STAGING_DIR/${APP_ID}.png"

flatpak-builder --force-clean --repo="$REPO_DIR" "$BUILD_DIR" "$MANIFEST"

BUNDLE_PATH="$OUTPUT_DIR/nanite-clip-${VERSION_TAG}-x86_64.flatpak"
flatpak build-bundle "$REPO_DIR" "$BUNDLE_PATH" "$APP_ID" stable

echo "$BUNDLE_PATH"
