"""Record golden test vectors from Buu's Fury (headless mGBA) for the battle simulator's unit tests.

Writes data/battle/buu_vectors.json with
  damage[]  : every ComputeDamage 0x0800E9D8 call in the scenarios of re/buu/verify_damage*.py
              (RNG state + all inputs at entry; crit/raw/final damage read at 0x0800EAEC, after the block step)
  levelup[] : LevelUp 0x08013678 calls (RNG state, record before / after)
Every event is re-predicted with re/buu/damage_model.py and the growth formula (notes §3).

usage: .venv/bin/python tools/battle/gen_buu_vectors.py
"""
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(os.path.dirname(HERE))
sys.path.insert(0, os.path.join(ROOT, 're/buu'))
from common import emu, KEY  # noqa: E402
from emutools import catch  # noqa: E402
from damage_model import compute  # noqa: E402
from rng import Rng  # noqa: E402
from combat import setup, pin, REC  # noqa: E402
from ents import enemies  # noqa: E402

ENTRY, EXIT = 0x0800E9D8, 0x0800EAEC   # EXIT: final damage (incl. block) is in [r4+0x18]
LEVELUP = 0x08013678
out = []
pend = []


def make_cb(scenario):
    def cb(e, pc):
        if pc == ENTRY:
            tgt, hit = e.reg(0), e.reg(1)
            atk = e.r32(hit + 0xc)
            ad = e.r32(hit + 8)
            kind = e.r8(ad)
            pend.append(dict(scenario=scenario, rng=[e.r32(0x03004a5c), e.r32(0x03004a60)], kind=kind, sub=e.r8(ad + 1),
                             power=e.r32(ad + 4), mult=e.r32(hit + 4), S=e.r16(atk + 0x1b0), POW=e.r16(atk + 0x1b2),
                             critBonus=e.r32(atk + 0x1bc), E=e.r16(tgt + 0x1b4), E2=e.r16(tgt + 0x1b6),
                             resist=e.r8(tgt + (0x1b8 if kind == 0 else 0x1b9)), blocking=e.r32(tgt + 0x1c8) != 0,
                             blockFactor=e.r16(tgt + 0x1c0), hitDir=e.r8(hit), defFacing=e.r8(tgt + 0x10), _hit=hit))
        elif pend:
            v = pend.pop()
            hit = e.reg(4)
            assert hit == v.pop('_hit')
            v.update(crit=e.r32(hit + 0x10), raw=e.r32(hit + 0x14), dmg=e.r32(hit + 0x18))
            out.append(v)
    return cb


def player_runs():
    """verify_damage.py / verify_damage2.py: Goku vs a pinned Majin Fighter."""
    scen = [('melee', lambda f: KEY.A if f % 8 < 4 else 0, 400, 30, None),
            ('kiblast', lambda f: KEY.B if f % 12 < 3 else 0, 300, 30, None)]

    def mod_crit_def(e, p, o):
        e.w32(p + 0x1bc, 30000)
        e.w16(o + 0x1b4, 250)
        e.w16(o + 0x1b6, 300)
    scen.append(('melee_crit_def200', lambda f: KEY.A if f % 8 < 4 else 0, 300, 22, mod_crit_def))
    scen.append(('enemy_vs_passive_pinned', lambda f: 0, 400, 22, None))
    scen.append(('enemy_vs_block_pinned', lambda f: KEY.R, 400, 22, None))
    for name, keys, frames, dy, mod in scen:
        e = emu('buu_fight_swforest.ss')
        e.w16(REC + 0x1c, 999)
        e.w16(REC + 0x1a, 999)
        p, en = setup(e)
        e.w8(p + 0x10, 0)
        cb = make_cb(name)
        for f in range(frames):
            pin(e, en[0], p, 0, dy)
            if mod:
                mod(e, p, en[0])
            e.w16(REC + 0x16, e.r16(REC + 0x18))
            catch(e, [ENTRY, EXIT], 1, lambda _f, f=f: keys(f), cb)


