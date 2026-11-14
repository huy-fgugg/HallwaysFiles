#!/usr/bin/env bash
# Builds an unsigned HALLWAYS .ipa for iPhone/iPad (run on macOS with Xcode).
# Install it with Sideloadly or AltStore, which sign it with your Apple ID.
# Usage: packaging/ios/build-ipa.sh <version> <out.ipa>
set -euo pipefail
VERSION="$1"
OUT="$2"
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
SDK="$(xcrun --sdk iphoneos --show-sdk-path)"
export IPHONEOS_DEPLOYMENT_TARGET=14.0

(cd "$ROOT/mobile" && cargo build --release --target aarch64-apple-ios)
LIB="$ROOT/mobile/target/aarch64-apple-ios/release/libhallways_mobile.a"

WORK="$(mktemp -d)"
APP="$WORK/Payload/HALLWAYS.app"
mkdir -p "$APP"

xcrun --sdk iphoneos clang -arch arm64 -isysroot "$SDK" -miphoneos-version-min=14.0 -O2 \
  "$ROOT/packaging/ios/main.m" "$LIB" \
  -framework UIKit -framework Foundation -framework CoreFoundation -framework CoreGraphics \
  -framework QuartzCore -framework Metal -framework AudioToolbox -framework CoreAudio \
  -framework AVFoundation -framework GameController -framework CoreHaptics -framework Security \
  -framework SystemConfiguration \
  -lc++ -liconv -lresolv -lobjc \
  -o "$APP/HALLWAYS"
strip -x "$APP/HALLWAYS" || true

sed "s/@VERSION@/$VERSION/g" "$ROOT/packaging/ios/Info.plist" > "$APP/Info.plist"
plutil -lint "$APP/Info.plist"
printf 'APPL????' > "$APP/PkgInfo"
cp -R "$ROOT/assets" "$APP/assets"
# Icons from the Linux icon (sips ships with macOS).
for s in 60 76; do
  sips -z $s $s "$ROOT/packaging/linux/hallways.png" --out "$APP/AppIcon${s}x${s}.png" >/dev/null
  sips -z $((s * 2)) $((s * 2)) "$ROOT/packaging/linux/hallways.png" --out "$APP/AppIcon${s}x${s}@2x.png" >/dev/null
done
sips -z 180 180 "$ROOT/packaging/linux/hallways.png" --out "$APP/AppIcon60x60@3x.png" >/dev/null

(cd "$WORK" && zip -qry "$OUT" Payload)
case "$OUT" in /*) ;; *) mv "$WORK/$OUT" "$ROOT/$OUT" ;; esac
echo "wrote $OUT"
