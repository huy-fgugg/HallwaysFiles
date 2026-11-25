#!/usr/bin/env bash
# Packages the Low-End Edition from a release build.
# usage: packaging/lowend/package.sh windows|linux VERSION [SRC_ROOT]
#   SRC_ROOT: checkout whose target/release and assets are used (default: .)
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
OS="$1"; VERSION="$2"; SRC="${3:-.}"
PY="$(command -v python3 || command -v python)"
"$PY" -m pip install --quiet pillow
OUT="$(pwd)/dist"; mkdir -p "$OUT"
STAGE="$(mktemp -d)"
DIR="$STAGE/HALLWAYS-LowEnd"
mkdir -p "$DIR"
cp -rL "$SRC/assets" "$DIR/assets"
"$PY" "$HERE/shrink.py" "$DIR/assets" 256
if [ "$OS" = windows ]; then
  cp "$SRC/target/release/hallways.exe" "$DIR/"
  cp "$HERE/HALLWAYS (Low-End).bat" "$DIR/"
  (cd "$STAGE" && 7z a -tzip -bso0 "$OUT/hallways-${VERSION}-windows-lowend.zip" HALLWAYS-LowEnd)
else
  install -m755 "$SRC/target/release/hallways" "$DIR/hallways"
  cat > "$DIR/hallways-lowend.sh" <<'SH'
#!/bin/sh
# HALLWAYS Low-End Edition: low graphics, smaller textures.
cd "$(dirname "$0")" && exec ./hallways --low --lowend "$@"
SH
  chmod 755 "$DIR/hallways-lowend.sh"
  tar -C "$STAGE" -czf "$OUT/hallways-${VERSION}-linux-lowend.tar.gz" HALLWAYS-LowEnd
fi
rm -rf "$STAGE"
ls -l "$OUT"
