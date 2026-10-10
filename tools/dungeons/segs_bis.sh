#!/bin/sh
# segs_bis.sh DIR...: replay segment directories (repo-relative) on the clean export in /tmp/bis (see gates_with.sh);
# prints the non-exact segments and the exact count per directory (SEGMENT_VRAM as set)
R=/Users/prof/source/buus-fury-decomp
cd /tmp/bis/engine || exit 1
for d in "$@"; do
    SEGMENT_DIR=$R/$d SEGMENT_STRICT=0 cargo test --target-dir /tmp/bis-target -p wf-import --test segments -- --nocapture > /tmp/segs_bis.log 2>&1
    echo "== $d exact $(grep -c 'divergence None' /tmp/segs_bis.log) / $(grep -c '^segment' /tmp/segs_bis.log)"
    grep "^segment" /tmp/segs_bis.log | grep -v "divergence None" | cut -c1-220
done
