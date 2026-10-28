#!/bin/sh
# The build gate: formatting, lints, tests, the app build, and the size limits
# of this repo. CI runs exactly this; so should you before every commit.
set -eu
cd "$(dirname "$0")/.."

cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build -p blitznir

fail=0
for f in $(find crates -type f -name '*.rs'); do
    n=$(wc -l < "$f")
    if [ "$n" -gt 400 ]; then
        echo "size gate: $f has $n lines (max 400)" >&2
        fail=1
    fi
done
n=$(wc -l < README.md)
if [ "$n" -gt 150 ]; then
    echo "size gate: README.md has $n lines (max 150)" >&2
    fail=1
fi
# the UI toolkit stays out of the shared half; the app has no webview (any OS)
deps() { cargo tree -q -p "$1" --target all -e normal,build --prefix none; }
if deps blitznir-shared | grep -qE '^freya'; then
    echo "dependency gate: blitznir-shared depends on freya" >&2
    fail=1
fi
if deps blitznir | grep -qE '^(wry|webkit)'; then
    echo "dependency gate: blitznir depends on a webview" >&2
    fail=1
fi
[ "$fail" -eq 0 ] && echo "check.sh: all green"
exit "$fail"
