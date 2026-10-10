import sys
sys.path.insert(0, '/Users/prof/source/buus-fury-decomp/re/buu/menu')
sys.path.insert(0, '/Users/prof/source/buus-fury-decomp/re/buu/menu/emut')
from emut import emut
e = emut('buu_snakeway_start.ss')
for i in range(3):
    e.run(1)
    print('after run: vcount', e.r16(0x04000006), 'cycles mod', e.cycles() % 280896)
# where does the input get read: breakpoint at 0x0803D4A0 (input read) and 0x0803F296 (wait)
bps = [e.breakpoint(a) for a in (0x0803D4A0, 0x0803F296, 0x0803D80A)]
e.hits()
c0 = e.cycles()
e.run(2)
for h in e.hits():
    print(hex(h['addr']), h['cycles'] - c0, h['vcount'])
