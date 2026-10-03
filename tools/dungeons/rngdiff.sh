#!/bin/sh
# rngdiff.sh DUNGEON SEG FRAME [FRAME2] -- the game's RNG calls (callers) vs wf-core's (source lines) for frames of a
# dungeon segment in work/db/<DUNGEON>/segs
R=/Users/prof/source/legacy-of-goku-x/.claude/worktrees/re-dashboard
B=${4:-$3}
cd $R && .venv/bin/python re/buu/ship/seg_rng.py work/db/$1/segs $2 $3 $B 2>&1 | cut -c1-1800
cd $R/engine && SEGMENT_DIR=$R/work/db/$1/segs SEGMENT=$2 SEGMENT_STRICT=0 SEGMENT_RNG=1 SEGMENT_TRACE=$3..$B cargo test -p wf-import --test segments -- --nocapture 2>&1 | grep "rng call\|  t[0-9]\|enemies\|^segment" | cut -c1-420
