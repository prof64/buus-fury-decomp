import sys
sys.path.insert(0, '/Users/prof/source/legacy-of-goku-x/.claude/worktrees/re-dashboard/re/buu/menu')
from cap import Cap, program, contact, OUT
from scenarios_minigame import native
which = sys.argv[1]
items = {'watch': [1, ('n', 0xA7)], 'power': [('n', 0xC6)], 'punch': [0, ('n', 0xC7)], 'punch1': [1, ('n', 0xC7)],
         'credits': [('n', 0xD1)]}[which]
c = Cap('buu_snakeway_start.ss')
c.run(['-:3'])
native(items)(c.e)
c.frames = []
c.screens = []
keys = sys.argv[2].split(',') if len(sys.argv) > 2 else ['-:400']
c.run(keys, lambda e: dict(lf=e.r32(0x03000F94), mode=e.r32(0x03000F90)))
for i, f in enumerate(c.frames):
    if f['snd']:
        print(i, f['keys'], f['snd'])
hs = [f['h'] for f in c.frames]
ch = [i for i in range(1, len(hs)) if hs[i] != hs[i - 1]]
print('changes', len(ch), ch[:200])
step = max(1, len(c.screens) // 48)
contact(c.screens, list(range(0, len(c.screens), step))[:48], OUT / f'd1_{which}.png', cols=8)
c.save('d1_' + which)
