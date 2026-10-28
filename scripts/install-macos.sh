#!/bin/sh
# Blitznir on macOS: build the signed bundle and install it as
# /Applications/Blitznir.app. Idempotent: run it again after a pull.
#
#   scripts/install-macos.sh           refuses to replace an existing bundle
#   scripts/install-macos.sh --force   replaces it; the old one goes to the Trash
#
# An existing bundle needs --force: it may be the copy in daily use.
set -eu
cd "$(dirname "$0")/.."

say() { printf '==> %s\n' "$*"; }
die() { printf 'install-macos: %s\n' "$*" >&2; exit 1; }

force=
case "${1:-}" in
"") ;;
--force) force=1 ;;
*) die "usage: scripts/install-macos.sh [--force]" ;;
esac

app=/Applications/Blitznir.app
built=target/release/bundle/Blitznir.app
sign="${BLITZNIR_SIGN:--}" # as bundle-macos.sh: a certificate's name, or ad hoc

# 1. refuse before a minute of building, not after
xcode-select -p >/dev/null 2>&1 ||
    die "no Xcode command line tools: xcode-select --install, then run this again"
command -v cargo >/dev/null 2>&1 ||
    die "no cargo: install Rust from https://rustup.rs, then run this again"
if [ -e "$app" ] && [ -z "$force" ]; then
    die "$app exists; --force replaces it"
fi
if pgrep -f "$app/Contents/MacOS/" >/dev/null; then
    die "a Blitznir runs from $app: quit it first (tray menu, Quit Blitznir)"
fi

# 2. a stable identity keeps the privacy grants across builds (README)
if [ "$sign" = "-" ]; then
    say "signing ad hoc: macOS asks for every grant again after each rebuild"
    say "(set BLITZNIR_SIGN to a code-signing certificate to keep them; README)"
elif ! security find-identity -v -p codesigning | grep -qF "\"$sign\""; then
    die "no valid code-signing identity \"$sign\" (README, Install macOS)"
else
    say "signing identity: $sign"
fi

# 3. build and sign
say "building $built"
scripts/bundle-macos.sh >/dev/null
codesign --verify --strict "$built" || die "$built: the signature does not verify"

# 4. install
if [ -e "$app" ]; then
    old="$HOME/.Trash/Blitznir-$(date +%Y%m%d-%H%M%S).app"
    mv "$app" "$old"
    say "moved the old bundle to $old"
fi
# moved, not copied: a second Blitznir.app left in target/ shows up in
# Spotlight and Launchpad as another app
mv "$built" "$app"
say "installed $app"
codesign -dvv "$app" 2>&1 | grep -E '^(Identifier|Authority)=' | sed 's/^/    /'
say "designated requirement: $(codesign -d -r- "$app" 2>&1 | sed -n 's/^designated => //p')"
say "next: open $app, grant what its tray menu asks for, tick Start at login there"
