#!/usr/bin/env bash
# Build a universal Marquee.app and a dmg. Run on macOS from the repository root.
# Needs: rustup targets aarch64-apple-darwin and x86_64-apple-darwin, and
# create-dmg (brew install create-dmg).
set -euo pipefail

VERSION=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
OUT=target/macos
APP="$OUT/Marquee.app"

cargo build --release --target aarch64-apple-darwin
cargo build --release --target x86_64-apple-darwin

rm -rf "$OUT"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
lipo -create -output "$APP/Contents/MacOS/Marquee" \
  target/aarch64-apple-darwin/release/Marquee \
  target/x86_64-apple-darwin/release/Marquee
sed "s/__VERSION__/$VERSION/g" packaging/macos/Info.plist > "$APP/Contents/Info.plist"

# Ad-hoc signature for local tests. Release builds need a Developer ID.
codesign --force --deep --sign - "$APP"

create-dmg \
  --volname "Marquee" \
  --window-size 480 300 \
  --icon "Marquee.app" 120 140 \
  --app-drop-link 360 140 \
  "$OUT/Marquee-$VERSION.dmg" \
  "$APP"

echo "Built $APP and $OUT/Marquee-$VERSION.dmg"
