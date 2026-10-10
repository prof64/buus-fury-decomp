#!/bin/sh
# tools/link/twoproc.sh [--no-build] - link play between TWO separate native app processes on this machine
# (engine/docs/netcode.md "Link play: two processes"): a local signalling server on 127.0.0.1 only (wf-signal), two
# headless + muted `buus-fury --link host|join:ROOM --link-bot dojo|trade` with their own data folders (their own save
# slots), real time. Checks (a) a Dojo match: both processes confirm the same match (frame, checksum, inputs' hash,
# mirrored endings) and (b) a trade: both report TRADE COMPLETE and the items + zenie are conserved across the two
# slot files on disk. Exit 0 = both pass. Env: TARGET (cargo target dir, default target-link2), WF_CONTENT (an
# extracted content/base; default the test-extracted one), PORT (default 3591), ONLY=dojo|trade, WAIT (s, 600).
set -u
R=$(cd "$(dirname "$0")/../.." && pwd)
E=$R/engine
TARGET=${TARGET:-target-link2}
BIN=$E/$TARGET/release
CONTENT=${WF_CONTENT:-$E/target/tmp/wf-core-field-content-2}
PORT=${PORT:-3591}
if [ "${1:-}" != "--no-build" ]; then
  # (each build's output kept: a failure prints its errors instead of failing silently)
  BL=$(mktemp /tmp/twoproc-build.XXXXXX)
  (cd "$E" && cargo build --release --target-dir "$TARGET" -p wf-native >"$BL" 2>&1 &&
    cargo build --release --target-dir "$TARGET" -p wf-app --example link_save >>"$BL" 2>&1 &&
    cargo build --release --target-dir "$TARGET" -p wf-net --features server --bin wf-signal >>"$BL" 2>&1) ||
    { echo "build failed:"; grep -E "^error" -A6 "$BL" | head -40; exit 2; }
fi
[ -f "$CONTENT/pack.json" ] || { echo "no extracted content at $CONTENT (set WF_CONTENT)"; exit 2; }
W=$(mktemp -d /tmp/wf-twoproc.XXXXXX)
"$BIN/wf-signal" "127.0.0.1:$PORT" > "$W/signal.log" 2>&1 &
SIG=$!
trap 'kill $SIG 2>/dev/null; kill $P1 $P2 2>/dev/null' EXIT INT TERM
sleep 1
fail=0

# (a) the Dojo: In-Game mode, 1 round; the host (Goku 90) walks in punching, the guest (Vegeta 60) stands
flow_dojo() {
  kind=dojo; room=TWOPROCD
  for who in host join; do mkdir -p "$W/$kind-$who/saves"; done
  "$BIN/examples/link_save" write "$CONTENT" "$W/dojo-host/saves/slot1.sav" 0:90 - 0 >/dev/null
  "$BIN/examples/link_save" write "$CONTENT" "$W/dojo-join/saves/slot1.sav" 4:60 - 0 >/dev/null
}
flow_trade() {
  kind=trade; room=TWOPROCT
  for who in host join; do mkdir -p "$W/$kind-$who/saves"; done
  "$BIN/examples/link_save" write "$CONTENT" "$W/trade-host/saves/slot1.sav" 0:20 0:5 500 >/dev/null
  "$BIN/examples/link_save" write "$CONTENT" "$W/trade-join/saves/slot1.sav" 4:20 1:4 100 >/dev/null
}
go() {
  kind=$1; room=$2
  for who in host join; do cp "$W/$kind-$who/saves/slot1.sav" "$W/$kind-$who.before.sav"; done
  "$BIN/buus-fury" --content "$CONTENT" --data "$W/$kind-host" --headless --mute --realtime --frames 40000 \
    --link "host:$room" --link-bot "$kind" --link-quick --signal "ws://127.0.0.1:$PORT" --net-name host \
    > "$W/$kind-host.log" 2>&1 & P1=$!
  "$BIN/buus-fury" --content "$CONTENT" --data "$W/$kind-join" --headless --mute --realtime --frames 40000 \
    --link "join:$room" --link-bot "$kind" --link-quick --signal "ws://127.0.0.1:$PORT" --net-name guest \
    > "$W/$kind-join.log" 2>&1 & P2=$!
  t=0
  while [ ! -s "$W/$kind-host/link_report.json" ] || [ ! -s "$W/$kind-join/link_report.json" ]; do
    sleep 1; t=$((t + 1))
    if [ $t -gt ${WAIT:-600} ]; then echo "$kind: no reports after ${WAIT:-600} s"; fail=1; break; fi
    if ! kill -0 $P1 2>/dev/null && ! kill -0 $P2 2>/dev/null; then echo "$kind: both apps ended without reports"; fail=1; break; fi
  done
  sleep 2
  kill $P1 $P2 2>/dev/null; wait $P1 $P2 2>/dev/null
  echo "== $kind (reports after $t s)"
  for who in host join; do echo "$who: $(tr -d '\n ' < "$W/$kind-$who/link_report.json" 2>/dev/null)"; done
}

if [ "${ONLY:-}" != trade ]; then
flow_dojo
go dojo TWOPROCD
python3 - "$W" <<'EOF' || fail=1
import json, sys
w = sys.argv[1]
h = json.load(open(f'{w}/dojo-host/link_report.json'))['match']
g = json.load(open(f'{w}/dojo-join/link_report.json'))['match']
ok = h and g and (h['frame'], h['checksum'], h['inputs_hash']) == (g['frame'], g['checksum'], g['inputs_hash'])
pair = {(h['ending'].split(' ')[0].split('{')[0], g['ending'].split(' ')[0].split('{')[0])} if h and g else set()
mirror = pair <= {('Won', 'Lost'), ('Lost', 'Won'), ('Draw', 'Draw')}
print('dojo: same match (frame, checksum, inputs) on both:', bool(ok), '| endings', h and h['ending'], '/', g and g['ending'], '| mirrored:', mirror)
sys.exit(0 if ok and mirror and h['frame'] > 60 else 1)
EOF
fi

if [ "${ONLY:-}" != dojo ]; then
flow_trade
go trade TWOPROCT
python3 - "$W" "$BIN/examples/link_save" <<'EOF' || fail=1
import json, subprocess, sys
w, tool = sys.argv[1], sys.argv[2]
rd = lambda p: json.loads(subprocess.check_output([tool, 'read', p]))
b = [rd(f'{w}/trade-{x}.before.sav') for x in ('host', 'join')]
a = [rd(f'{w}/trade-{x}/saves/slot1.sav') for x in ('host', 'join')]
last = [json.load(open(f'{w}/trade-{x}/link_report.json'))['last'] for x in ('host', 'join')]
tot = lambda s: {k: sum(int(x['supplies'].get(k, 0)) for x in s) for k in set().union(*[x['supplies'] for x in s])}
cons = tot(a) == tot(b) and sum(x['zenie'] for x in a) == sum(x['zenie'] for x in b) and sum(x['equipment'] for x in a) == sum(x['equipment'] for x in b)
changed = a != b
print('trade: reports', last, '| before', b, '| after', a, '| conserved:', cons, '| applied:', changed)
sys.exit(0 if cons and changed and last == ['complete', 'complete'] else 1)
EOF
fi
echo "logs: $W"
[ $fail = 0 ] && echo "TWO PROCESSES: PASS" || echo "TWO PROCESSES: FAIL"
exit $fail
