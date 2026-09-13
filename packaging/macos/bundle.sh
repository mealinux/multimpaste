#!/usr/bin/env bash
# Build MultiPaste.app. The .app wrapper is what registers the "Multi Paste"
# right-click Service with macOS -- a bare binary cannot do that.
set -euo pipefail

root="$(cd "$(dirname "$0")/../.." && pwd)"
app="$root/target/MultiPaste.app"

cargo build --release --manifest-path "$root/Cargo.toml"

rm -rf "$app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
cp "$root/packaging/macos/Info.plist" "$app/Contents/Info.plist"
cp "$root/target/release/multipaste" "$app/Contents/MacOS/multipaste"

# Ad-hoc signature: without any signature macOS refuses to load the Service.
codesign --force --deep --sign - "$app"

echo "Built $app"
echo "Install it with:  cp -R \"$app\" /Applications/"
