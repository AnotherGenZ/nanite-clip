#!/usr/bin/env bash
set -euo pipefail

REPO="AnotherGenZ/nanite-clip"
CARGO_TOML="Cargo.toml"

# --- helpers ---

die() { echo "error: $*" >&2; exit 1; }

confirm() {
    read -rp "$1 [y/N] " ans
    [[ "$ans" =~ ^[Yy]$ ]] || exit 0
}

current_version() {
    grep '^version' "$CARGO_TOML" | head -1 | sed 's/.*"\(.*\)"/\1/'
}

# --- preflight ---

command -v gh >/dev/null || die "gh CLI is required"
command -v git >/dev/null || die "git is required"
[[ -f "$CARGO_TOML" ]] || die "run this script from the repo root"
[[ -z "$(git status --porcelain)" ]] || die "working tree is dirty — commit or stash first"

# --- version ---

OLD_VERSION=$(current_version)
echo "Current version: $OLD_VERSION"

if [[ $# -ge 1 ]]; then
    NEW_VERSION="$1"
else
    read -rp "New version (without v prefix): " NEW_VERSION
fi

[[ -n "$NEW_VERSION" ]] || die "version cannot be empty"
[[ "$NEW_VERSION" != "$OLD_VERSION" ]] || die "version unchanged"

TAG="v$NEW_VERSION"

git tag -l "$TAG" | grep -q . && die "tag $TAG already exists"

echo ""
echo "Will release: $OLD_VERSION -> $NEW_VERSION (tag: $TAG)"
confirm "Proceed?"

# --- bump version ---

echo ""
echo "==> Bumping version in $CARGO_TOML"
sed -i "0,/^version = \"$OLD_VERSION\"/s//version = \"$NEW_VERSION\"/" "$CARGO_TOML"

echo "==> Updating Cargo.lock"
cargo update --workspace --quiet

# --- commit and tag ---

echo ""
echo "==> Committing version bump"
git add "$CARGO_TOML" Cargo.lock
git commit -m "release: v$NEW_VERSION"

echo "==> Tagging $TAG"
git tag "$TAG"

# --- push ---

echo ""
confirm "Push commit and tag to origin? (this triggers the release build)"
git push origin main
git push origin "$TAG"

echo ""
echo "==> Release build triggered. Watch it at:"
echo "    https://github.com/$REPO/actions"
echo ""
echo "Once the draft release appears, review and publish it at:"
echo "    https://github.com/$REPO/releases"
echo ""
echo "The AUR package will be updated automatically after the build completes."
