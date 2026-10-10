#!/bin/sh
# Internet-like scenarios with real processes on this machine (no system installs, nothing public):
#   relay   TURN relay only: wf-turn (REST credentials) + wf-signal handing them out at /ice, both clients
#           --net-relay-only (only relay candidates are exchanged: what two symmetric NATs force)
#   wss     signaling over TLS (wf-signal --tls with a throwaway CA + certificate, clients --net-ca), server key and
#           room token required; a client with a wrong token is refused
#   rejoin  the joiner crashes mid-match (--net-exit-at) and its restarted game rejoins with the saved ticket
#   pause   the room's shared pause: the joiner holds its menu for 120 frames, the world stands still for both
# Every packet gets LATENCY ms + LOSS loss on send, each way. Prints both NETRESULT lines per scenario and checks the
# state hash at FRAMES.
#   engine/crates/wf-net/scripts/internet_sim.sh [relay|wss|rejoin|pause|all] [FRAMES] [LATENCY_MS] [LOSS]
# Needs: cargo build --release -p wf-native -p wf-net --features wf-net/server,wf-net/turn \
#          --bin buus-fury --bin wf-signal --bin wf-turn
# and extracted content in $DATA (any headless run with the ROM path extracts it).
set -e
ENGINE="$(cd "$(dirname "$0")/../../.." && pwd)"
BIN="$ENGINE/target/release"
OUT="$ENGINE/target/wfnet/sim"
DATA="${DATA:-$ENGINE/target/wfnet/data}"
WHAT="${1:-all}"; FRAMES="${2:-2400}"; LAT="${3:-60}"; LOSS="${4:-0.03}"
mkdir -p "$OUT"
PIDS=""
cleanup() { for p in $PIDS; do kill "$p" 2>/dev/null || true; done; }
trap cleanup EXIT
COMMON="--headless --data $DATA --frames $FRAMES --net-latency $LAT --net-loss $LOSS --net-report $OUT/reports --net-record $OUT/replays"

hash_of() { sed -n 's/.*"hash":"\([0-9a-f]*\)".*/\1/p' "$1" | tail -1; }
field() { sed -n "s/.*\"$2\":\([^,}]*\).*/\1/p" "$1" | tail -1; }
check() {
  H=$(hash_of "$OUT/$1-host.out"); J=$(hash_of "$OUT/$1-join.out")
  grep -h NETRESULT "$OUT/$1-host.out" "$OUT/$1-join.out" | cut -c1-420
  if [ -n "$H" ] && [ "$H" = "$J" ] && [ "$H" != "0000000000000000" ]; then echo "[$1] SAME HASH at state $FRAMES: $H"; else echo "[$1] HASH MISMATCH: $H vs $J"; exit 1; fi
}
signal() { # PORT [args]
  P=$1; shift
  "$BIN/wf-signal" "127.0.0.1:$P" "$@" 2>"$OUT/signal-$P.log" &
  PIDS="$PIDS $!"
  sleep 0.5
}

relay() {
  SECRET="test-$(date +%s)"
  "$BIN/wf-turn" --listen 127.0.0.1:3479 --public-ip 127.0.0.1 --secret "$SECRET" 2>"$OUT/turn.log" &
  PIDS="$PIDS $!"
  signal 3551 --turn-url turn:127.0.0.1:3479 --turn-secret "$SECRET"
  ROOM="R$(date +%s | tail -c 5)"
  A="$COMMON --signal ws://127.0.0.1:3551 --net-ice auto --net-relay-only"
  "$BIN/buus-fury" $A --net-host --room "$ROOM" --net-random-input 11 --net-seed 1 >"$OUT/relay-host.out" 2>"$OUT/relay-host.log" &
  H=$!; sleep 0.3
  "$BIN/buus-fury" $A --net-join "$ROOM" --net-random-input 22 --net-seed 2 --fighter vegeta --form 26 >"$OUT/relay-join.out" 2>"$OUT/relay-join.log" &
  J=$!; wait $H; wait $J
  check relay
  grep -h "ICE servers\|relay" "$OUT/relay-host.log" | head -3
  echo "[relay] TURN server: $(grep -c authenticated "$OUT/turn.log") clients authenticated"; grep authenticated "$OUT/turn.log" | head -4
  [ "$(grep -c authenticated "$OUT/turn.log")" -ge 2 ] || { echo "[relay] the TURN server was not used"; exit 1; }
}

