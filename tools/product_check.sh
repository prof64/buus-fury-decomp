#!/bin/sh
# The product regression in one command (headless, muted; never a window): the parts of the app that sit on top of the
# exact core, next to tools/wholerun/check.sh (which asks "is the core still exact?"). Steps, in order:
#   browser   engine/web/tests/browser_check.mjs - the wasm build in a headless Chromium (ROM drop, story, saves,
#             reload, launcher, battles; its recording verified natively)
#   savecheck in-game save -> Continue on the app vs the game, at the whole-run cuts (tools/wholerun/savecheck.py
#             --compare of the game side - cached in work/wholerun/savecheck.json, made once from run 1's cuts - and
#             examples/savecheck.rs)
#   cartcheck real cartridge saves (work/run2/*.sav) -> Continue on the app (imported as the EEPROM) vs the game
#             (tools/wholerun/cartcheck.py, cached in work/wholerun/cartcheck.json; compared as the save check)
#   features  examples/features.rs - replays, save states into fresh sims, rewind, along run 1 from power-on
#   netstory  examples/netstory.rs - story mode through the rollback netcode (two peers, latency / jitter / loss)
#   wideview  tests/wide_view.rs - the zoomed-out PC view against the GBA view at the pixel gate's frames
# Exit 0 = every step passed; 1 = one failed (each step's log: work/product_check/<step>.log).
#
#   tools/product_check.sh [--only browser,savecheck,...] [--build] [--steps N] [--fresh-savecheck]
#     --build            rebuild the wasm (engine/web/app/build.sh) before the browser check
#     --steps N          features: steps of run 1 (default 400000; 0 = the whole run)
#     --fresh-savecheck  re-make the game side of the save checks (emulator, a few minutes)
# CARGO_TARGET_DIR is honoured (default engine/target). Needs the ROM (roms/buu.gba or $WF_ROM), run 1's whole-run
# master (tools/wholerun/record.py) and power_on.json, and engine/web/tests' npm install.
cd "$(dirname "$0")/.." || exit 2
ROOT=$(pwd)
PY=$ROOT/.venv/bin/python
[ -x "$PY" ] || PY=python3
TD=${CARGO_TARGET_DIR:-$ROOT/engine/target}
export CARGO_TARGET_DIR="$TD"
LOG=$ROOT/work/product_check
mkdir -p "$LOG"

ONLY=browser,savecheck,cartcheck,features,netstory,wideview
BUILD=
STEPS=400000
FRESH=
while [ $# -gt 0 ]; do
  case "$1" in
    --only) ONLY=$2; shift ;;
    --build) BUILD=--build ;;
    --steps) STEPS=$2; shift ;;
    --fresh-savecheck) FRESH=1 ;;
    *) echo "unknown option $1"; exit 2 ;;
  esac
  shift
done
want() { case ",$ONLY," in *",$1,"*) return 0 ;; esac; return 1; }

FAILED=
RESULTS=
note() { RESULTS="$RESULTS
  $1"; }
# run STEP CMD..: the command's output to its log; pass = exit 0 (and the check below, if any)
run() {
  step=$1; shift
  echo "== $step" >&2
  t0=$(date +%s)
  "$@" > "$LOG/$step.log" 2>&1
  rc=$?
  echo "   ($(( $(date +%s) - t0 ))s, exit $rc)" >&2
  return $rc
}
pass() { note "ok    $1: $2"; }
fail() { note "FAIL  $1: $2 (log: work/product_check/$1.log)"; FAILED="$FAILED $1"; }

if want browser; then
  if run browser sh -c "cd '$ROOT/engine/web/tests' && node browser_check.mjs $BUILD"; then
    pass browser "$(grep -iE 'passed|checks' "$LOG/browser.log" | tail -1)"
  else
    fail browser "$(tail -1 "$LOG/browser.log")"
  fi
fi

if want savecheck; then
  GAME=$ROOT/work/wholerun/savecheck.json
  ok=1
  if [ -n "$FRESH" ] || [ ! -f "$GAME" ]; then
    # the game side from run 1's whole-run cuts (the 41 the save check has used; their ready points are re-made)
    CUTS=$(ls "$ROOT/work/wholerun/savecheck_cuts" 2>/dev/null | sed -n 's/\.ss$//p' | sort -u | while read -r c; do
      [ -f "$ROOT/work/wholerun/cuts/$c.ss" ] && echo "$ROOT/work/wholerun/cuts/$c.ss"; done)
    if [ -z "$CUTS" ]; then
      fail savecheck "no cuts (work/wholerun/savecheck_cuts / work/wholerun/cuts)"; ok=
    else
      # shellcheck disable=SC2086
      run savecheck_game "$PY" tools/wholerun/savecheck.py $CUTS --out "$GAME" || { fail savecheck "the game side failed"; ok=; }
    fi
  fi
  if [ -n "$ok" ]; then
    if run savecheck sh -c "cd '$ROOT/engine' && cargo run -q --release -p wf-app --example savecheck -- '$GAME' '$LOG/savecheck_app.json' && cd '$ROOT' && '$PY' tools/wholerun/savecheck.py --compare '$GAME' '$LOG/savecheck_app.json'"; then
      last=$(tail -1 "$LOG/savecheck.log")
      case "$last" in *" 0 differ"*) pass savecheck "$last" ;; *) fail savecheck "$last" ;; esac
    else
      fail savecheck "$(tail -1 "$LOG/savecheck.log")"
    fi
  fi
