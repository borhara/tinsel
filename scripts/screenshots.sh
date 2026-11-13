#!/usr/bin/env bash
# Draws the README's pictures with tinsel itself: each example writes one frame as a web
# page, and headless Chrome, when it's installed, turns the page into a PNG in docs/.
#
#   scripts/screenshots.sh
#
# Set CHROME to the browser to use if it isn't found. Re-run whenever the look changes.

set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
out="$root/docs"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

chrome="${CHROME:-}"
if [[ -z "$chrome" ]]; then
    for c in "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome" \
        google-chrome google-chrome-stable chromium chromium-browser; do
        if command -v "$c" >/dev/null 2>&1 || [[ -x "$c" ]]; then
            chrome="$c"
            break
        fi
    done
fi

mkdir -p "$out"
cd "$root"
cargo build --quiet --examples

# name and size in columns x rows
shots=("dashboard 120x36" "services 120x30")

for shot in "${shots[@]}"; do
    read -r name size <<<"$shot"
    cols="${size%x*}"
    rows="${size#*x}"
    page="$work/$name.html"
    cargo run --quiet --example "$name" -- --html "$size" >"$page"

    if [[ -z "$chrome" ]]; then
        cp "$page" "$out/$name.html"
        echo "no Chrome found: wrote docs/$name.html only"
        continue
    fi

    # the page's 14px monospace is about 8.4px a column and 17.5px a row, inside 28px of
    # padding at the left and right and 24px at the top and bottom
    width=$((cols * 84 / 10 + 60))
    height=$((rows * 35 / 2 + 52))
    "$chrome" --headless --disable-gpu --hide-scrollbars --force-device-scale-factor=2 \
        --window-size="$width,$height" --screenshot="$out/$name.png" "file://$page" \
        >/dev/null 2>&1
    echo "wrote docs/$name.png (${width}x${height}, at twice the pixels)"
done
