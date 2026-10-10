#!/bin/sh
# Build both wasm binaries the browser shell needs and copy them next to the pages:
#   engine/web/wf_wasm.wasm      the extractor (crates/wf-wasm), run in a worker
#   engine/web/app/wf_web.wasm   the game + audio (crates/wf-web)
# Then serve engine/web/ (e.g. `python3 -m http.server 5370 -d engine/web`) and open /app/.
# CARGO_TARGET_DIR is honoured (default engine/target).
set -e
cd "$(dirname "$0")/../.."
T="${CARGO_TARGET_DIR:-target}"
cargo build --target wasm32-unknown-unknown -p wf-wasm -p wf-web --profile release-wasm --target-dir "$T"
cp "$T/wasm32-unknown-unknown/release-wasm/wf_wasm.wasm" web/wf_wasm.wasm
cp "$T/wasm32-unknown-unknown/release-wasm/wf_web.wasm" web/app/wf_web.wasm
ls -la web/wf_wasm.wasm web/app/wf_web.wasm
