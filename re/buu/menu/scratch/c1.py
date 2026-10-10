import sys
sys.path.insert(0, '/Users/prof/source/buus-fury-decomp/re/buu/menu')
sys.path.insert(0, '/Users/prof/source/buus-fury-decomp/re/buu')
from cap import *
from script_nat_b_lib import asm, start_call, CODE_AT, HDR_AT, INTERP_AT


def inject(e, items):
    code = asm(items)
    blob = b'\x01\x00\x00' + code
    blob += b'\x00' * (-len(blob) % 4)
    for i in range(0, len(blob), 4):
        e.w32(CODE_AT + i, int.from_bytes(blob[i:i + 4], 'little'))
    e.w32(HDR_AT, CODE_AT)
    for i in range(0, 0x6c, 4):
        e.w32(INTERP_AT + i, 0)
    start_call(e, 0x08004098, (INTERP_AT, HDR_AT, 0, 0))


c = Cap('buu_snakeway_start.ss')
c.e.w32(0x03001068 + 0x4F4, 5000)
items = [0, 0, 50, 0, ('n', 0xCD), 0, 1, 120, 0, ('n', 0xCD), 1, 5, 300, 0, ('n', 0xCD), ('n', 0xCE)]
c.run(['-:3'])
inject(c.e, items)
c.frames = []; c.screens = []
c.run(['-:60', 'DOWN:1', '-:20', 'A:1', '-:30', 'A:1', '-:30', 'B:1', '-:30', 'B:1', '-:80'], lambda e: dict(lf=e.r32(0x03000F94)))
for i, f in enumerate(c.frames):
    if f['snd']:
        print(i, f['snd'])
hs = [f['h'] for f in c.frames]
ch = [i for i in range(1, len(hs)) if hs[i] != hs[i - 1]]
print(ch)
print(''.join(str(min(9, c.frames[i]['lf'] - c.frames[i-1]['lf'])) for i in range(1, len(c.frames))))
contact(c.screens, [i for i in ch][:48], OUT / 'shop_contact.png', cols=6)
