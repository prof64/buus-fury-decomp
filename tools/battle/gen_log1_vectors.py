"""Record golden test vectors from the real LoG1 (headless mGBA) for the battle simulator's unit tests.

Writes data/battle/log1_vectors.json with
  damage[]   : apply_damage 0x08007352 calls (RNG state before, dmg, defense, hp, flags -> result, hp after)
               (same procedure as re/log1/verify_damage.py: wolf vs Goku, defenses randomised every 100 frames)
  punch[]    : player punches (secondary-RNG state at hit(), maxHP -> damage) like re/log1/verify_melee.py
  traces[]   : per-frame recordings of scripted fights (initial RAM state + inputs + expected enemy/player state)
               used to replay the simulator frame by frame (AI, pathfinder, timers, attacks, ki moves).

usage: .venv/bin/python tools/battle/gen_log1_vectors.py
"""
import json, os, random, sys
HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(os.path.dirname(HERE))
sys.path.insert(0, os.path.join(ROOT, 're/log1'))
from game import *  # noqa: E402,F401
from model import Rng  # noqa: E402

OUT = os.path.join(ROOT, 'data/battle/log1_vectors.json')
DMG = 0x08007352
HIT = 0x08007416


def damage_vectors():
    e = emu('log1_wolf.ss')
    pc = player_char(e)
    wolf = [en for en in enemies(e) if en['name'] == 'wolf' and abs(en['x'] - 240) < 50][0]['char']
    random.seed(1)
    e.hold(KEY.UP, 20); e.set_keys(0)
    e.breakpoint(DMG)
    out = []
    for it in range(60):
        for c in (pc, wolf):
            e.w32(c + 0x60, random.choice([0, 1, 10, 30, 50, 100, 200, 250, 400]))
            e.w32(c + 0x4c, 3000)
        e.w32(0x03000C1C, 1000)
        for f in range(100):
            st = e.save()
            k = KEY.A if f % 8 < 2 else 0
            e.set_keys(k)
            e.run(1)
            if not [h for h in e.hits() if h['kind'] == 'break']:
                continue
            e.load(st)
            e.set_keys(k)
            start = e.frame
            while e.frame == start:
                e.step()
                if e.reg(15) == DMG + 4 and e.reg(16) & 0x20:
                    dfn, dmg = e.reg(1), e.reg(3)
                    rng = [e.r32(0x0300098C), e.r32(0x03000990)]
                    hp0 = s32v(e.r32(dfn + 0x4c))
                    v = dict(rng=rng, dmg=s32v(dmg), defense=s32v(e.r32(dfn + 0x60)), hp=hp0,
                             team=e.r16(dfn + 0x9a), stun=e.r8(dfn + 0xa), act=e.r8(dfn + 0xb), god=e.r32(0x03000C18))
                    lr = e.reg(14)
                    while True:
                        e.step()
                        if (e.reg(15) - 4) == (lr & ~1):
                            break
                    v['result'] = e.reg(0)
                    v['hpAfter'] = s32v(e.r32(dfn + 0x4c))
                    out.append(v)
            e.hits()
    return out


def punch_vectors():
    e = emu('log1_wolf.ss')
    pc = player_char(e)
    wolf = [en for en in enemies(e) if en['name'] == 'wolf' and abs(en['x'] - 240) < 50][0]['char']
    e.hold(KEY.UP, 20); e.set_keys(0)
    random.seed(2)
    e.breakpoint(HIT)
    out = []
    for it in range(40):
        M = random.choice([105, 200, 600, 1700, 9000, 150000])
        e.w32(pc + 0x50, M)
        e.w32(wolf + 0x4c, 10 ** 7); e.w32(pc + 0x4c, 10 ** 6)
        for f in range(40):
            st = e.save()
            k = KEY.A if f % 8 < 2 else 0
            e.set_keys(k); e.run(1)
            if not any(h['kind'] == 'break' and h['lr'] == 0x08005695 for h in e.hits()):
                continue
            e.load(st); e.set_keys(k)
            start = e.frame
            # state of the secondary RNG before the punch's rand2 call: step to the call site 0x08005678
            while e.frame == start:
                e.step()
                if e.reg(15) == 0x08005678 + 4 and e.reg(16) & 0x20:
                    out.append(dict(rng2=[e.r32(0x03003D24), e.r32(0x03003D28)], maxHp=M))
                if e.reg(15) == HIT + 4 and e.reg(16) & 0x20 and e.reg(14) == 0x08005695 and out and 'dmg' not in out[-1]:
                    out[-1]['dmg'] = e.reg(3) & 0xFFFFFFFF
            e.hits()
    return [v for v in out if 'dmg' in v]


