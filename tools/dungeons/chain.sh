#!/bin/sh
# chain.sh [FROM]: rebuild the dungeon chain's recordings from dungeon FROM (2 | 3 | 4; default 2's cut only):
# each dungeon's start state is built on the previous dungeon's real end state (tools/dungeons/dbN_save.py), the
# route is played by the real game (dbN_route.py), the recording is cut into segments (work/db/dbN/segs).
# Logs: work/db/chain.log. Stops at the first failing step.
R=/Users/prof/source/legacy-of-goku-x/.claude/worktrees/re-dashboard
cd $R || exit 1
PY=.venv/bin/python
LOG=work/db/chain.log
: > $LOG
cut() {
    rm -rf work/db/$1/segs
    $PY re/buu/ship/rec_segments.py work/db/$1 work/db/$1/segs --prefix $1 >> $LOG 2>&1 || exit 1
}
run() {
    $PY tools/dungeons/$1_save.py >> $LOG 2>&1 || exit 1
    $PY tools/dungeons/$1_route.py $1 >> $LOG 2>&1 || exit 1
    cut $1
}
FROM=${1:-2}
if [ "$FROM" -le 2 ]; then cut db2; fi
if [ "$FROM" -le 3 ]; then run db3; fi
if [ "$FROM" -le 4 ]; then run db4; fi
echo CHAIN DONE >> $LOG
