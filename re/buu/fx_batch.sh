#!/bin/sh
# trace every player skill's effect OBJs (fx_skill.py) and summarise (fx_summary.py) into work/buu/fx/summary_<id>.txt
cd "$(dirname "$0")"
PY=../../.venv/bin/python
for spec in "1 90 140" "2 100 220" "3 90 140" "4 4 90" "5 40 90" "6 90 140" "8 30 60" "9 130 180" "10 40 90" "11 90 140" "13 100 200" "14 20 60" "16 90 140" "17 60 140" "23 4 90"; do
  set -- $spec
  $PY fx_skill.py $1 $2 $3 2 0 -1 200 > /dev/null 2>&1
  $PY fx_summary.py ../../work/buu/fx/skill_$1.json > ../../work/buu/fx/summary_$1.txt
  echo "== $1"; cat ../../work/buu/fx/summary_$1.txt
done
