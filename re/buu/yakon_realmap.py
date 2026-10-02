"""yakon_realmap.py -- warp Goku to Yakon's Planet (world 8, map 40) from buu_snakeway_start.ss and report whether
Yakon (0x91) spawns there through his real spawn record 0x0807D914 (condition word 0x1); if not, retry with the
record's condition patched to the always-true one of the SW-forest Majin Fighter.  Saves work/buu/yakon_map40.ss
when he is present."""
from common import *
from warp import warp
from ents import enemies
from forcespawn import etype, spawn_records, REC

e = emu('buu_snakeway_start.ss')
e.w16(REC + 0x16, 9999); e.w16(REC + 0x18, 9999)
REC40 = 0x0807D914
print('record', [hex(e.r32(REC40 + i)) for i in range(0, 0x20, 4)])
ok = warp(e, 8, 40, 112, 200)
print('warped', ok, 'area', hex(e.r32(0x03001980)), 'types', [hex(etype(e, o)) for o in enemies(e)])
if not any(etype(e, o) == 0x91 for o in enemies(e)):
    sw = [s for s in spawn_records() if (s['world'], s['map']) == (8, 3) and s['type'] == 0x44]
    cond = e.r32(int(sw[0]['record'], 16) + 4)
    e = emu('buu_snakeway_start.ss')
    e.w16(REC + 0x16, 9999); e.w16(REC + 0x18, 9999)
    e.w32(REC40 + 4, cond)
    ok = warp(e, 8, 40, 112, 200)
    print('patched cond', hex(cond), 'warped', ok, 'types', [hex(etype(e, o)) for o in enemies(e)])
ys = [o for o in enemies(e) if etype(e, o) == 0x91]
if ys:
    o = ys[0]
    print('yakon', hex(o), 'hp', e.r32(o + 0x28c) / 1024, 'pos', e.r32(o + 0x1a0) / 256, e.r32(o + 0x1a4) / 256)
    e.save_file(WORK / 'yakon_map40.ss')
    e.screenshot(str(WORK / 'yakon_map40.png'), scale=2)
