#!/usr/bin/env bash
# Build MultimPaste.app. The .app wrapper is what registers the "Multim Paste"
# right-click Service with macOS -- a bare binary cannot do that.
set -euo pipefail

root="$(cd "$(dirname "$0")/../.." && pwd)"
app="$root/target/MultimPaste.app"

cargo build --release --manifest-path "$root/Cargo.toml"

rm -rf "$app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
cp "$root/packaging/macos/Info.plist" "$app/Contents/Info.plist"
cp "$root/target/release/multimpaste" "$app/Contents/MacOS/multimpaste"

# Ad-hoc signature: without any signature macOS refuses to load the Service.
# Pinning the identifier keeps it stable across rebuilds. The signature itself still
# changes with every build, so macOS treats each build as a new app and the
# Accessibility permission has to be granted again after an update.
codesign --force --deep --sign - --identifier com.multimpaste.app "$app"

echo "Built $app"
echo "Install it with:  cp -R \"$app\" /Applications/"
