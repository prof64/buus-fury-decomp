"""Write a GameData image into a flash save slot with the game's own writer 0x080049F2(gd, slot)
(copy into a stack record, checksum over 0x8A8 bytes at +0x8A8, flash write of 0x8B0 bytes at slot*0x8B0+0x10)."""
import sys
sys.path.insert(0,str(__import__('pathlib').Path(__file__).resolve().parents[2] / 're' / 'buu'))
from common import u16, u32, ROOT
import emutools as T

def call_long(e, func, r=(0,0,0,0), frames=600):
    blob = (ROOT / 'work' / 'buu' / 'stub.bin').read_bytes()
    for i in range(0, len(blob), 2):
        e.w16(T.STUB + i, blob[i] | (blob[i + 1] << 8))
    vals = list(r) + [0,0,0,0] + [func | 1, T.HOOK_TARGET | 1]
    for i, v in enumerate(vals):
        e.w32(T.ARGS + 4 * i, v)
    hi, lo = T._bl(T.HOOK_SITE, T.STUB)
    o1, o2 = e.r16(T.HOOK_SITE), e.r16(T.HOOK_SITE + 2)
    e.w32(T.MAILBOX, 0xDEADBEEF)
    e.w16(T.HOOK_SITE, hi); e.w16(T.HOOK_SITE + 2, lo)
    n=0
    while e.r32(T.MAILBOX) == 0xDEADBEEF and n < frames:
        e.run(1); n+=1
        if n==1:
            pass
    e.w16(T.HOOK_SITE, o1); e.w16(T.HOOK_SITE + 2, o2)
    for i in range(0, len(blob), 2):
        e.w16(T.STUB + i, u16(T.STUB + i))
    for i in range(len(vals)):
        e.w32(T.ARGS + 4 * i, u32(T.ARGS + 4 * i))
    return e.r32(T.MAILBOX), n
