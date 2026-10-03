#!/bin/sh
# segs.sh DIR [SEGMENT[,SEGMENT]]: replay a directory of segments (relative to the repo, e.g. work/db/db1/segs or
# data/segments) on wf-core; prints the segments that do not match to their end, the number that do, errors.
R=/Users/prof/source/legacy-of-goku-x/.claude/worktrees/re-dashboard
cd $R/engine || exit 1
if [ -n "$2" ]; then export SEGMENT=$2; fi
SEGMENT_DIR=$R/$1 SEGMENT_STRICT=0 cargo test -p wf-import --test segments -- --nocapture > /tmp/db_segs_$$.log 2>&1
grep -A7 "^error" /tmp/db_segs_$$.log | cut -c1-300 | head -40
grep "^segment\|import note\|script error" /tmp/db_segs_$$.log | grep -v "divergence None" | cut -c1-330
echo "exact: $(grep -c 'divergence None' /tmp/db_segs_$$.log) / $(grep -c '^segment' /tmp/db_segs_$$.log)"
rm -f /tmp/db_segs_$$.log
