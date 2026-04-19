#!/usr/bin/env bash
set -euo pipefail

APP_BINARY=${1:-}
PLATFORM_SERVICE_BINARY=${2:-}
VERSION_TAG=${3:-}
OUTPUT_DIR=${4:-dist}
UPDATER_BINARY="$(dirname "$APP_BINARY")/nanite-clip-updater"

die() {
    echo "error: $*" >&2
    exit 1
}

[[ -n "$APP_BINARY" ]] || die "usage: $0 <app-binary> <platform-service-binary> <version-tag> [output-dir]"
[[ -n "$PLATFORM_SERVICE_BINARY" ]] || die "missing platform service binary path"
[[ -n "$VERSION_TAG" ]] || die "missing version tag"
[[ -f "$APP_BINARY" ]] || die "app binary not found: $APP_BINARY"
[[ -f "$PLATFORM_SERVICE_BINARY" ]] || die "platform service binary not found: $PLATFORM_SERVICE_BINARY"
[[ -f LICENSE ]] || die "LICENSE not found"
[[ -f assets/NaniteClips.png ]] || die "icon asset not found: assets/NaniteClips.png"
[[ -f assets/linux/nanite-clip.desktop ]] || die "desktop file not found: assets/linux/nanite-clip.desktop"
[[ -f assets/linux/dev.angz.NaniteClip.metainfo.xml ]] || die "AppStream file not found: assets/linux/dev.angz.NaniteClip.metainfo.xml"

ARCHIVE_NAME="nanite-clip-${VERSION_TAG}-x86_64-linux.tar.gz"
STAGING_DIR="${OUTPUT_DIR}/nanite-clip-${VERSION_TAG}-x86_64-linux"

rm -rf "$STAGING_DIR"
mkdir -p "$STAGING_DIR"

install -m755 "$APP_BINARY" "$STAGING_DIR/nanite-clip"
if [[ -f "$UPDATER_BINARY" ]]; then
    install -m755 "$UPDATER_BINARY" "$STAGING_DIR/nanite-clip-updater"
fi
install -m755 "$PLATFORM_SERVICE_BINARY" "$STAGING_DIR/nanite-clip-platform-service"
printf 'linux_portable\n' > "$STAGING_DIR/install-channel.txt"
install -m644 LICENSE "$STAGING_DIR/LICENSE"
install -Dm644 assets/NaniteClips.png \
    "$STAGING_DIR/usr/share/icons/hicolor/512x512/apps/nanite-clip.png"
install -Dm644 assets/linux/nanite-clip.desktop \
    "$STAGING_DIR/usr/share/applications/nanite-clip.desktop"
install -Dm644 assets/linux/dev.angz.NaniteClip.metainfo.xml \
    "$STAGING_DIR/usr/share/metainfo/dev.angz.NaniteClip.metainfo.xml"

mkdir -p "$OUTPUT_DIR"
tar -C "$OUTPUT_DIR" -czf "${OUTPUT_DIR}/${ARCHIVE_NAME}" "$(basename "$STAGING_DIR")"

echo "${OUTPUT_DIR}/${ARCHIVE_NAME}"
