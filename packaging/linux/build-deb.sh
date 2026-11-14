#!/usr/bin/env bash
# Builds dist/hallways_<version>_amd64.deb from a release binary.
# Usage: packaging/linux/build-deb.sh [version] [path/to/hallways]
set -euo pipefail
cd "$(dirname "$0")/../.."
VERSION="${1:-$(grep -m1 '^version' Cargo.toml | cut -d'"' -f2)}"
BIN="${2:-target/release/hallways}"
ROOT="$(mktemp -d)"
PKG="$ROOT/hallways_${VERSION}_amd64"

install -Dm755 "$BIN" "$PKG/opt/hallways/hallways"
cp -r assets "$PKG/opt/hallways/assets"
# Music drop-in folders are user-editable (no sudo needed).
chmod -R a+rwX "$PKG/opt/hallways/assets/music"
install -Dm644 packaging/linux/hallways.desktop "$PKG/usr/share/applications/hallways.desktop"
install -Dm644 packaging/linux/hallways.png "$PKG/usr/share/icons/hicolor/256x256/apps/hallways.png"
mkdir -p "$PKG/usr/bin"
cat > "$PKG/usr/bin/hallways" <<'SH'
#!/bin/sh
# The game finds its assets next to the real executable.
exec /opt/hallways/hallways "$@"
SH
chmod 755 "$PKG/usr/bin/hallways"

SIZE=$(du -sk "$PKG" | cut -f1)
mkdir -p "$PKG/DEBIAN"
cat > "$PKG/DEBIAN/control" <<CTL
Package: hallways
Version: ${VERSION}
Section: games
Priority: optional
Architecture: amd64
Installed-Size: ${SIZE}
Depends: libasound2 | libasound2t64, libudev1, libvulkan1, libx11-6, libxcursor1, libxrandr2, libxi6, libxkbcommon-x11-0
Recommends: mesa-vulkan-drivers
Maintainer: HALLWAYS developers <noreply@example.com>
Homepage: https://github.com/huy-fgugg/HallwaysFiles
Description: First-person procedural horror game
 Make it through 100 procedurally generated rooms, hide from rushing
 entities, read the door numbers, and play online with up to 3 friends
 using room codes.
CTL
mkdir -p dist
dpkg-deb --build --root-owner-group "$PKG" "dist/hallways_${VERSION}_amd64.deb"
rm -rf "$ROOT"
ls -l "dist/hallways_${VERSION}_amd64.deb"
