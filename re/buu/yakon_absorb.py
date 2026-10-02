"""yakon_absorb.py [scenario ...] -- force-spawn Yakon (0x91) and hit him with one attack type, logging his HP
every frame, every ComputeDamage on him (final damage hit+0x18), every HealFraction (0x0803A79C) / ApplyDamage
(0x080187A4) onHit call, and his AI state changes.  Writes work/buu/yakon_absorb.json.

scenarios: punch, blast, kame (Kamehameha Lv2 held), bigbang (Lv2 charged), masenko (Lv2), sword, flash
(Final Flash Lv2), dfist (Dragon Fist).  options: frames=N hp=H (start HP) tag=T (output suffix) map=40
(fight in the real arena, Yakon's Planet 8-40, from work/buu/yakon_map40.ss).  The player is pinned next to / in front of Yakon each frame, HP/EP refilled."""
import sys
import json
from common import *
from emutools import catch
from forcespawn import setpos
from ents import PLAYER
from bv_fighters_lib import spawn2, pos, REPLACE, PUSH, REC

DMG, DMG_RET, HEAL, APPLY = 0x0800E9D8, 0x0800EAEC, 0x0803A79C, 0x080187A4
SC = {
    # name: (skill id or None, level, distance, keyfn)
    'punch': (None, 0, 18, lambda f: KEY.A if f % 20 < 2 else 0),
    'blast': (7, 0, 50, lambda f: KEY.B if f % 48 < 2 else 0),
    'kame': (15, 2, 60, lambda f: KEY.B if f % 400 < 340 else 0),
    'bigbang': (2, 2, 70, lambda f: KEY.B if f % 400 < 200 else 0),
    'masenko': (17, 2, 70, lambda f: KEY.B if f % 300 < 150 else 0),
    'sword': (9, 0, 20, lambda f: KEY.B if f % 200 < 100 else 0),
    'flash': (10, 2, 60, lambda f: KEY.B if f % 200 < 60 else 0),
    'dfist': (5, 0, 24, lambda f: KEY.B if f % 300 < 120 else 0),
    # Energy Blasts for 300 f (from 50 px), then punches (from 18 px)
    'mix': (7, 0, lambda f: 50 if f < 300 else 18, lambda f: (KEY.B if f % 48 < 2 else 0) if f < 300 else (KEY.A if f % 20 < 2 else 0)),
}
OPT = dict(a.split('=') for a in sys.argv[1:] if '=' in a)
FRAMES = int(OPT.get('frames', 900))
HP0 = float(OPT['hp']) if 'hp' in OPT else None      # start Yakon at this many HP
TAG = OPT.get('tag', '')
names = [a for a in sys.argv[1:] if '=' not in a] or list(SC)

if OPT.get('map') == '40':     # the real arena (yakon_realmap.py -> work/buu/yakon_map40.ss)
    from ents import enemies
    from forcespawn import etype
    e0 = emu(WORK / 'yakon_map40.ss')
    o = [x for x in enemies(e0) if etype(e0, x) == 0x91][0]
else:
    e0, o, ok = spawn2(0x91)
assert o, 'no yakon'
base = e0.save()
vt = e0.r32(o)
out = {'obj': hex(o), 'vtable': hex(vt), 'max_hp': e0.r32(0x08711274 + 28 * 0x91 + 4),
       'resist_melee': e0.r8(o + 0x1b8), 'resist_energy': e0.r8(o + 0x1b9)}
print(out)
s32 = lambda v: v - (1 << 32) if v & 0x80000000 else v

for name in names:
    sk, lv, dist, keyfn = SC[name]
    e = e0
    e.load(base)
    p = e.r32(PLAYER)
    if HP0 is not None:
        e.w32(o + 0x28c, int(HP0 * 1024))
    if sk is not None:
        e.w8(REC + 0xc, 1); e.w8(REC + 0xd, sk); e.w8(REC + 0x15, 0); e.w8(REC + 0x2e, lv); e.w8(REC + 0x2f, lv)
    ev = []
    hp = []
    pend = {}

    def cb(e, pc, f):
        if pc == DMG and e.reg(0) == o:
            h = e.reg(1)
            at = e.r32(h + 8)
            pend['h'] = h
            pend['ev'] = dict(f=f, ev='dmg', kind=e.r8(at), sub=e.r8(at + 1), mult=e.r32(h + 4),
                              hp_before=e.r32(o + 0x28c) / 1024)
        elif pc == DMG_RET and pend.get('h') == e.reg(4):
            d = pend.pop('ev')
            d['final'] = e.r32(e.reg(4) + 0x18)
            ev.append(d)
            pend.pop('h')
        elif pc == HEAL and e.reg(1) == o:
            m = e.reg(2)
            vf = e.r32(o)
            ev.append(dict(f=f, ev='heal', mult=e.r32(m + 4), dmg=e.r32(m + 0x18), frac=e.r32(e.reg(0) + 0xc),
                           hp=e.r32(o + 0x28c) / 1024, vfunc60=hex(vf + 0x60 + e.r32(vf + 0x60))))
        elif pc == APPLY and e.reg(1) == o:
            ev.append(dict(f=f, ev='apply', dmg=e.r32(e.reg(2) + 0x18), hp=e.r32(o + 0x28c) / 1024))
        elif pc in (REPLACE, PUSH) and e.reg(0) == o + 0x238:
            ev.append(dict(f=f, ev='push' if pc == PUSH else 'goto', state=hex(e.reg(1))))

    for f in range(FRAMES):
        e.w16(REC + 0x16, 9999)
        e.w16(REC + 0x1a, 999); e.w16(REC + 0x1c, 999)
        bx, by = pos(e, o)
        setpos(e, p, bx - (dist(f) if callable(dist) else dist), by)
        e.w8(p + 0x10, 3)
        catch(e, [DMG, DMG_RET, HEAL, APPLY, REPLACE, PUSH], 1, lambda _: keyfn(f), lambda e, pc: cb(e, pc, f))
        hp.append(round(s32(e.r32(o + 0x28c)) / 1024, 3))
    dm = [x for x in ev if x['ev'] == 'dmg']
    print(f'== {name}: hits {len(dm)} heals {sum(x["ev"] == "heal" for x in ev)} applies '
          f'{sum(x["ev"] == "apply" for x in ev)} hp {hp[0]} -> {hp[-1]} min {min(hp)} max {max(hp)}')
    for x in ev[:25]:
        print('  ', x)
    out[name] = dict(events=ev, hp=hp)
(WORK / f'yakon_absorb{TAG}.json').write_text(json.dumps(out))
