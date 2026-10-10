#!/bin/sh
# bisect_seg.sh COMMIT SEGDIR SEGMENT: replay one segment on the engine as it was at COMMIT (exported read-only with
# git archive into /tmp/bis, built in the private target dir /tmp/bis-target); prints the segment's result line
R=/Users/prof/source/buus-fury-decomp
B=/tmp/bis
mkdir -p $B/roms
rm -rf $B/engine/crates
cd $R && git archive "$1" engine | tar -x -m -C $B
[ -e $B/roms/buu.gba ] || ln -s $R/roms/buu.gba $B/roms/buu.gba
cd $B/engine || exit 1
SEGMENT_DIR=$R/$2 SEGMENT=$3 SEGMENT_STRICT=0 cargo test --target-dir /tmp/bis-target -p wf-import --test segments -- --nocapture > /tmp/bis_$1.log 2>&1
grep "^segment\|^error" /tmp/bis_$1.log | cut -c1-260