fi

if want cartcheck; then
  GAME=$ROOT/work/wholerun/cartcheck.json
  ok=1
  if [ -n "$FRESH" ] || [ ! -f "$GAME" ]; then
    # shellcheck disable=SC2046
    run cartcheck_game "$PY" tools/wholerun/cartcheck.py $(ls "$ROOT"/work/run2/*.sav) --out "$GAME" || { fail cartcheck "the game side failed"; ok=; }
  fi
  if [ -n "$ok" ]; then
    if run cartcheck sh -c "cd '$ROOT/engine' && cargo run -q --release -p wf-app --example savecheck -- '$GAME' '$LOG/cartcheck_app.json' && cd '$ROOT' && '$PY' tools/wholerun/savecheck.py --compare '$GAME' '$LOG/cartcheck_app.json'"; then
      last=$(tail -1 "$LOG/cartcheck.log")
      case "$last" in *" 0 differ"*) pass cartcheck "$last" ;; *) fail cartcheck "$last" ;; esac
    else
      fail cartcheck "$(tail -1 "$LOG/cartcheck.log")"
    fi
  fi
fi

if want features; then
  SARG=
  [ "$STEPS" != 0 ] && SARG="--steps $STEPS"
  # shellcheck disable=SC2086
  if run features sh -c "cd '$ROOT/engine' && cargo run -q --release -p wf-app --example features -- '$ROOT/work/wholerun/master.jsonl' --power-on '$ROOT/work/wholerun/power_on.json' $SARG"; then
    r=$(grep -E '^rewind:' "$LOG/features.log"); s=$(grep -E '^save states:' "$LOG/features.log"); p=$(grep -E '^replay:' "$LOG/features.log")
    if echo "$r" | grep -q ' 0 differ' && echo "$s" | grep -q ' 0 differ' && echo "$p" | grep -q 'ok true'; then
      pass features "rewind 0 differ, save states 0 differ, replay ok"
    else
      fail features "$r | $s | $p"
    fi
  else
    fail features "$(tail -1 "$LOG/features.log")"
  fi
fi

if want netstory; then
  if run netstory sh -c "cd '$ROOT/engine' && cargo run -q --release -p wf-app --example netstory" \
      && grep -q 'netcode story mode: OK' "$LOG/netstory.log"; then
    pass netstory "$(grep -E 'straight replay' "$LOG/netstory.log")"
  else
    fail netstory "$(tail -1 "$LOG/netstory.log")"
  fi
fi

if want wideview; then
  if run wideview sh -c "cd '$ROOT/engine' && cargo test -q --release -p wf-app --test wide_view -- --nocapture"; then
    pass wideview "$(grep -E 'test result' "$LOG/wideview.log" | tail -1)"
  else
    fail wideview "$(grep -E 'test result|panicked' "$LOG/wideview.log" | tail -1)"
  fi
fi

echo "product check:$RESULTS"
if [ -n "$FAILED" ]; then
  echo "FAILED:$FAILED"
  exit 1
fi
echo "ALL OK"