# ------------------------------------------------------------------------------------------------- traces
def ent_state(e, ent):
    if not ent:
        return -1
    return (e.r32(ent + 0x34) - e.r32(ent + 0x18)) // 0x14


def keymask(s):
    m = 0
    for n in s.split('|'):
        if n:
            m |= getattr(KEY, n)
    return m


def set_char_pos(e, c, x, y):
    e.w16(c + 0x20, x); e.w16(c + 0x22, y)
    e.w16(c + 0x24, x << 4); e.w16(c + 0x26, y << 4)
    e.w32(c + 0x78, x >> 3); e.w32(c + 0x7c, y >> 3)
    ent = e.r32(c + 0x274)
    if ent:
        e.w16(ent + 0x20, x); e.w16(ent + 0x22, y)


def enemy_init(e, c):
    ent = e.r32(c + 0x274)
    pl = s32v(e.r32(c + 0xac))
    return dict(type=e.r16(c), x=s16(e.r16(c + 0x20)), y=s16(e.r16(c + 0x22)), sx=s16(e.r16(c + 0x24)), sy=s16(e.r16(c + 0x26)),
                state=e.r8(c + 9), face=e.r8(c + 8), stun=e.r8(c + 0xa), act=e.r8(c + 0xb), inv=e.r16(c + 0x14), aggro=e.r16(c + 0x1e),
                stuck=e.r16(c + 0x1c), hp=s32v(e.r32(c + 0x4c)), ki=s32v(e.r32(c + 0x54)), tx=s32v(e.r32(c + 0x78)), ty=s32v(e.r32(c + 0x7c)),
                lastGoal=[s16(e.r16(c + 0x80)), s16(e.r16(c + 0x82))], pathIdx=s32v(e.r32(c + 0xa8)), pathLen=pl,
                path=[[s16(e.r16(c + 0xb0 + 4 * i)), s16(e.r16(c + 0xb2 + 4 * i))] for i in range(max(0, min(99, pl)))],
                spriteDef=e.r32(ent + 0x1c) if ent else 0, entState=ent_state(e, ent), entTick=e.r8(ent + 0x10) if ent else 0, entT24=s32v(e.r32(ent + 0x24)) if ent else 0, entFace=e.r8(ent + 0x13) if ent else 0, entX=s16(e.r16(ent + 0x20)), entY=s16(e.r16(ent + 0x22)))


