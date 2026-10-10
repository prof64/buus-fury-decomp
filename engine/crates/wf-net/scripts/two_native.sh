#!/bin/sh
# Two headless native clients through a local signaling server over real WebRTC (localhost only), with artificial
# latency / loss added on send by each client. Prints both NETRESULT lines and compares the state hash at FRAMES.
#   engine/crates/wf-net/scripts/two_native.sh [FRAMES] [LATENCY_MS] [LOSS] [extra args for both clients...]
# Needs: cargo build --release -p wf-native -p wf-net --features wf-net/server --bin buus-fury --bin wf-signal
# and extracted content in $DATA (any headless run with the ROM path extracts it).
set -e
ENGINE="$(cd "$(dirname "$0")/../../.." && pwd)"
BIN="$ENGINE/target/release"
OUT="$ENGINE/target/wfnet"
DATA="${DATA:-$OUT/data}"
FRAMES="${1:-3600}"; LAT="${2:-100}"; LOSS="${3:-0.05}"
shift 3 2>/dev/null || shift $#
PORT="${PORT:-3537}"
ROOM="T$(date +%s | tail -c 5)"
mkdir -p "$OUT"
"$BIN/wf-signal" "127.0.0.1:$PORT" 2>"$OUT/signal.log" &
SIG=$!
trap 'kill $SIG 2>/dev/null' EXIT
sleep 0.5
COMMON="--headless --data $DATA --signal ws://127.0.0.1:$PORT --frames $FRAMES --net-latency $LAT --net-loss $LOSS --net-report $OUT/reports"
"$BIN/buus-fury" $COMMON --net-host --room "$ROOM" --net-random-input 11 --net-seed 1 --net-result "$OUT/host.json" "$@" >"$OUT/host.out" 2>"$OUT/host.log" &
H=$!
sleep 0.3
"$BIN/buus-fury" $COMMON --net-join "$ROOM" --net-random-input 22 --net-seed 2 --fighter vegeta --form 26 --net-result "$OUT/join.json" "$@" >"$OUT/join.out" 2>"$OUT/join.log" &
J=$!
wait $H; wait $J
grep NETRESULT "$OUT/host.out" "$OUT/join.out"
HH=$(sed -n 's/.*"hash":"\([0-9a-f]*\)".*/\1/p' "$OUT/host.out")
JH=$(sed -n 's/.*"hash":"\([0-9a-f]*\)".*/\1/p' "$OUT/join.out")
if [ -n "$HH" ] && [ "$HH" = "$JH" ] && [ "$HH" != "0000000000000000" ]; then echo "SAME HASH at state $FRAMES: $HH"; else echo "HASH MISMATCH: $HH vs $JH"; exit 1; fi
