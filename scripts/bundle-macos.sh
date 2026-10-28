#!/bin/sh
# The app as a signed bundle: target/release/bundle/Blitznir.app.
# Installs nothing. The macOS privacy grants hang on the designated
# requirement: the identifier, and with a certificate its leaf.
#
#   BLITZNIR_SIGN="My Code Signing"  a certificate of yours (README): every
#                                    rebuild keeps the grants
#   unset                            ad hoc: no certificate needed, but each
#                                    rebuild asks for every grant again
set -eu
cd "$(dirname "$0")/.."

sign="${BLITZNIR_SIGN:--}"
id=dev.blitznir.app
min=13.0
version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n 1)
app=target/release/bundle/Blitznir.app
plist=$app/Contents/Info.plist

BLITZNIR_COMMIT="$(git rev-parse --short HEAD 2>/dev/null || true)" \
    cargo build --release -p blitznir
rm -rf "$app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
cp target/release/blitznir "$app/Contents/MacOS/"
cp assets/Blitznir.icns "$app/Contents/Resources/"
# the hand-written keys (CFBundleName, LSUIElement, the microphone text), then the rest
cp crates/app/Info.plist "$plist"
while read -r key type value; do
    plutil -insert "$key" "-$type" "$value" "$plist"
done <<PLIST
CFBundleIdentifier string $id
CFBundleExecutable string blitznir
CFBundleDisplayName string Blitznir
CFBundleIconFile string Blitznir.icns
CFBundlePackageType string APPL
CFBundleInfoDictionaryVersion string 6.0
CFBundleShortVersionString string $version
CFBundleVersion string $version
LSMinimumSystemVersion string $min
NSHighResolutionCapable bool true
PLIST
plutil -lint -s "$plist"

codesign --force --deep --options runtime --entitlements crates/app/entitlements.plist \
    --sign "$sign" "$app"
codesign --verify --strict "$app"
codesign -dvv "$app" 2>&1 | grep -E '^(Identifier|Authority)='
echo "designated requirement: $(codesign -d -r- "$app" 2>&1 | sed -n 's/^designated => //p')"
