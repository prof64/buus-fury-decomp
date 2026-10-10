#!/bin/sh
# midcuts.sh SRC.json OUTDIR LEN F1 F2 ...: cut a recording at arbitrary video frames (mid-area cuts) of LEN frames
R=/Users/prof/source/buus-fury-decomp
SRC=$1
OUT=$2
LEN=$3
shift 3
mkdir -p "$OUT"
for f in "$@"; do
  SEGMENT_DIR="$OUT" "$R/.venv/bin/python" "$R/tools/segments/segment.py" cut --from "$SRC" --at "$f" --frames "$LEN" --name "mid_$f" 2>&1 | tail -1 | cut -c1-160
done
