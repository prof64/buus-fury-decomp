"""Post-game legs from the REAL post-game state (work/side/pg_start.ss: tools/side/postgame_state.py - the real
route's own frame after the ending's flag 149, flying on the world map; Goku 158, Gohan 141, Vegeta 150).

Each leg starts from pg_start.ss (or a (state, goals) entry: another leg's end) and is driven by the HFIL driver (tools/side/hfil.py SideDriver: 'fly' = the
world-map entry rect, 'goto' = the area maze, charsel = the save point's Switch Character); the leg's end state and
key log go to work/side/pg_<leg>.ss / pg_<leg>.keys.json, then `--record` records it as a master
(work/side/seg/pg_<leg>.json) and cuts it (work/side/pgseg/<leg>/).

    .venv/bin/python tools/side/pgroute.py LEG [LEG ...] [--record] [--cap N]
"""
import json
import os
import subprocess
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parent.parent
sys.path.insert(0, str(HERE))
import hfil  # noqa: E402

A_, U_ = 1, 64
D = ROOT / 'work' / 'side'
PY = str(ROOT / '.venv' / 'bin' / 'python')
# a level gate: stand inside its trigger rect and press A (facing up: walk up against it first)
GATE = [('keys', [(U_, 20, 5), (A_, 3, 60)]), ('events',), ('wait', 30)]
# the save point: Switch Character, `moves` steps along the row (Goku, Gohan, Goten, Trunks, Vegeta, ...)
LEGS = {
    # Southwest Forest 8-2: the tier-2 gate, Goku L100 (s517, flag 163)
    'g82': [('fly', 849, 1785, 31, 23), ('wait', 30), ('goto', '8-2', 104, 58, False), *GATE],
    # Northern Mountains 13-1: the post-game per-visit gate, Goku L135 (s518, flag 187 for the visit), walk through
    'g131': [('fly', 721, 809, 143, 135), ('wait', 30), ('goto', '13-1', 104, 100, False), *GATE,
             ('keys', [(U_, 60, 10)]), ('wait', 30)],
    # East District 2-8's save point: Gohan; 2-33: the hidden dungeon's per-visit gate, Gohan L140 (s518)
    'g233': [('fly', 2632, 1465, 80, 71), ('wait', 30), ('goto', '2-8', 150, 290, False), *hfil.charsel(1),
             ('wait', 20), ('goto', '2-33', 226, 92, False), *GATE, ('keys', [(U_, 60, 10)]), ('wait', 30)],
    # 8-1's save point: Vegeta; 8-7: the per-visit gate, Vegeta L120 (s518), through to 8-8
    # (the way to 8-7 is behind 8-2's tier-2 gate: Goku opens it first)
    'g87': [('fly', 849, 1785, 31, 23), ('wait', 30), ('goto', '8-2', 104, 58, False), *GATE,
            ('goto', '8-1', 145, 165, False), *hfil.charsel(4),
            ('wait', 20), ('goto', '8-7', 448, 72, False), *GATE, ('keys', [(U_, 60, 10)]), ('wait', 30)],
    # 8-1's save point: Gohan; 8-1's take-off pad (A) -> the world map; Diablo Desert 11-1: the tier-2 gate, Gohan
    # L100 (s517, flag 170)
    'g111': [('fly', 849, 1785, 31, 23), ('wait', 30), ('goto', '8-1', 145, 165, False), *hfil.charsel(1),
             ('wait', 20), ('walk', 280, 262), ('keys', [(A_, 3, 240)]), ('fly', 1505, 985, 143, 159), ('wait', 30),
             ('goto', '11-1', 224, 192, False), *GATE, ('keys', [(U_, 60, 10)]), ('wait', 30)],
    # the hidden dungeon past 2-33 (from the g233 leg's end, Gohan in 2-34): the Key in 2-47 (s118, via 2-39), then
    # the locked chest in 2-46 (s117: the Z-Fighter Exhibit, item 71, flag 193)
    # (2-39's house door: the exit rect x 192..208 - walk up from x 200; the planner's edge spot at x 189 is blocked)
    'hidden': ('pg_g233.ss', [('goto', '2-39', 200, 180), ('walk', 201, 180), ('keys', [(U_, 60, 5)]), ('wait', 30),
                              ('goto', '2-47', 104, 120), ('walk', 90, 96), ('face', 'U'), ('keys', [(A_, 3, 60)]), ('events',),
                              ('wait', 30), ('walk', 104, 128),
                              # 2-35's rock (breakable, 3 punches, hit box 249..279 x 128..144) splits the area: the
                              # way to 2-40 .. 2-46 is past it
                              ('goto', '2-35', 230, 128), ('walk', 230, 128), ('face', 'R'),
                              ('keys', [(A_, 2, 14)] * 5), ('wait', 30),
                              ('openwall', '2-35', [(x, y) for x in range(31, 35) for y in range(16, 18)]),
                              ('goto', '2-46', 64, 105), ('keys', [(A_, 3, 60)]), ('events',),
                              ('wait', 30)]),
    # Kami's Lookout (flag 149: 6-1) -> 6-14's pad (A, flag 149) -> the Other World map (cursor on Earth; one LEFT =
    # Heaven) -> 22-1: Dabura (the first map NPC, wandering) gives the Crystal O-Yoroi (s490, flag 152)
    'heaven': [('fly', 633, 1065, 31, 23), ('wait', 60), ('goto', '6-14', 150, 190, False), ('keys', [(A_, 3, 200)]),
               ('keys', [(32, 3, 40), (A_, 3, 200)]), ('wait', 30), ('talknpc', 0), ('wait', 30)],
    # the airship marker (flag 149, not 151: Flying Mega-Fortress XJ-6) -> 12-1; the dungeon itself: tools/side/xj6.py
    'xj6a': [('fly', 1929, 401, 95, 79), ('wait', 200)],
    # the Hercule Cafe 3-20 (s170: "Did you find any of the exhibits?" - Yes hands one in, flag bits 341 count them):
    # from XJ-6's end (the real start's exhibit 65 + XJ-6's 58)
    'cafe': ('pg_xj6.ss', [('fly', 2441, 1065, 127, 135), ('wait', 30), ('goto', '3-20', 150, 240, False), ('face', 'U'),
                           # (the first talk only tells the story, flag 19; each later one takes one exhibit)
                           *[g for _ in range(3) for g in (('keys', [(A_, 3, 60)]), ('events',), ('wait', 30))]]),
    # the Z-Fighter Museum 4-23 (s189, the same hand-in; flag bits 346): from the hidden dungeon's end (exhibit 71) -
    # back out to 2-8's take-off pad (161, 369, 47, 31; A), West City 4-1 -> 4-21 -> 4-23, talk facing up
    # (the rock is back after each reload: punch it again, from the east this time)
    'museum': ('pg_hidden.ss', [('goto', '2-35', 282, 136, False), ('walk', 282, 136), ('face', 'L'),
                                ('keys', [(A_, 2, 14)] * 5), ('wait', 30),
                                ('openwall', '2-35', [(x, y) for x in range(31, 35) for y in range(16, 18)]),
                                ('goto', '2-8', 176, 380, False), ('keys', [(A_, 3, 240)]),
                                ('fly', 1137, 1137, 71, 119), ('wait', 30), ('goto', '4-23', 100, 90, False),
                                ('face', 'U'),
                                *[g for _ in range(2) for g in (('keys', [(A_, 3, 60)]), ('events',), ('wait', 30))]]),
}
# the Hercule City Z-Mart (Orange Star 3-1's door -> 0-91, s028; post-game list: 0 Onigiri, 1 Curry Plate, 2 Steak,
# 3 Turkey, 4 Cherry Soda, 5 Root Beer, 6 Hercule-ade, 7 Elixir): A at the counter facing left, Buy, Items, then
# per row DOWN / UP and one A per item; B out of the list, the menus, and the goodbye
D_, B_ = 128, 2


