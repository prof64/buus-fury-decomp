"""yakon_window.py [map=40] -- the post-combo window: Yakon's state 0x323 (Wait 60 after the 6-punch combo) has the
on-hit tree  Sequence{ IfAttack(1 energy) -> IncCounter ; SubState(5) -> ApplyDamage }, i.e. energy hits there
are NOT absorbed but do full damage (and, counter != 0, cancel the mouth beam).  Force state 0x323 while a held
Kamehameha Lv2 (and, second run, Energy Blasts) hits him; log HP and state per frame -> work/buu/yakon_window.json.
Also checks state 0x325 (summon / wait for Mini-Yakons) and 0x326 (mouth beam): Idle on-hit -> no effect."""
import sys
import json
from common import *
from emutools import catch
from forcespawn import setpos, etype
from ents import PLAYER, enemies
from bv_fighters_lib import spawn2, pos, set_state, REPLACE, PUSH, REC

DMG, DMG_RET, HEAL, APPLY = 0x0800E9D8, 0x0800EAEC, 0x0803A79C, 0x080187A4
MAP40 = 'map=40' in sys.argv
if MAP40:
    e = emu(WORK / 'yakon_map40.ss')
    o = [x for x in enemies(e) if etype(e, x) == 0x91][0]
else:
    e, o, ok = spawn2(0x91)
base = e.save()
s32 = lambda v: v - (1 << 32) if v & 0x80000000 else v
out = {}
RUNS = [('kame_0x323', 15, 2, 60, lambda f: KEY.B, 0x323, 40),
        ('blast_0x323', 7, 0, 50, lambda f: KEY.B if f % 48 < 2 else 0, 0x323, 20),
        ('kame_0x31d', 15, 2, 60, lambda f: KEY.B, None, 40),
        ('kame_0x326', 15, 2, 60, lambda f: KEY.B, 0x326, 40),
        ('punch_0x323', None, 0, 18, lambda f: KEY.A if f % 20 < 2 else 0, 0x323, 20)]
for name, sk, lv, dist, keyfn, st, at in RUNS:
    e.load(base)
    p = e.r32(PLAYER)
    e.w32(o + 0x28c, 1000 << 10)
    if sk is not None:
        e.w8(REC + 0xc, 1); e.w8(REC + 0xd, sk); e.w8(REC + 0x15, 0); e.w8(REC + 0x2e, lv); e.w8(REC + 0x2f, lv)
    ev, hp = [], []
    pend = {}

    def cb(e, pc, f):
        if pc == DMG and e.reg(0) == o:
            at_ = e.r32(e.reg(1) + 8)
            pend['h'] = e.reg(1)
            pend['ev'] = dict(f=f, ev='dmg', kind=e.r8(at_), sub=e.r8(at_ + 1), mult=e.r32(e.reg(1) + 4),
                              hp1024_before=s32(e.r32(o + 0x28c)))
        elif pc == DMG_RET and pend.get('h') == e.reg(4):
            d = pend.pop('ev'); pend.pop('h')
            d['final'] = e.r32(e.reg(4) + 0x18)
            ev.append(d)
        elif pc in (HEAL, APPLY) and e.reg(1) == o:
            ev.append(dict(f=f, ev='heal' if pc == HEAL else 'apply'))
        elif pc in (REPLACE, PUSH) and e.reg(0) == o + 0x238:
            ev.append(dict(f=f, ev='push' if pc == PUSH else 'goto', state=hex(e.reg(1))))

    for f in range(160):
        e.w16(REC + 0x16, 9999); e.w16(REC + 0x1a, 999); e.w16(REC + 0x1c, 999)
        bx, by = pos(e, o)
        setpos(e, p, bx - dist, by)
        e.w8(p + 0x10, 3)
        if st is not None and f == at:
            set_state(e, o, st)     # runs one frame (call injection)
        catch(e, [DMG, DMG_RET, HEAL, APPLY, REPLACE, PUSH], 1, lambda _: keyfn(f), lambda e, pc: cb(e, pc, f))
        hp.append(s32(e.r32(o + 0x28c)))
    dm = [x for x in ev if x['ev'] == 'dmg']
    print(f'== {name}: hits {len(dm)} heals {sum(x["ev"] == "heal" for x in ev)} applies '
          f'{sum(x["ev"] == "apply" for x in ev)} hp {hp[0] / 1024:.2f} -> {hp[-1] / 1024:.2f} min {min(hp) / 1024:.2f}')
    print('   ', [x for x in ev if x['ev'] in ('goto', 'push')][:8])
    out[name] = dict(events=ev, hp1024=hp)
(WORK / f'yakon_window{"_map40" if MAP40 else ""}.json').write_text(json.dumps(out))