wss() {
  # a throwaway CA and a localhost certificate it signs
  C="$OUT/tls"; mkdir -p "$C"
  openssl req -x509 -newkey rsa:2048 -nodes -keyout "$C/ca.key" -out "$C/ca.pem" -days 2 -subj "/CN=wf test CA" >/dev/null 2>&1
  openssl req -newkey rsa:2048 -nodes -keyout "$C/key.pem" -out "$C/req.csr" -subj "/CN=localhost" >/dev/null 2>&1
  printf "subjectAltName=DNS:localhost,IP:127.0.0.1\nbasicConstraints=CA:FALSE\nextendedKeyUsage=serverAuth\n" >"$C/ext.cnf"
  openssl x509 -req -in "$C/req.csr" -CA "$C/ca.pem" -CAkey "$C/ca.key" -CAcreateserial -out "$C/cert.pem" -days 2 -extfile "$C/ext.cnf" >/dev/null 2>&1
  KEY="k$(date +%s)"
  signal 3552 --tls 127.0.0.1:3553 --tls-cert "$C/cert.pem" --tls-key "$C/key.pem" --key "$KEY" --require-token
  # /ice over https with the key (and without: refused)
  curl -s --cacert "$C/ca.pem" "https://localhost:3553/ice?key=$KEY" | head -c 200; echo
  echo "[wss] /ice without the key: HTTP $(curl -s -o /dev/null -w '%{http_code}' --cacert "$C/ca.pem" https://localhost:3553/ice)"
  ROOM="W$(date +%s | tail -c 5)"
  A="$COMMON --signal wss://localhost:3553 --net-ca $C/ca.pem --net-key $KEY --net-ice none"
  "$BIN/buus-fury" $A --net-host --room "$ROOM" --net-token TOK123 --net-random-input 11 --net-seed 1 >"$OUT/wss-host.out" 2>"$OUT/wss-host.log" &
  H=$!; sleep 0.5
  "$BIN/buus-fury" $A --net-join "$ROOM-WRONG1" --net-lobby-timeout 8 >"$OUT/wss-bad.out" 2>"$OUT/wss-bad.log" || true
  echo "[wss] wrong token: $(grep -h -o 'refused[^"]*' "$OUT/wss-bad.log" | head -1)"
  "$BIN/buus-fury" $A --net-join "$ROOM-TOK123" --net-random-input 22 --net-seed 2 --fighter vegeta --form 26 >"$OUT/wss-join.out" 2>"$OUT/wss-join.log" &
  J=$!; wait $H; wait $J
  check wss
  grep "refused" "$OUT/signal-3552.log" | head -2
}

rejoin() {
  signal 3554
  ROOM="J$(date +%s | tail -c 5)"
  A="$COMMON --signal ws://127.0.0.1:3554"
  T="$OUT/ticket-join.json"; rm -f "$T"
  "$BIN/buus-fury" $A --net-host --room "$ROOM" --net-random-input 11 --net-seed 1 --net-ticket "$OUT/ticket-host.json" >"$OUT/rejoin-host.out" 2>"$OUT/rejoin-host.log" &
  H=$!; sleep 0.3
  "$BIN/buus-fury" $A --net-join "$ROOM" --net-random-input 22 --net-seed 2 --fighter vegeta --form 26 --net-ticket "$T" --net-exit-at 900 >"$OUT/rejoin-crash.out" 2>"$OUT/rejoin-crash.log" || echo "[rejoin] the joiner vanished at frame 900 (exit $?)"
  sleep 2
  "$BIN/buus-fury" $A --net-rejoin --net-ticket "$T" --net-random-input 33 --net-seed 3 >"$OUT/rejoin-join.out" 2>"$OUT/rejoin-join.log" &
  J=$!; wait $H; wait $J
  check rejoin
  grep -h "rejoin" "$OUT/rejoin-host.log" "$OUT/rejoin-join.log" | head -6
  [ "$(field "$OUT/rejoin-join.out" was_rejoin)" = "true" ] || { echo "[rejoin] not a rejoin?"; exit 1; }
}

pause() {
  signal 3555
  ROOM="P$(date +%s | tail -c 5)"
  A="$COMMON --signal ws://127.0.0.1:3555"
  "$BIN/buus-fury" $A --net-host --room "$ROOM" --net-pause shared --net-random-input 11 --net-seed 1 >"$OUT/pause-host.out" 2>"$OUT/pause-host.log" &
  H=$!; sleep 0.3
  "$BIN/buus-fury" $A --net-join "$ROOM" --net-random-input 22 --net-seed 2 --net-pause-at 600:120 >"$OUT/pause-join.out" 2>"$OUT/pause-join.log" &
  J=$!; wait $H; wait $J
  check pause
  echo "[pause] host: session frame $(field "$OUT/pause-host.out" frame_now), world frame $(field "$OUT/pause-host.out" world_frame), stalls $(field "$OUT/pause-host.out" stalls); joiner menu ticks $(field "$OUT/pause-join.out" menu_ticks)"
}

case "$WHAT" in
  relay) relay ;;
  wss) wss ;;
  rejoin) rejoin ;;
  pause) pause ;;
  all) relay; wss; rejoin; pause ;;
  *) echo "usage: $0 [relay|wss|rejoin|pause|all] [FRAMES] [LATENCY_MS] [LOSS]"; exit 2 ;;
esac