def enemy_runs():
    """verify_damage3.py: enemies attack a standing / blocking Goku (no pinning)."""
    for name, keys in (('enemy_vs_standing', 0), ('enemy_vs_blocking', KEY.R)):
        e = emu('buu_fight_swforest.ss')
        cb = make_cb(name)
        for f in range(500):
            e.w16(REC + 0x16, e.r16(REC + 0x18))
            catch(e, [ENTRY, EXIT], 1, lambda _f: keys, cb)


def check_damage():
    ok = 0
    for v in out:
        dmg, crit, raw = compute(Rng(*v['rng']), v['kind'], v['sub'], v['power'], v['mult'], v['S'], v['POW'],
                                 v['critBonus'], v['E'], v['E2'], v['resist'],
                                 v['blocking'] and (v['hitDir'] ^ 1) == v['defFacing'], v['blockFactor'])
        good = dmg == v['dmg'] and crit == v['crit'] and raw == v['raw']
        ok += good
        if not good:
            print('MISMATCH', v, 'predicted', dmg, crit, raw, file=sys.stderr)
    return ok


def growth_rows():
    c = json.load(open(os.path.join(ROOT, 'data/buu/characters.json')))
    return {ch['index']: ch['growth']['raw_u32'] for ch in c['characters']}


def rec_snapshot(e):
    return dict(hp=e.r16(REC + 0x16), maxHp=e.r16(REC + 0x18), ep=e.r16(REC + 0x1a), maxEp=e.r16(REC + 0x1c),
                points=e.r16(REC + 0x2c), xp=e.r32(REC + 0x24))


def levelup_vectors():
    res = []
    for xp in (2874, 4200, 9000):
        e = emu('buu_fight_swforest.ss')
        e.w32(REC + 0x24, xp)
        for o in enemies(e):
            e.w32(o + 0x28c, 1 << 10)
        snap = {}

        def cb(e, pc):
            if snap:
                return
            snap.update(rng=[e.r32(0x03004a5c), e.r32(0x03004a60)], old=e.reg(1), new=e.reg(2), rec=e.reg(0),
                        before=rec_snapshot(e))
        f = 0
        while not snap and f < 400:
            catch(e, [LEVELUP], 1, lambda _f, f=f: KEY.A if f % 8 < 4 else 0, cb)
            f += 1
        if not snap:
            print('no level-up for xp', xp, file=sys.stderr)
            continue
        e.run(2)
        after = rec_snapshot(e)
        res.append(dict(char=e.r8(0x03001068), old_level=snap['old'], new_level=snap['new'], rng=snap['rng'],
                        recordPtr=snap['rec'], before=snap['before'], after=after))
    return res


def check_levelup(vs):
    g_all = growth_rows()
    ok = 0
    for v in vs:
        g = g_all[v['char']]
        r = Rng(*v['rng'])
        hp, ep = v['before']['maxHp'], v['before']['maxEp']
        for _ in range(v['old_level'], v['new_level']):
            a, b, c, d = r.randn(256), r.randn(256), r.randn(256), r.randn(256)
            hp = hp + ((((g[0] * a) >> 8) + g[1]) >> 16)
            hp = hp + ((hp * (((g[2] * b) >> 8) + g[3])) >> 16)
            ep = ep + (((g[4] * c) + g[5]) >> 16)
            ep = ep + ((ep * (((g[6] * d) >> 8) + g[7])) >> 16)
        pts = v['before']['points'] + 3 * (v['new_level'] - v['old_level'])
        a = v['after']
        good = (hp, ep, hp, ep, pts) == (a['maxHp'], a['maxEp'], a['hp'], a['ep'], a['points'])
        ok += good
        if not good:
            print('LEVELUP MISMATCH', v, 'predicted', hp, ep, pts, file=sys.stderr)
    return ok


def main():
    player_runs()
    enemy_runs()
    print('damage events', len(out), 'exact', check_damage(), file=sys.stderr)
    lv = levelup_vectors()
    print('levelups', len(lv), 'exact', check_levelup(lv), file=sys.stderr)
    res = {'source': 'tools/battle/gen_buu_vectors.py (headless mGBA, Buu\'s Fury USA)', 'damage': out, 'levelup': lv}
    path = os.path.join(ROOT, 'data/battle/buu_vectors.json')
    json.dump(res, open(path, 'w'), indent=1)
    print('wrote', path)


main()
