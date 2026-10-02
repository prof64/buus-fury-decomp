"""npc_talkprobe.py: place the player next to an NPC in Yemma's office (1-3), press A, catch the NPC message handler."""
import sys
from npc_lib import *

dx = int(sys.argv[1]) if len(sys.argv) > 1 else 0
dy = int(sys.argv[2]) if len(sys.argv) > 2 else 12
face = int(sys.argv[3]) if len(sys.argv) > 3 else 1
e = emu('buu_snakeway_start.ss')
fix = arm_warp(e, 1, 3, 240, 416)
area0 = e.r32(AREA_PTR)
e.set_keys(KEY.RIGHT)
for f in range(600):
    fix()
    e.run(1)
    if e.r32(AREA_PTR) != area0:
        break
e.set_keys(0)
e.run(60)
npc = [o for o in npc_objects(e) if (e.r32(o + 0x1A0) >> 8) == 184][0]
nx, ny = e.r32(npc + 0x1A0) >> 8, e.r32(npc + 0x1A4) >> 8
print('npc', hex(npc), nx, ny, 'foot', e.r8(npc + 0x19C), e.r8(npc + 0x19D), 'event', hex(e.r32(npc + 0x1D0)))
p = e.r32(PLAYER)
print('player foot', e.r8(p + 0x19C), e.r8(p + 0x19D))
e.w32(p + 0x1A0, (nx + dx) << 8)
e.w32(p + 0x1A4, (ny + dy) << 8)
e.w8(p + 0x10, face)
e.run(2)


def cb(e, pc):
    r = [e.reg(i) for i in range(16)]
    box = [s32v(e.r32(r[2] + 4 * i)) for i in range(5)]
    print("bc", hex(r[1]), "lr", hex(r[14]), [s32v(e.r32(r[2] + 4 * i)) for i in range(4)] if r[2] > 0x2000000 else r[2]) if r[1] == 3 else None


n = emutools.catch(e, [0x08011C70], 3, keyfn=lambda f: KEY.A if f == 0 else 0, cb=cb)
print('calls', n)
e.run(30)
print('script ctx npc [0x03001bac]', hex(e.r32(0x03001BAC)), 'npc facing', e.r8(npc + 0x10))
e.screenshot(str(WORK / 'npc_talk.png'))
