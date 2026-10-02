"""Batch of forced-state experiments for the Buu group; prints compact summaries.
usage: bv_buu_batch.py name [name...]   (names below)"""
import sys
import json
from common import *
from bv_buu_lib import *
from ents import enemies

TYPE_TAB = 0x08711274


def etype(e, ob):
    return (e.r32(ob + 0x224) - TYPE_TAB) // 28


def summarize(e, o, r, objs=3, series=()):
    print(' runs', state_runs(r)[:16])
    print(' spawns', r['spawns'][:16])
    shown = 0
    for ob, sn in r['objs'].items():
        if shown >= objs:
            break
        shown += 1
        print('  obj', hex(ob), 'vt', hex(sn[0][1][0]), 'first', sn[0][0], 'life', r['life'].get(ob), 'nsnap', len(sn))
        mf = moving_fields(sn)
        print('   moving', json.dumps(mf)[:600])
    for d in r['damage'][:12]:
        print(' dmg', {k: d[k] for k in ('frame', 'target', 'attacker', 'mult', 'atk', 'atk_kind', 'atk_sub', 'raw', 'final')})
    if len(r['damage']) > 12:
        print(' ... total dmg events', len(r['damage']), 'frames', [d['frame'] for d in r['damage']])
    pl = [(f, l, g, c) for f, l, g, c, v in r['player'] if l]
    print(' player hp loss frames', pl[:20])
    hp = [(f, h) for f, x, y, a, c, cf, h in r['boss']]
    ch = [hp[0]] + [b for a, b in zip(hp, hp[1:]) if a[1] != b[1]]
    print(' boss hp changes', ch[:12])
    mv = [(f, round(x, 1), round(y, 1), a, c) for f, x, y, a, c, cf, h in r['boss'][::8]]
    print(' boss pos/anim/ctrl /8f', mv[:30])


def kid_beam():
    e, o = setup(0x3c, dx=90, dy=0)
    force(e, o, 0x125)
    r = trace(e, o, 160)
    summarize(e, o, r)


def kid_split():
    e, o = setup(0x3c, dx=120, dy=0)
    print('kid hp<<10', e.r32(o + 0x28c), 'hp', e.r32(o + 0x28c) >> 10)
    force(e, o, 0x128)
    r = trace(e, o, 120)
    summarize(e, o, r, objs=2)
    for ob in enemies(e):
        print('  enemy', hex(ob), 'type', hex(etype(e, ob)), 'hp', e.r32(ob + 0x28c) / 1024, 'pos', pos(e, ob))
    # now merge: force the original into 0x134 (merge) after letting both live
    e.run(30)
    obs = enemies(e)
    print(' before merge', [(hex(x), hex(etype(e, x)), e.r32(x + 0x28c) / 1024) for x in obs])
    force(e, o, 0x134)
    e.run(60)
    print(' after merge', [(hex(x), hex(etype(e, x)), e.r32(x + 0x28c) / 1024) for x in enemies(e)])


def kid_cooldown():
    """phase-2 timing: after split, how long until the merge state 0x134."""
    e, o = setup(0x3c, dx=200, dy=0)
    force(e, o, 0x128)
    r = trace(e, o, 2000, damage=False, track_spawns=False)
    print(' runs (merge/phase changes)', [x for x in state_runs(r) if x[0] in ('0x128', '0x129', '0x12a', '0x134', '0x11e')][:12])
    print(' enemies', [(hex(x), hex(etype(e, x)), e.r32(x + 0x28c) / 1024) for x in enemies(e)])


def sbuu_transform():
    e, o = setup(0x74, dx=150, dy=0)
    b = [e.r16(o + 0x1b0 + 2 * k) for k in range(4)]
    print(' before: gfx', hex(e.r32(o + 0x58)), 'stats', b, 'res', e.r8(o + 0x1b8), e.r8(o + 0x1b9))
    force(e, o, 0x259)
    r = trace(e, o, 90, damage=False)
    print(' runs', state_runs(r))
    a = [e.r16(o + 0x1b0 + 2 * k) for k in range(4)]
    print(' after: gfx', hex(e.r32(o + 0x58)), 'stats', a, 'res', e.r8(o + 0x1b8), e.r8(o + 0x1b9), 'hp', e.r32(o + 0x28c) >> 10)


def sbuu_react():
    """state 0x255 (incoming player projectile reaction): what does the boss do; does damage still land?"""
    e, o = setup(0x74, dx=90, dy=0)
    force(e, o, 0x255)
    r = trace(e, o, 200, damage=True)
    summarize(e, o, r, objs=1)
    print(' ctrl 0x03001ba4 vt', hex(e.r32(0x03001ba4)))


