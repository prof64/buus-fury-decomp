#!/bin/sh
# Coverage replays: every gated comparison (and the by-hand audits in work/) run with the recorder on
# (`WF_COVERAGE`, engine/crates/wf-view/src/coverage.rs: counters only). Headless and muted (the test harnesses).
#   tools/coverage/run.sh [TARGET_DIR] [LEVEL..]     (default target dir `target`, default every level)
# Levels: locked (the pixel lock), state (the segment gate's actor comparison), menu (menu scenarios), link (the link
# recordings), lockstep (the battle lockstep traces), audit (work/ub/oa opponent audit + work/skillaudit/segs),
# run (the three whole runs, tools/coverage/runs.py: compared per logic iteration; its counts are the frequencies),
# board (the picture board over every segment: compared, not all exact; its per-frame results go to
# work/coverage/board.json for the report's list of the new scenes). Raw lines: work/coverage/raw/*.jsonl
# (git-ignored); then `tools/coverage/report.py` writes engine/docs/coverage.md + data/coverage.json.
R=$(cd "$(dirname "$0")/../.." && pwd)
T=${1:-target}
[ $# -gt 0 ] && shift
LEVELS=${*:-locked state menu link lockstep run audit board}
RAW=${COVERAGE_RAW:-$R/work/coverage/raw}
LOG=$R/work/coverage/logs
mkdir -p "$RAW" "$LOG"
cd "$R/engine" || exit 1
export WF_COVERAGE="$RAW"
t() { # level, cargo test args
  l=$1; shift
  rm -f "$RAW/$l"-*.jsonl
  WF_COVERAGE_LEVEL=$l cargo test --release --target-dir "$T" "$@" >"$LOG/$l.log" 2>&1
  echo "$l: $(tail -3 "$LOG/$l.log" | grep 'test result' | head -1)"
}
for l in $LEVELS; do
  case $l in
    locked) t locked -p wf-app --test pixels ;;
    state) t state -p wf-import --test segments ;;
    menu) t menu -p wf-app --test menu_parity ;;
    link) t link -p wf-app --test dojo_pixels --test link ;;
    lockstep) t lockstep -p wf-core --test lockstep ;;
    run) "$R/.venv/bin/python" "$R/tools/coverage/runs.py" "$T" ;;
    audit)
      rm -f "$RAW"/audit-*.jsonl
      cargo build --release --target-dir "$T" -p wf-app --example opp_audit --example pixels >"$LOG/audit.log" 2>&1
      WF_COVERAGE_LEVEL=audit OA_OUT="$R/work/coverage/oa_out" OA_PNG=0 "$T/release/examples/opp_audit" "$R"/work/ub/oa/oa_t*.json >>"$LOG/audit.log" 2>&1
      ls "$R"/work/skillaudit/segs/*.json | WF_COVERAGE_LEVEL=audit PIX_NOPROPS=1 PIX_ALL=1 SEGMENT_VRAM=0 xargs "$T/release/examples/pixels" >>"$LOG/audit.log" 2>&1
      echo "audit: $(grep -c '^oa_\|^sa_' "$LOG/audit.log") scenarios"
      ;;
    board)
      rm -f "$RAW"/board-*.jsonl
      cargo build --release --target-dir "$T" -p wf-app --example pixels >"$LOG/board.log" 2>&1
      ls "$R"/data/segments/*.json | grep -v "/pixels" | WF_COVERAGE_LEVEL=board PIX_JSON="$R/work/coverage/board.json" xargs "$T/release/examples/pixels" >>"$LOG/board.log" 2>&1
      echo "board: $(tail -1 "$LOG/board.log")"
      ;;
  esac
done
