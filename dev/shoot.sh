#!/bin/sh
# Screenshot the dashboard frontend with mocked backend state — no build, no
# Windows, no Tauri. Renders dev/preview.html (which shims the Tauri API and
# fires fake config/media/weather/stats events into the REAL main.js) in
# headless Chromium at panel proportions.
#
# Usage: dev/shoot.sh out.png ["?scenario=empty-media"]
# SIZE=1100,3840 for full panel resolution (default renders at 50%).
set -e
cd "$(dirname "$0")/.."
OUT="$1"
QUERY="$2"
SIZE="${SIZE:-550,1920}"

CHROME=$(ls -d "$HOME"/.cache/ms-playwright/chromium-*/chrome-linux*/chrome 2>/dev/null | tail -1)
[ -n "$CHROME" ] || { echo "no playwright chromium found under ~/.cache/ms-playwright" >&2; exit 1; }

python3 -m http.server 8931 >/dev/null 2>&1 &
SERVER=$!
trap 'kill $SERVER 2>/dev/null' EXIT
sleep 0.4

"$CHROME" \
  --headless --disable-gpu --hide-scrollbars \
  --window-size="$SIZE" \
  --screenshot="$OUT" \
  --virtual-time-budget=4000 \
  "http://127.0.0.1:8931/dev/preview.html$QUERY" 2>/dev/null
echo "wrote $OUT"
