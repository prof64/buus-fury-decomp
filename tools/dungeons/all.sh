#!/bin/sh
# all.sh: replay the four dungeons' full local cuts (work/db/dbN/segs) on wf-core (see segs.sh)
R=/Users/prof/source/legacy-of-goku-x/.claude/worktrees/re-dashboard
for d in ${DUNGEONS:-db1 db2 db3 db4}; do
    echo "== $d"
    sh $R/tools/dungeons/segs.sh work/db/$d/segs 2>&1 | cut -c1-235
done
