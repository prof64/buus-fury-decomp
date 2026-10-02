from common import *
from emutools import _bl, HOOK_SITE, STUB
e = emu('buu_snakeway_start.ss')
blob = (WORK / 'stub.bin').read_bytes()
for i in range(0, len(blob), 2):
    e.w16(STUB + i, blob[i] | (blob[i + 1] << 8))
print('stub', hex(e.r32(STUB)))
hi, lo = _bl(HOOK_SITE, STUB)
e.w16(HOOK_SITE, hi); e.w16(HOOK_SITE + 2, lo)
print('site', hex(e.r16(HOOK_SITE)), hex(e.r16(HOOK_SITE + 2)))
b1 = e.breakpoint(0x08007052)
b2 = e.breakpoint(STUB)
b3 = e.breakpoint(0x0800965c)
e.run(1)
print([(hex(h['pc']), h['frame']) for h in e.hits()])
