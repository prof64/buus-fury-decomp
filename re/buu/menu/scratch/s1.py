import sys; sys.path.insert(0, '/Users/prof/source/legacy-of-goku-x/.claude/worktrees/re-dashboard/re/buu/menu')
from cap import *
c = Cap('buu_snakeway_start.ss')
c.run(['-:5', 'START:1', '-:60', 'R:1', '-:30', 'L:1', '-:30', 'B:1', '-:60'])
for i, f in enumerate(c.frames):
    if f['snd']:
        print(i, f['snd'])
hs = [f['h'] for f in c.frames]
ch = [i for i in range(1, len(hs)) if hs[i] != hs[i - 1]]
print('changes', ch)
contact(c.screens, list(range(4, 60)), OUT / 'start_contact.png', cols=8)
contact(c.screens, list(range(60, 186, 3)), OUT / 'start_contact2.png', cols=8)
c.save('explore_start')