def zmart(buys):
    k, row = [(32, 20, 10), (A_, 3, 60), (A_, 3, 60), (A_, 3, 120), (0, 1, 60)], 0
    for r, n in buys:
        k += [(D_ if r > row else U_, 3, 15)] * abs(r - row) + [(A_, 3, 40)] * n
        row = r
    return [('goto', '0-91', 100, 123, False), ('walk', 88, 123), ('keys', k + [(B_, 3, 60)] * 4), ('events',),
            ('wait', 30)]


# supplies first (6 Curry Plates for the hidden dungeon; the Lighthouse Keeper's Root Beer, Turkey, Hercule-ade,
# Elixir), then the 2-33 / hidden dungeon / museum chain again from that state
LEGS['shop'] = [('fly', 2441, 1065, 127, 135), ('wait', 30), *zmart([(1, 6), (5, 1), (6, 1), (7, 1), (3, 1)])]
LEGS['g233s'] = ('pg_shop.ss', [('walk', 120, 200), ('keys', [(D_, 60, 60)]), ('goto', '3-1', 30, 150, False), ('keys', [(A_, 3, 240)]), *LEGS['g233']])
LEGS['hiddens'] = ('pg_g233s.ss', list(LEGS['hidden'][1]))
# (the per-visit gate at 2-33 is shut again on the way out: Gohan opens it from inside, A in its rect)
_m = list(LEGS['museum'][1])
_k = next(i for i, g in enumerate(_m) if g[0] == 'goto' and g[1] == '2-8')
LEGS['museums'] = ('pg_hiddens.ss', _m[:_k] + [('goto', '2-33', 226, 60, False), ('keys', [(A_, 3, 60)]), ('events',),
                                             ('wait', 30), ('keys', [(D_, 60, 30)])] + _m[_k:])

