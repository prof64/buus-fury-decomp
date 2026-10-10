"""yakon_record.py -- emulator recordings of Yakon (0x91) taking hits, for the sim regression test
(site/tests/battle/buu_yakon.test.ts) -> data/battle/buu_yakon_absorb.json.

Per run: start HP (1/1024 units), then every ComputeDamage 0x0800E9D8 on Yakon: his top AI state at that moment
(stack at obj+0x238: depth +4, entries {runtime, def} from +8, def = 0x0870F514 + 8 * state), attack kind/sub,
mult, final damage (hit+0x18 at the return 0x0800EAEC) and his HP (signed) just before; at the end his HP and
whether the death branch of the damage vfunc (0x0800FA56) ran.

Runs (force-spawned via spawn2 unless 'map40', which uses the real arena state work/buu/yakon_map40.ss from
yakon_realmap.py):
  kame_absorb   map40, HP 1000, Kamehameha Lv2 held: every tick heals (low 16 bits < 0x8000) -> full HP
  blast_wrap    force, HP 1000, Energy Blasts of 31..34 HP: >= 32 HP wraps negative and hurts
  blast_neg     force, HP 20, blasts drive HP to -40 (alive), then a punch kills him
  kame_window   map40, HP 800, Kamehameha Lv2 held, state forced to 0x323 at f20 (before the beam connects): full damage, dies
  punch         force, full HP, punches 7/128"""
import json
from common import *
from emutools import catch
from forcespawn import setpos, etype
from ents import PLAYER, enemies
from bv_fighters_lib import spawn2, pos, set_state, REC

DMG, DMG_RET, DEATH = 0x0800E9D8, 0x0800EAEC, 0x0800FA56
DEFS = 0x0870F514
s32 = lambda v: v - (1 << 32) if v & 0x80000000 else v


def top_state(e, o):
    b = o + 0x238
    n = e.r32(b + 4)
    return (e.r32(b + 8 + 8 * (n - 1) + 4) - DEFS) // 8 if n else -1


def run(e, o, name, hp0, frames, skill, lv, dist, keyfn, force=None):
    p = e.r32(PLAYER)
    if hp0 is not None:
        e.w32(o + 0x28c, hp0 << 10)
    if skill is not None:
        e.w8(REC + 0xc, 1); e.w8(REC + 0xd, skill); e.w8(REC + 0x15, 0); e.w8(REC + 0x2e, lv); e.w8(REC + 0x2f, lv)
    hits, pend, dead = [], {}, []

    def cb(e, pc, f):
        if pc == DMG and e.reg(0) == o:
            h = e.reg(1)
            at = e.r32(h + 8)
            pend[h] = dict(f=f, state=top_state(e, o), kind=e.r8(at), sub=e.r8(at + 1), mult=e.r32(h + 4),
                           hp=s32(e.r32(o + 0x28c)))
        elif pc == DMG_RET and e.reg(4) in pend:
            d = pend.pop(e.reg(4))
            d['dmg'] = e.r32(e.reg(4) + 0x18)
            hits.append(d)
        elif pc == DEATH and e.reg(4) == o:
            dead.append(f)

    start = s32(e.r32(o + 0x28c))
    for f in range(frames):
        e.w16(REC + 0x16, 9999); e.w16(REC + 0x1a, 999); e.w16(REC + 0x1c, 999)
        if not dead:
            bx, by = pos(e, o)
            setpos(e, p, bx - dist(f), by)
            e.w8(p + 0x10, 3)
        if force and f == force[1]:
            set_state(e, o, force[0])
        catch(e, [DMG, DMG_RET, DEATH], 1, lambda _: keyfn(f) if not dead else 0, lambda e, pc: cb(e, pc, f))
    r = dict(name=name, hp_start=start, hits=hits, hp_end=s32(e.r32(o + 0x28c)), dead=bool(dead),
             dead_frame=dead[0] if dead else None)
    print(name, 'hits', len(hits), 'hp', start / 1024, '->', r['hp_end'] / 1024, 'dead', r['dead'],
          'states', sorted({h['state'] for h in hits}))
    return r


kame = lambda f: KEY.B
blast = lambda f: KEY.B if f % 48 < 2 else 0
punch = lambda f: KEY.A if f % 20 < 2 else 0
out = {'source': 're/buu/yakon_record.py (emulator, Goku from buu_snakeway_start.ss)', 'max_hp': 1725, 'runs': []}

e = emu(WORK / 'yakon_map40.ss')
o = [x for x in enemies(e) if etype(e, x) == 0x91][0]
m40 = e.save()
out['runs'].append(run(e, o, 'kame_absorb', 1000, 140, 15, 2, lambda f: 60, kame))
e.load(m40)
out['runs'].append(run(e, o, 'kame_window', 800, 140, 15, 2, lambda f: 60, kame, force=(0x323, 20)))

e, o, ok = spawn2(0x91)
fs = e.save()
out['runs'].append(run(e, o, 'blast_wrap', 1000, 240, 7, 0, lambda f: 50, blast))
e.load(fs)
out['runs'].append(run(e, o, 'blast_neg', 20, 360, 7, 0, lambda f: 50 if f < 300 else 18,
                       lambda f: blast(f) if f < 300 else punch(f)))
e.load(fs)
out['runs'].append(run(e, o, 'punch', None, 200, None, 0, lambda f: 18, punch))
(ROOT / 'data' / 'battle' / 'buu_yakon_absorb.json').write_text(json.dumps(out, indent=0))
