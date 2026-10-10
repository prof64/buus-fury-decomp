import sys; sys.path.insert(0, '/Users/prof/source/buus-fury-decomp/re/buu/menu')
from cap import *
c = Cap('buu_snakeway_start.ss')
REC = 0x03001560
c.e.w16(REC + 0x2c, 5)
c.run(['-:2', 'START:1', '-:40', 'A:1', '-:20', 'DOWN:1', '-:10', 'A:1', '-:10', 'UP:1', '-:10', 'RIGHT:1', '-:10', 'A:1', '-:10', 'B:1', '-:20', 'A:1', '-:10', 'A:1', '-:20'])
for i, f in enumerate(c.frames):
    if f['snd']:
        print(i, f['keys'], f['snd'])
hs = [f['h'] for f in c.frames]
ch = [i for i in range(1, len(hs)) if hs[i] != hs[i - 1]]
print('changes', ch)
contact(c.screens, [i for i in ch if i > 25][:48], OUT / 'alloc_contact.png', cols=6)
c.save('explore_alloc')
