#!/bin/sh
# rngdiff.sh SEG FRAME [FRAME2] -- the game's RNG calls (callers) vs wf-core's (source lines) for frames of a chapter
# segment in work/ship/segs_full
R=/Users/prof/source/buus-fury-decomp
B=${3:-$2}
cd $R/work/ship && ../../.venv/bin/python ../../re/buu/ship/seg_rng.py segs_full $1 $2 $B 2>&1 | cut -c1-1500
cd $R/engine && SEGMENT_DIR=$R/work/ship/segs_full SEGMENT=$1 SEGMENT_STRICT=0 SEGMENT_RNG=1 SEGMENT_TRACE=$2..$B cargo test --release -p wf-import --test segments -- --nocapture 2>&1 | grep "rng call\|  t[0-9]\|enemies\|^segment" | cut -c1-400
