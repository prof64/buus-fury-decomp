#!/bin/sh
# gates_with.sh COMMIT FILE...: the gated segments (data/segments, strict) on COMMIT's engine (a clean export in
# /tmp/bis, private target /tmp/bis-target) with the given working-tree files (paths under engine/) copied over it -
# one agent's changes checked without the other agents' uncommitted edits
R=/Users/prof/source/legacy-of-goku-x/.claude/worktrees/re-dashboard
B=/tmp/bis
C=$1
shift
mkdir -p $B/roms
rm -rf $B/engine/crates
cd $R && git archive "$C" engine | tar -x -m -C $B
[ -e $B/roms/buu.gba ] || ln -s $R/roms/buu.gba $B/roms/buu.gba
for f in "$@"; do cp $R/engine/$f $B/engine/$f; done
cd $B/engine || exit 1
SEGMENT_DIR=${GATES_DIR:-$R/data/segments} SEGMENT_STRICT=1 cargo test --target-dir /tmp/bis-target -p wf-import --test segments -- --nocapture > /tmp/gates_with.log 2>&1
grep "test result\|segments below" /tmp/gates_with.log | cut -c1-600
