#!/usr/bin/env bash
# Build tanu .deb + .rpm for Linux x86_64 and ARM (arm64 + armv7) in one run.
# Native target builds with cargo; ARM targets cross-compile with `cross`
# (needs Docker). Artifacts are collected into ./dist.
#
# A step progress bar tracks the 3 phases (build → deb → rpm) per target.
set -euo pipefail
cd "$(dirname "$0")/.."

HOST=x86_64-unknown-linux-gnu
TRIPLES=(
    x86_64-unknown-linux-gnu        # amd64
    aarch64-unknown-linux-gnu       # arm64
    armv7-unknown-linux-gnueabihf   # arm (32-bit hard-float)
)

TOTAL=$(( ${#TRIPLES[@]} * 3 ))   # build + deb + rpm per target
STEP=0

progress() { # $1 = label for the phase just starting
    STEP=$((STEP + 1))
    local width=30
    local filled=$(( STEP * width / TOTAL ))
    local pct=$(( STEP * 100 / TOTAL ))
    local bar
    bar=$(printf '%*s' "$filled" '' | tr ' ' '#')
    bar+=$(printf '%*s' "$(( width - filled ))" '')
    printf '\r[%s] %3d%% (%d/%d) %-28s' "$bar" "$pct" "$STEP" "$TOTAL" "$1"
    [ "$STEP" -eq "$TOTAL" ] && printf '\n'
}

mkdir -p dist
echo "Building tanu release packages → ./dist"

for t in "${TRIPLES[@]}"; do
    short=${t%%-*}

    progress "$short: compiling"
    if [ "$t" = "$HOST" ]; then
        cargo build --release --target "$t" >/dev/null 2>&1
        AUTO_REQ=auto   # host-arch: let rpm compute lib deps
    else
        cross build --release --target "$t" >/dev/null 2>&1
        AUTO_REQ=no     # cross-arch: find-requires can't read a foreign binary
    fi

    progress "$short: .deb"
    # Binary already stripped by the release profile → --no-strip.
    cargo deb --no-build --no-strip --target "$t" >/dev/null 2>&1

    progress "$short: .rpm"
    cargo generate-rpm --target "$t" --auto-req "$AUTO_REQ" >/dev/null 2>&1

    cp target/"$t"/generate-rpm/*.rpm dist/ 2>/dev/null || true
done

# cargo-deb writes to target/<triple>/debian and target/debian — grab both.
find target -name '*.deb' -path '*debian*' -exec cp {} dist/ \; 2>/dev/null || true

echo "==================== artifacts ===================="
ls -1 dist
