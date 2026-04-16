#!/usr/bin/env bash
set -euo pipefail

APP_BINARY=${1:-}
PLATFORM_SERVICE_BINARY=${2:-}
VERSION_TAG=${3:-}
OUTPUT_DIR=${4:-dist}
PACKAGE_NAME="nanite-clip"
PACKAGE_VERSION="${VERSION_TAG#v}"

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
[[ -f packaging/linux/maintainer-scripts/after-install.sh ]] || die "after-install hook not found"
[[ -f packaging/linux/maintainer-scripts/after-remove.sh ]] || die "after-remove hook not found"
command -v fpm >/dev/null || die "fpm is required to build rpm packages"

mkdir -p "$OUTPUT_DIR"
STAGING_DIR=$(mktemp -d "$OUTPUT_DIR/rpm-root.XXXXXX")
trap 'rm -rf "$STAGING_DIR"' EXIT

install -Dm755 "$APP_BINARY" "$STAGING_DIR/usr/bin/$PACKAGE_NAME"
install -Dm755 "$PLATFORM_SERVICE_BINARY" \
    "$STAGING_DIR/usr/lib/$PACKAGE_NAME/nanite-clip-platform-service"
install -Dm644 assets/NaniteClips.png \
    "$STAGING_DIR/usr/share/icons/hicolor/512x512/apps/$PACKAGE_NAME.png"
install -Dm644 assets/linux/nanite-clip.desktop \
    "$STAGING_DIR/usr/share/applications/$PACKAGE_NAME.desktop"
install -Dm644 assets/linux/dev.angz.NaniteClip.metainfo.xml \
    "$STAGING_DIR/usr/share/metainfo/dev.angz.NaniteClip.metainfo.xml"
install -Dm644 LICENSE "$STAGING_DIR/usr/share/licenses/$PACKAGE_NAME/LICENSE"

fpm \
    --input-type dir \
    --output-type rpm \
    --name "$PACKAGE_NAME" \
    --version "$PACKAGE_VERSION" \
    --iteration 1 \
    --architecture x86_64 \
    --maintainer "AnotherGenZ <git@angz.dev>" \
    --license "Apache-2.0" \
    --url "https://github.com/AnotherGenZ/nanite-clip" \
    --description "Desktop companion for PlanetSide 2 that automatically saves gameplay clips on notable events" \
    --rpm-tag "Recommends: gpu-screen-recorder" \
    --rpm-tag "Suggests: gpu-screen-recorder-notification" \
    --depends "sqlite" \
    --after-install packaging/linux/maintainer-scripts/after-install.sh \
    --after-remove packaging/linux/maintainer-scripts/after-remove.sh \
    --package "$OUTPUT_DIR" \
    --chdir "$STAGING_DIR" \
    .

find "$OUTPUT_DIR" -maxdepth 1 -name "*.rpm" -print | sort | tail -n 1
