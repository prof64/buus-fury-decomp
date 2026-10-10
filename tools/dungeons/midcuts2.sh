#!/bin/sh
# midcuts2.sh SRC.json OUTDIR PREFIX LEN F1 F2 ...: cut a recording at arbitrary video frames (mid-area cuts) of LEN
# frames, named PREFIX_F (unique across chapters); each cut's start state is kept in OUTDIR/states (python tools)
R=/Users/prof/source/buus-fury-decomp
SRC=$1
OUT=$2
P=$3
LEN=$4
shift 4
mkdir -p "$OUT/states"
for f in "$@"; do
  SEGMENT_DIR="$OUT" "$R/.venv/bin/python" "$R/tools/segments/segment.py" cut --from "$SRC" --at "$f" --frames "$LEN" --name "${P}_$f" 2>&1 | tail -1 | cut -c1-160
  cp "$R/work/segments/${P}_$f.ss" "$OUT/states/" 2>/dev/null
done