# the Lighthouse Keeper's trades (s347, Thieves Den 9-9, the keeper standing at (112, 80)): the first talk asks for a
# Root Beer (flag 362); each later talk with the asked item takes it and gives a Hercule Exhibit, then asks for the
# next (Root Beer 15 -> Turkey 6 -> Hercule-ade 16 -> Elixir 17 -> Dinosaur Tail 9 -> Super Elixir 18; flags
# 363 .. 368). The Hercule City Z-Mart has the first four (the last two: the Z-Marts 0-94 / 0-95).
# (closed loop: open the list by keys, then hfil's `zbuy` - rows of the post-game list, item ids, counts)
ZOPEN = [('goto', '0-91', 100, 123, False), ('walk', 88, 123), ('keys', [(32, 20, 10), (A_, 3, 60), (A_, 3, 60), (A_, 3, 120)])]
# (the list takes one step per press - hold length does not matter - and its cursor byte 0x03003300 / the screen
# show a press only at the next input read: presses 3 frames, 40 frames apart, counted open loop)
def zkeys(buys):
    # (the first press after the list opens, and after a purchase, is dropped: a throwaway SELECT there)
    SEL = (4, 3, 40)
    k, row = [(0, 1, 60), SEL], 0
    for r, n in buys:
        k += [(D_ if r > row else U_, 3, 40)] * abs(r - row) + [(A_, 3, 60)] * n + [SEL]
        row = r
    return ('keys', k + [(B_, 3, 60)] * 4)


LEGS['shopt'] = [('fly', 2441, 1065, 127, 135), ('wait', 30), *ZOPEN,
                 ('zbuy', [(1, 5, 3), (3, 6, 1), (5, 15, 1), (6, 16, 1), (7, 17, 1)]), ('events',), ('wait', 30)]
LEGS['trade'] = ('pg_shopt.ss', [('walk', 120, 200), ('keys', [(D_, 60, 60)]), ('goto', '3-1', 30, 150, False),
                                 ('keys', [(A_, 3, 240)]), ('fly', 3145, 2001, 95, 79), ('wait', 30),
                                 ('goto', '9-9', 112, 96), *[('talkto', 112, 96, 'U')] * 5, ('wait', 30)])

# the last two trades: the Bandana can't be had in the real post-game (the bandits of 0-55 spawn only for a leader of
# level <= 70; s508), so not 9-11's Z-Mart but the Planet of the Kais one (19-5 -> 0-94, s033: Dinosaur Tail, Super
# Elixir): Thieves Den 9-1's take-off pad, Kami's Lookout 6-14's pad, the Other World map (Earth, two LEFT = the
# Planet of the Kais)
LEGS['kais'] = ('pg_trade.ss', [('goto', '9-1', 40, 235), ('keys', [(A_, 3, 240)]),
                                ('fly', 633, 1065, 31, 23), ('wait', 60), ('goto', '6-14', 150, 190, False),
                                ('keys', [(A_, 3, 200)]), ('keys', [(32, 3, 40), (32, 3, 40), (A_, 3, 200)]),
                                ('wait', 30), ('goto', '19-5', 166, 90, False)])

# the Kais Z-Mart (0-94: 19-5's gate - A facing up at (163, 89) - then up through the door): sell one Curry Plate
# (1,000; the list's first row) for the missing zenie, then Buy, Items: rows 0 Steak, 1 Turkey, 2 Three Course Meal,
# 3 Dinosaur Tail, 4 Hercule-ade, 5 Elixir, 6 Super Elixir
LEGS['kaisbuy'] = ('pg_kais.ss', [('keys', [(U_, 10, 5), (A_, 3, 150), (U_, 60, 60)]), ('walk', 100, 123),
                                  ('walk', 88, 123),
                                  ('keys', [(32, 20, 10), (A_, 3, 60), (D_, 12, 40), (A_, 3, 240), (A_, 12, 60),
                                            (B_, 12, 60), (A_, 3, 60), (A_, 3, 240)]),
                                  ('zbuy', [(3, 9, 1), (6, 18, 1)]), ('events',), ('wait', 30)])

