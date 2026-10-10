#!/bin/sh
# Build the static web version into engine/web/dist/ (no game data inside: players drop their own ROM).
#   engine/package/web-dist.sh               wasm release builds + copy
#   SKIP_BUILD=1 engine/package/web-dist.sh  copy the wasm files already built by web/app/build.sh
# Host dist/ on any static server. No COOP/COEP needed (no SharedArrayBuffer); it needs a secure context
# (https:// or http://localhost) for OPFS / AudioWorklet, and .js served as JavaScript. Local: package/serve.py.
set -e
ENGINE="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ENGINE"
if [ -z "$SKIP_BUILD" ]; then
  sh web/app/build.sh
fi
DIST=web/dist
rm -rf "$DIST"
mkdir -p "$DIST/app"
cp web/extract.js web/wf_wasm.wasm "$DIST/"
cp web/app/index.html web/app/*.js web/app/wf_web.wasm "$DIST/app/"
# (build.sh is a dev script, not part of the site)
cat > "$DIST/index.html" <<'EOF'
<!doctype html><meta charset="utf-8"><title>Buu's Fury</title>
<meta http-equiv="refresh" content="0; url=app/"><a href="app/">Play</a>
EOF
du -sh "$DIST"
echo "built $ENGINE/$DIST  (serve: python3 package/serve.py)"
