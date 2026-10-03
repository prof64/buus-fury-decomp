#!/bin/sh
# trace.sh DUNGEON SEG A..B [GREP]: the harness trace of frames A..B of a segment in work/db/<DUNGEON>/segs
# (SEGMENT_TRACE + SEGMENT_VERBOSE; lines filtered by GREP, default the frame / menu / divergence lines)
R=/Users/prof/source/legacy-of-goku-x/.claude/worktrees/re-dashboard
cd $R/engine || exit 1
G=${4:-'^  t\|^    f\|^segment'}
SEGMENT_DIR=$R/work/db/$1/segs SEGMENT_VRAM=${SEGMENT_VRAM:-1} SEGMENT=$2 SEGMENT_TRACE=$3 SEGMENT_VERBOSE=4 SEGMENT_STRICT=0 cargo test -p wf-import --test segments -- --nocapture 2>&1 | grep "$G" | cut -c1-${5:-260}
