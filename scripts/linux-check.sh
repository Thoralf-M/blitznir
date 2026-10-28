#!/bin/sh
# Compile-check (or test/build) the Linux side inside Docker, from macOS.
#
#   scripts/linux-check.sh                       cargo check --workspace   in the repo root
#   scripts/linux-check.sh DIR                   cargo check               in DIR (relative to the repo)
#   scripts/linux-check.sh DIR test              cargo test                in DIR
#   scripts/linux-check.sh DIR build --release   any cargo subcommand + args
#
# The repo is bind-mounted at /work. The Linux target dir is target-linux/
# (gitignored), so macOS and Linux never share target/. The cargo registry
# lives in the named volume blitznir-cargo. CARGO_BUILD_JOBS is passed through.
# Debug info is line tables only: full debug info makes linking the app
# binary run out of memory in a small Docker VM (killed, signal 9). Nothing
# here runs a debugger; test backtraces keep file:line. The image links with
# lld for the same reason.
set -eu

root=$(cd "$(dirname "$0")/.." && pwd)
docker=${DOCKER:-docker}
command -v "$docker" >/dev/null 2>&1 || docker=/opt/homebrew/bin/docker
command -v "$docker" >/dev/null 2>&1 || {
    echo "no docker: install Docker (or set DOCKER to its path) and run this again" >&2
    exit 1
}
image=blitznir-linux

# Stdin build: no context is sent (the image needs nothing from the repo).
# Fully cached after the first run, so this costs ~1 s. Output only on failure
# (the legacy builder prints a deprecation notice on every run).
if ! log=$("$docker" build -q -t "$image" - < "$root/docker/Dockerfile.linux" 2>&1); then
    printf '%s\n' "$log" >&2
    exit 1
fi

dir=${1:-.}
[ $# -gt 0 ] && shift
if [ $# -eq 0 ]; then
    if [ "$dir" = "." ]; then set -- check --workspace; else set -- check; fi
fi
[ -d "$root/$dir" ] || { echo "no such directory: $root/$dir" >&2; exit 2; }

tty=
[ -t 0 ] && [ -t 1 ] && tty=-t

exec "$docker" run --rm -i $tty \
    -v "$root:/work" \
    -v blitznir-cargo:/usr/local/cargo/registry \
    -e CARGO_TARGET_DIR=/work/target-linux \
    -e CARGO_PROFILE_DEV_DEBUG=line-tables-only \
    -e CARGO_TERM_COLOR="${CARGO_TERM_COLOR:-auto}" \
    ${CARGO_BUILD_JOBS:+-e CARGO_BUILD_JOBS="$CARGO_BUILD_JOBS"} \
    -w "/work/$dir" \
    "$image" cargo "$@"