def trace(name, state, frames, place, keys, player_char_pokes=None, enemy_pokes=None, player=None, pokes=None):
    e = emu(state)
    for a, sz, v in (pokes or []):
        {1: e.w8, 2: e.w16, 4: e.w32}[sz](a, v)
    pc = player_char(e)
    pe = e.r32(PLAYER_CTL + 0xC)
    for k, v in (player_char_pokes or {}).items():
        e.w32(pc + int(k, 0), v)
    if player:
        set_player_xy(e, *player)
    keep = []
    used = set()
    for en in enemies(e):
        want = None
        for i, p in enumerate(place):
            if (p[0] == en['name'] or p[0] == en['type']) and i not in used:
                want = p
                used.add(i)
                break
        if want:
            set_char_pos(e, en['char'], want[1], want[2])
            for k, v in (enemy_pokes or {}).items():
                e.w32(en['char'] + int(k, 0), v)
            keep.append(en['char'])
        else:
            set_char_pos(e, en['char'], 4000, 4000)
    ctl = PLAYER_CTL
    mp = e.r32(0x03000600)
    mw, mh = e.r16(mp + 4), e.r16(mp + 6)
    import base64
    collision = [base64.b64encode(e.read(0x02005040 + l * 0x1D4C, (mw * mh + 7) // 8)).decode() for l in range(3)]
    init = dict(map=e.r8(0x0300004C), collision=collision, fc=e.r32(0x03003B6C), rng=[e.r32(0x0300098C), e.r32(0x03000990)], rng2=[e.r32(0x03003D24), e.r32(0x03003D28)],
                pathTimer=e.r32(0x03003B30), solar=e.r32(0x03003B34), cam=[s16(e.r16(0x03000608 + 2 * i)) for i in range(4)],
                cap=s32v(e.r32(0x03000C1C)), seen=[s16(e.r16(pc + 0x20)), s16(e.r16(pc + 0x22))],
                player=dict(sx=s16(e.r16(ctl + 0x14)), sy=s16(e.r16(ctl + 0x16)), x=s16(e.r16(pe + 0x20)), y=s16(e.r16(pe + 0x22)),
                            hp=s32v(e.r32(pc + 0x4c)), maxHp=s32v(e.r32(pc + 0x50)), ki=s32v(e.r32(pc + 0x54)), maxKi=s32v(e.r32(pc + 0x58)),
                            regen=s32v(e.r32(pc + 0x5c)), defense=s32v(e.r32(pc + 0x60)), exp=s32v(e.r32(pc + 0x84)), face=e.r8(pe + 0x13),
                            st=ent_state(e, pe), tick=e.r8(pe + 0x10), t24=s32v(e.r32(pe + 0x24)), flight=e.r8(ctl + 1), maxFlight=e.r8(ctl), selected=e.r8(ctl + 5), learned=e.r8(0x0300004D)),
                enemies=[enemy_init(e, c) for c in keep])
    km = [(a, b, keymask(s)) for a, b, s in keys]
    rows = []
    hits = []
    e.breakpoint(HIT)
    for f in range(1, frames + 1):
        k = 0
        for a, b, m in km:
            if a <= f <= b:
                k |= m
        e.set_keys(k)
        e.run(1)
        for h in e.hits():
            if h['kind'] == 'break':
                hits.append([f, 0 if h['regs'][0] == pc else 1 + keep.index(h['regs'][0]) if h['regs'][0] in keep else -1,
                             0 if h['regs'][1] == pc else 1 + keep.index(h['regs'][1]) if h['regs'][1] in keep else -1, h['regs'][2], s32v(h['regs'][3] & 0xFFFFFFFF)])
        row = [s16(e.r16(ctl + 0x14)), s16(e.r16(ctl + 0x16)), s32v(e.r32(pc + 0x4c)), s32v(e.r32(pc + 0x54)), ent_state(e, pe), e.r8(ctl + 1)]
        for c in keep:
            row += [s16(e.r16(c + 0x20)), s16(e.r16(c + 0x22)), e.r8(c + 9), e.r8(c + 0xa), e.r8(c + 0xb), s32v(e.r32(c + 0x4c)), s32v(e.r32(c + 0x54)), e.r16(c + 0x9c)]
        rows.append(row)
    return dict(name=name, state=state, init=init, keys=keys, frames=frames,
                columns=['px16', 'py16', 'hp', 'ki', 'st', 'flight'] + ['x', 'y', 'cst', 'stun', 'act', 'hp', 'ki', 'alive'] * len(keep),
                rows=rows, hits=hits)


TRACES = [
    # wolf stays put while Goku stands in the same tile, chases when he walks, melee + 45-frame cooldown
    dict(name='wolf_chase', state='log1_wolf.ss', frames=1500, place=[['wolf', 240, 112]],
         keys=[[400, 440, 'LEFT'], [600, 640, 'DOWN'], [800, 801, 'A'], [820, 821, 'A'], [840, 841, 'A'], [860, 861, 'A'], [880, 881, 'A'],
               [900, 901, 'A'], [920, 921, 'A'], [940, 941, 'A'], [960, 961, 'A'], [980, 981, 'A'], [1000, 1001, 'A'], [1100, 1180, 'RIGHT']]),
    # Frieza soldier (70000 HP): re-walks its old path after each melee, negative-damage capable
    dict(name='henchman', state='log1_hench.ss', frames=1600, place=[[23, 340, 236]], keys=[], player_char_pokes={'0x4c': 30000}),
    # player moves: tap / charged ki blast, punches, walking with diagonal input
    dict(name='player_moves', state='log1_wolf.ss', frames=260, place=[], player_char_pokes={'0x50': 600, '0x4c': 600, '0x58': 400, '0x54': 400},
         keys=[[10, 10, 'B'], [60, 99, 'B'], [150, 150, 'A'], [160, 160, 'A'], [190, 190, 'A'], [200, 215, 'LEFT'], [216, 230, 'UP|LEFT']]),
    # Raditz fight start (scripted conversion already done), Goku fights back with punches and ki blasts
    dict(name='raditz', state='log1_raditz_fight.ss', frames=1200, place=[['raditz', 448, 173]], player_char_pokes={'0x4c': 50000},
         keys=[[1, 30, 'DOWN'], [60, 61, 'A'], [130, 131, 'B'], [200, 201, 'A'], [260, 262, 'A'], [330, 331, 'B'], [400, 440, 'LEFT'], [500, 501, 'A'],
               [560, 561, 'A'], [620, 650, 'RIGHT'], [700, 701, 'B'], [760, 761, 'A'], [900, 960, 'UP']]),
    # flight: take off, fly around (energy drain), land
    dict(name='flight', state='log1_wolf.ss', frames=200, place=[], keys=[[5, 5, 'R'], [10, 40, 'LEFT'], [41, 60, 'UP'], [80, 80, 'R']]),
    # Kamehameha (40-frame charge) into a wolf, Solar Flare freeze, then ki blasts (all moves learned)
    dict(name='kame_solar', state='log1_wolf.ss', frames=700, place=[['wolf', 240, 96]],
         player_char_pokes={'0x50': 600, '0x4c': 600, '0x58': 400, '0x54': 400, '0x5c': 6},
         pokes=[[0x0300004D, 1, 7], [0x030007AC + 5, 1, 1]],
         keys=[[2, 2, 'UP'], [5, 45, 'B'], [150, 150, 'L'], [160, 200, 'B'], [330, 330, 'L'], [340, 340, 'B'], [380, 380, 'B'], [420, 420, 'B'],
               [460, 470, 'B'], [520, 521, 'A'], [560, 600, 'RIGHT']]),
    # Frieza soldier (henchman_3, 12000 Ki, wanders) chasing Goku along a row: ki blasts when aligned
    dict(name='hench_ki', state='log1_hench.ss', frames=3000, place=[[22, 340, 236]], player_char_pokes={'0x4c': 30000},
         keys=[[20, 200, 'LEFT'], [260, 560, 'RIGHT'], [600, 800, 'LEFT'], [850, 1100, 'RIGHT'], [1150, 1400, 'LEFT'], [1450, 1700, 'RIGHT'],
               [1750, 2000, 'LEFT'], [2050, 2300, 'RIGHT'], [2350, 2600, 'LEFT'], [2650, 2900, 'RIGHT']]),
    # Goku runs back and forth along an open row, the soldier chases: ki blasts that hit (damage = owner maxHP/3)
    dict(name='hench_blast', state='log1_hench.ss', frames=2400, player=[560, 368], place=[[22, 620, 368]], player_char_pokes={'0x4c': 30000},
         keys=[[10, 290, 'LEFT'], [330, 600, 'RIGHT'], [640, 900, 'LEFT'], [940, 1200, 'RIGHT'], [1240, 1500, 'LEFT'], [1540, 1800, 'RIGHT'],
               [1840, 2100, 'LEFT'], [2140, 2390, 'RIGHT']]),
    # three wolves (pack): separation quirk, simultaneous attacks; Goku punches back
    dict(name='wolf_pack', state='log1_maps/log1_map04.ss', frames=1500, player=[560, 456], player_char_pokes={'0x4c': 50000},
         place=[['wolf', 496, 416], ['wolf', 536, 432], ['wolf', 520, 448]],
         keys=[[100, 101, 'A'], [130, 131, 'A'], [160, 161, 'A'], [200, 230, 'LEFT'], [260, 261, 'A'], [300, 301, 'B'], [340, 341, 'A'],
               [400, 440, 'UP'], [480, 481, 'A'], [520, 521, 'A'], [600, 640, 'RIGHT'], [700, 701, 'A'], [760, 761, 'B'], [900, 940, 'DOWN'], [1000, 1001, 'A']]),
    # squirrels (target type 4 = always flee, move type 4 = wander), Goku walks among them
    dict(name='squirrels', state='log1_maps/log1_map09.ss', frames=1500, player=[312, 400],
         place=[['squirrel', 300, 360], ['squirrel', 340, 380], ['squirrel', 280, 380]],
         keys=[[50, 90, 'UP'], [150, 190, 'LEFT'], [300, 340, 'RIGHT'], [500, 560, 'UP'], [700, 760, 'DOWN'], [900, 960, 'LEFT'], [1100, 1150, 'RIGHT']]),
    # pterodactyl (flying layer, wanders, aggressive)
    dict(name='ptero', state='log1_maps/log1_map04.ss', frames=1200, player=[560, 456], player_char_pokes={'0x4c': 50000},
         place=[['pterodactyl_1', 560, 380]],
         keys=[[200, 240, 'LEFT'], [400, 401, 'A'], [500, 540, 'RIGHT'], [700, 701, 'B'], [800, 840, 'UP']]),
]


def main():
    out = dict(source='tools/battle/gen_log1_vectors.py (headless mGBA, LoG1 USA)')
    out['damage'] = damage_vectors()
    print('damage', len(out['damage']))
    out['punch'] = punch_vectors()
    print('punch', len(out['punch']))
    out['traces'] = []
    for t in TRACES:
        tr = trace(**t)
        out['traces'].append(tr)
        print('trace', tr['name'], len(tr['rows']), 'hits', len(tr['hits']))
    json.dump(out, open(OUT, 'w'), separators=(',', ':'))
    print('wrote', OUT)


if __name__ == '__main__':
    main()