# back to the keeper: out of the shop, 19-1's pad (A) -> the Other World map (two RIGHT = Earth) -> Kami's Lookout
# 6-14 -> 6-1's take-off pad (s515, A at (312, 560)) -> Thieves Den 9-9: the last two trades
LEGS['trade2'] = ('pg_kaisbuy.ss', [('walk', 120, 200), ('keys', [(D_, 60, 60)]), ('goto', '19-1', 200, 250, False),
                                    ('keys', [(A_, 3, 200)]), ('keys', [(16, 3, 40), (16, 3, 40), (A_, 3, 200)]),
                                    ('wait', 60), ('walk', 114, 208), ('keys', [(D_, 40, 60)]),
                                    ('goto', '6-1', 312, 560, False), ('keys', [(A_, 3, 240)]),
                                    ('fly', 3145, 2001, 95, 79), ('wait', 30), ('goto', '9-9', 112, 96),
                                    *[('talkto', 112, 96, 'U')] * 2, ('wait', 30)])


# level grinds for the gates (the real route's way: HFIL 18-1 <-> 18-2, a Janemba Hand a room and three Living Dead
# in 18-2, ~21,000 XP each, back on each entry): Kami's Lookout 6-14's pad -> the Other World map (from Earth: DOWN =
# Yemma's Office, LEFT = HFIL) -> 18-1's save point: the character (menu row Goku, Gohan, Goten, Trunks, Vegeta)
def hfil_grind(moves, level):
    return [('fly', 633, 1065, 31, 23), ('wait', 60), ('goto', '6-14', 150, 190, False), ('keys', [(A_, 3, 200)]),
            ('keys', [(D_, 3, 40), (32, 3, 40), (A_, 3, 200)]), ('wait', 30), ('goto', '18-1', 107, 240, False),
            *hfil.charsel(moves), ('wait', 20), ('grind', level, [(18, 1), (18, 2)])]


LEGS['goten120'] = hfil_grind(2, 120)
LEGS['vegeta155'] = hfil_grind(4, 155)
LEGS['trunks110'] = hfil_grind(3, 110)
# back to Earth: 18-1's pad (A) -> the map (from HFIL: RIGHT = Yemma's Office, UP = Earth) -> 6-14 -> its exit ->
# 6-1's take-off pad -> the world map
TO_EARTH = [('goto', '18-1', 190, 262, False), ('keys', [(A_, 3, 200)]), ('keys', [(16, 3, 40), (U_, 3, 40), (A_, 3, 200)]),
            ('wait', 60), ('walk', 114, 208), ('keys', [(D_, 40, 60)]), ('goto', '6-1', 312, 560, False),
            ('keys', [(A_, 3, 240)])]
# Pilaf's Castle 10-1: the tier-2 gate, Goten L120 (s517, flag 167), facing up at (256, 72)
LEGS['g101'] = ('pg_goten120.ss', [*TO_EARTH, ('fly', 2289, 1561, 31, 23), ('wait', 200),
                                   ('goto', '10-1', 256, 72, False), *GATE, ('keys', [(U_, 60, 10)]), ('wait', 30)])


def start_of(leg):
    v = LEGS[leg]
    return (D / v[0], list(v[1])) if isinstance(v, tuple) else (D / 'pg_start.ss', list(v))


def record(leg):
    # (legs driven by another script: xj6 = tools/side/xj6.py from pg_xj6a.ss)
    if leg == 'xj6':
        LEGS[leg] = ('pg_xj6a.ss', [])
    keys = json.loads((D / f'pg_{leg}.keys.json').read_text())
    keys = keys['keys'] if isinstance(keys, dict) else keys
    kf = D / f'k_pg_{leg}.json'
    kf.write_text(json.dumps({'keys': keys}))
    env = dict(os.environ, SEGMENT_DIR=str(D / 'seg'))
    r = subprocess.run([PY, str(ROOT / 'tools/segments/segment.py'), 'record', '--state', str(start_of(leg)[0]),
                        '--keys', str(kf), '--frames', str(len(keys)), '--name', f'pg_{leg}'],
                       env=env, cwd=ROOT, capture_output=True, text=True)
    print(leg, (r.stdout.strip().splitlines() or [r.stderr[-300:]])[-1], flush=True)
    out = D / 'pgseg' / leg
    out.mkdir(parents=True, exist_ok=True)
    for old in out.glob('*'):
        old.unlink()
    subprocess.run([PY, str(ROOT / 'tools/superbuu/mkcuts.py'), str(D / 'seg' / f'pg_{leg}.json'), str(out),
                    '--prefix', f'pg_{leg}', '--min', '100'], env=env, cwd=ROOT, capture_output=True, text=True)
    kf.unlink()


if __name__ == '__main__':
    a = sys.argv[1:]
    cap = int(a[a.index('--cap') + 1]) if '--cap' in a else 30000
    for leg in [x for x in a if x in LEGS]:
        st, goals = start_of(leg)
        hfil.run(str(st), goals, str(D / f'pg_{leg}.ss'), cap, str(D / f'pg_{leg}.keys.json'),
                 True, marks=False)
        if '--record' in a:
            record(leg)
