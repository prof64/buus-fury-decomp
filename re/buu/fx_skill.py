"""fx_skill.py skill [hold] [frames] [face] [level] [form] -- trace the effect OBJs a player skill draws.

Per frame: every OBJ whose tiles match an fx record (effect id, record index, record address), its world position
relative to the player position (obj+0x1A0), flips, OBJ mode; unmatched 8bpp OBJs with a ROM hit for their raw gfx.
Writes work/buu/fx/skill_<id>.json and a screenshot strip."""
import sys
import json
from common import *
from combat import setup, pin, REC
from fxcap import FxIndex, capture, camera

sk = int(sys.argv[1], 0)
hold = int(sys.argv[2]) if len(sys.argv) > 2 else 60
frames = int(sys.argv[3]) if len(sys.argv) > 3 else 120
face = int(sys.argv[4]) if len(sys.argv) > 4 else 3
lvl = int(sys.argv[5]) if len(sys.argv) > 5 else 0
form = int(sys.argv[6]) if len(sys.argv) > 6 else -1
dist = int(sys.argv[7]) if len(sys.argv) > 7 else 80
SNAPS = {int(x) for x in sys.argv[8].split(',')} if len(sys.argv) > 8 else set()
e = emu('buu_fight_swforest.ss')
e.w8(REC + 0xc, 1); e.w8(REC + 0xd, sk); e.w8(REC + 0x15, 0)
e.w8(REC + 0x2e, lvl)   # level of skill slot 0
e.w16(REC + 0x1c, 999); e.w16(REC + 0x1a, 999)
if form >= 0:
    e.w8(REC + 0x14, form)
p, en = setup(e)
e.w8(p + 0x10, face)
ix = FxIndex()
s = lambda v: v - (1 << 32) if v & 0x80000000 else v
log = []
S = Shots(e, f'fx/skill_{sk}', cols=6)
(WORK / 'fx').mkdir(exist_ok=True)
dx, dy = {3: (dist, 0), 2: (-dist, 0), 0: (0, dist), 1: (0, -dist)}[face]
for f in range(frames):
    if en:
        pin(e, en[0], p, dx, dy)
        e.w32(en[0] + 0x28c, 9999 << 10)
    e.w16(REC + 0x16, e.r16(REC + 0x18))
    e.w16(REC + 0x1a, 999)
    e.set_keys(KEY.B if f < hold else 0)
    e.run(1)
    px, py = s(e.r32(p + 0x1a0)) >> 8, s(e.r32(p + 0x1a4)) >> 8
    objs = capture(e, ix)
    fr = []
    for o in objs:
        if 'fx' in o:
            fr.append(dict(fx=o['fx'][0][0], i=o['fx'][0][1], rec=o['fx'][0][2], rx=o['wx'] - px, ry=o['wy'] - py,
                           hf=o['hflip'], vf=o['vflip'], mode=o['mode'], prio=o['prio'], oam=o['oam'], aff=o['affine']))
        elif o.get('rom_gfx'):
            fr.append(dict(gfx=hex(o['rom_gfx']), W=o['W'], H=o['H'], sx=o['x'], sy=o['y'], oam=o['oam'], mode=o['mode'], aff=o['affine']))
    log.append(dict(f=f, gt=e.r32(0x03000f94), px=px, py=py, cam=camera(e), st=e.r32(p + 0x10) & 0xff, objs=fr))
    if f % 4 == 0 and f < 144:
        S.snap()
    if f in SNAPS:
        e.screenshot(str(WORK / 'fx' / f'skill_{sk}_f{f}.png'), scale=3)
S.write()
(WORK / 'fx' / f'skill_{sk}.json').write_text(json.dumps(log))
for L in log:
    if L['objs']:
        print(L['f'], L['gt'] % 1000, ' | '.join(
            (f"{o['fx'][3:]}#{o['i']}@{o['rx']},{o['ry']}{'H' if o['hf'] else ''}{'V' if o['vf'] else ''}{'m' + str(o['mode']) if o['mode'] else ''}"
             if 'fx' in o else f"raw{o['gfx']}{o['W']}x{o['H']}@{o['sx']},{o['sy']}") for o in L['objs']))
