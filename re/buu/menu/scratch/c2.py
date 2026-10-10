import sys
sys.path.insert(0, '/Users/prof/source/buus-fury-decomp/re/buu/menu')
sys.path.insert(0, '/Users/prof/source/buus-fury-decomp/tools/story_playthrough')
from cap import *
from route import start_emu
e = start_emu('reset')
c = Cap(e=e)
prog = sys.argv[1].split(',') if len(sys.argv) > 1 else ['-:400', 'START:2', '-:80', 'DOWN:2', '-:30', 'UP:2', '-:30', 'A:2', '-:80', 'DOWN:2', '-:30', 'A:2', '-:60']
c.run(prog, lambda e: dict(lf=e.r32(0x03000F94), mode=e.r32(0x03000F90)))
hs = [f['h'] for f in c.frames]
ch = [i for i in range(1, len(hs)) if hs[i] != hs[i - 1]]
print(len(ch), ch[:200])
print([(i, f['snd']) for i, f in enumerate(c.frames) if f['snd']])
sel = ch if len(ch) < 48 else ch[::max(1, len(ch) // 48)]
contact(c.screens, sel[:48], OUT / 'title_contact.png', cols=8)
c.e.save_file(OUT / 'title_end.ss')