def sbuu_react_vs_blast():
    """fire a player ki blast (B, Energy Blast selected) at Super Buu 0x74 during the 0x255 pose and in a
    normal state; compare damage events on the boss"""
    for st in (0x255, 0x249):
        e, o = setup(0x74, dx=-60, dy=0)
        p = e.r32(PLAYER)
        e.w8(REC + 0x15, 0)          # select first skill (Energy Blast)
        e.w8(p + 0x10, 3)            # face right (towards the boss)
        force(e, o, st)
        r = trace(e, o, 90, keys=lambda f: KEY.B if 5 <= f < 8 else 0)
        hits = [d for d in r['damage'] if d['target'] == hex(o)]
        hp = [(f, h) for f, x, y, a, c, cf, h in r['boss']]
        print(f' state {st:#x}: boss hits', [(d['frame'], d['atk_sub'], d['mult'], d['final']) for d in hits],
              'hp', hp[0][1], '->', hp[-1][1], 'runs', state_runs(r)[:4], 'spawns', r['spawns'][:4])


def sbuu_summon():
    e, o = setup(0x76, dx=120, dy=0)
    force(e, o, 0x296)
    r = trace(e, o, 700, damage=True, track_spawns=False)
    summarize(e, o, r, objs=0)
    kids = [x for x in enemies(e) if x != o]
    print(' children', [(hex(x), hex(etype(e, x)), e.r32(x + 0x28c) / 1024, e.r32(x + 0x204)) for x in kids])


def near(e, o, dx, dy=0, settle=150):
    """let the boss settle (it may glide to an absolute waypoint), then put the player at boss + (dx, dy)"""
    p = e.r32(PLAYER)
    for _ in range(settle):
        e.w16(REC + 0x16, e.r16(REC + 0x18))
        e.run(1)
    bx, by = pos(e, o)
    setpos(e, p, bx + dx, by + dy)


WAYPTS = 0x0877B1BC


def patch_waypoints(e, cx=425, cy=433):
    """ROM-patch Super Buu 0x77's 5 glide waypoints (24.8 px, absolute) to sit around the SW-forest anchor"""
    pts = [(cx, cy), (cx + 40, cy), (cx - 40, cy), (cx, cy - 30), (cx, cy + 30)]
    for i, (x, y) in enumerate(pts):
        e.w32(WAYPTS + 8 * i, x << 8)
        e.w32(WAYPTS + 8 * i + 4, y << 8)


def sbuu_ball(t=0x77, dx=90, dy=0):
    e, o = setup(t, dx=dx, dy=dy)
    patch_waypoints(e)
    force(e, o, 0x29d)
    near(e, o, dx, dy, settle=40)
    print(' boss at', pos(e, o), 'player', pos(e, e.r32(PLAYER)), 'hp', e.r32(o + 0x28c) >> 10)
    force(e, o, 0x29e if t == 0x77 else 0x2b2)
    r = trace(e, o, 260)
    summarize(e, o, r, objs=3)


def sbuu78_ball():
    """0x78 (fighter Super Buu, exp 0): state 0x2b2 = chase until within 80 px, then 0x803bf0f ball"""
    e, o = setup(0x78, dx=70, dy=0)
    force(e, o, 0x2b2)
    r = trace(e, o, 220)
    summarize(e, o, r, objs=3)


def sbuu_glide():
    e, o = setup(0x77, dx=90, dy=0)
    force(e, o, 0x29d)
    r = trace(e, o, 300, damage=False)
    print(' runs', state_runs(r)[:10])
    mv = [(f, round(x, 1), round(y, 1)) for f, x, y, *_ in r['boss'][::6]]
    print(' pos /6f', mv[:50])
    print(' waypoint idx global 0x030032ec', e.r32(0x030032ec), 'boss+0x214', e.r32(o + 0x214))


def babidi():
    e, o = setup(0x1d, dx=90, dy=0)
    r = trace(e, o, 600, damage=True)
    summarize(e, o, r, objs=1)
    print(' height s16 +0x176', s32v(e.r16(o + 0x176) | (0xffff0000 if e.r16(o + 0x176) & 0x8000 else 0)))


def majin_melee():
    e, o = setup(0x1a, dx=20, dy=0)
    force(e, o, 0xb8)
    r = trace(e, o, 120)
    summarize(e, o, r, objs=1)


def majin_kiblast():
    e, o = setup(0x1a, dx=100, dy=0)
    force(e, o, 0xbd)
    r = trace(e, o, 80)
    summarize(e, o, r, objs=1)


for name in sys.argv[1:]:
    print('########', name)
    globals()[name]()
