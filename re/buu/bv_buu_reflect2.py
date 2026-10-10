"""Reflect details: player ki blast into Super Buu 0x74 in stance 0x255; follow every spawned ki blast (x, y,
owner word +0x?) and all damage events (target/attacker), then fire a second blast during 0x256."""
from common import *
from bv_buu_lib import *

e, o = setup(0x74, dx=-70, dy=0)
p = e.r32(PLAYER)
e.w8(REC + 0x15, 0)
e.w8(p + 0x10, 3)
force(e, o, 0x255)
r = trace(e, o, 160, keys=lambda f: KEY.B if (5 <= f < 8 or 45 <= f < 48) else 0, nwords=0x16)
print('runs', state_runs(r))
print('player', hex(p), 'boss', hex(o))
for d in r['damage']:
    print(' dmg', d['frame'], 'target', d['target'], 'attacker', d['attacker'], 'sub', d['atk_sub'], 'mult', d['mult'], 'final', d['final'])
for ob, sn in r['objs'].items():
    if sn[0][1][0] != 0x0804b454:
        continue
    xs = [(f, s32v(w[8]) / 256, [hex(w[k]) for k in (0x13, 0x14, 0x15)]) for f, w in sn[::3]]
    print(' blast', hex(ob), 'life', r['life'].get(ob), xs[:12])
print('player hp loss', [(f, l) for f, l, g, c, v in r['player'] if l])
print('boss hp', r['boss'][0][-1], '->', r['boss'][-1][-1])
