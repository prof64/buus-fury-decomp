"""Run 2's fat-Buu (and Janemba) chapters, played as pieces from run 2's ship end (8-70 after Majin Buu), each a goal
list for the ship / dungeon agents' drivers (unchanged) built from the state the piece starts in. Owner: dungeon
agent B (engine/docs/segments.md "Run 2"). Input only - no RAM writes. The pieces' key logs, in PIECES' order of
play (radar, bandana, grind_goten, airship, fukurou, pyramid, desert, pilaf, thieves, nataday, dbx), make the chain's
part `work/run2/parts/f_fat.json`.

`python3 tools/run2/fat2.py NAME START.ss START.eeprom [--cap N]` plays one piece from a state and its cartridge save
(the .ss does not carry the EEPROM; deaths continue from saves this part makes): work/run2fat/<NAME>.{ss,eeprom,
keys.json}, a screenshot on failure. `driver(name, e)` -> the piece's driver (done() / keys(e) / k / goals)."""
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
# the live ship driver and its nav (ship_route2 puts run 1's frozen copies, re/buu/ship/r1frozen, first on the path
# for the ship chapter's replay; not here)
sys.path[:] = [p for p in sys.path if not p.endswith('r1frozen')]
for m in ('nav', 'ship_driver'):
    if m in sys.modules and 'r1frozen' in (getattr(sys.modules[m], '__file__', '') or ''):
        del sys.modules[m]
for d in ('tools/superbuu', 'tools/story_playthrough', 'tools/field', 're/buu', 're/buu/ship', 'tools/run2'):
    if str(ROOT / d) not in sys.path:
        sys.path.insert(0, str(ROOT / d))
from ship_driver import ShipDriver, alloc_keys, KEY, status  # noqa: E402
from ship_route import nav, charsel, rest_at  # noqa: E402

U, D, L, R, A, B = KEY.UP, KEY.DOWN, KEY.LEFT, KEY.RIGHT, KEY.A, KEY.B
S_ = KEY.START

# world map places (the flyer's landing point in run 1's flights, re/buu/ship + work/ship/fat logs)
WEST_CITY = ('fly', 1137, 1137, 71, 119)
LOOKOUT = ('fly', 633, 1065, 31, 23)
SW_FOREST = ('fly', 849, 1785, 31, 23)
THIEVES = ('fly', 3145, 2001, 95, 79)
PILAF = ('fly', 2289, 1561, 31, 23)
DESERT = ('fly', 1505, 985, 143, 159)
AIRSHIP = ('fly', 1966 - 15, 478 - 11, 31, 23)
FUKUROU = ('fly', 721, 809, 143, 135)
PYRAMID = ('fly', 2185 - 15, 1210 - 11, 31, 23)


def save_spot(w, a):
    """where to stand (facing up) at area w-a's save point (its rect from the content, as nav reads it)"""
    import json as _j
    import nav as navm
    ar = navm.areas()[f'{w}-{a}']
    o = _j.loads((navm.CONTENT / 'maps' / ar['maps'][0] / 'objects.json').read_text())
    for x in o.get('areas', []):
        if x['key'] == f'{w}-{a}':
            for t in x['objects']:
                if t['type'] == 'save_point':
                    r = t['rect']
                    return r['x'] + r['w'] // 2 - 8, r['y'] + r['h'] - 6
    raise KeyError(f'no save point in {w}-{a}')


def save_at(w, a):
    """walk to w-a's save point, Save -> GAME 1"""
    x, y = save_spot(w, a)
    return [('unstick',), ('walk', x, y), ('face', 'U'), SAVE, ('wait', 30)]


def retry_from(w, a, parts):
    """`parts` after a save at w-a: a game over continues from it and plays them again"""
    return lambda e: Retry(parts, [DEATH_CONTINUE, ('until', w, a), ('wait', 30)])


def take_off(x, y):
    """walk to a pad / the area's take-off spot, A: the world map"""
    return [('unstick',), ('walk', x, y), ('face', 'U'), ('press', A), ('wait', 60)]


def zmart(row, n):
    """West City 4-1 -> the Z-Mart (s030): Buy -> Items -> row `row` (0 Rice Ball, 1 Chicken Leg, 2 Hamburger, 3
    Onigiri ...) `n` times, out, back to 4-1"""
    return [('unstick',), ('walk', 264, 310), ('hold', U, 40), ('wait', 30),
            ('unstick',), ('walk', 102, 124), ('hold', L, 10),
            ('keys', [(A, 3, 60), (A, 3, 60), (A, 3, 100)] + [(D, 3, 15)] * row + [(A, 3, 60)] * n +
             [(B, 3, 80), (B, 3, 60), (D, 3, 15), (D, 3, 15), (A, 3, 60), (A, 3, 60)]), ('events',), ('wait', 30),
            ('unstick',), ('walk', 112, 212), ('hold', D, 40), ('wait', 30)]


def g_radar(e):
    """8-70 -> Kami's Lookout (s214: Goten) -> West City: Bulma's Dragon Radar (s193, flag 77) on the first visit"""
    return [
        ('walk', 272, 280), ('face', 'U'), ('press', A), ('wait', 60),
        LOOKOUT, ('wait', 30),
        ('walk', 310, 560), ('story', 6, 1, True),                             # s214: now Goten
        ('walk', 310, 560), ('face', 'D'), ('press', A), ('wait', 60),
        WEST_CITY, ('wait', 30),
        nav(4, 27, False), ('unstick',), ('talkto', 150, 172, 'R'), ('wait', 30), ('needflag', 77),
    ]


def g_bandana(e):
    """West City: Trunks at the save point (run 1 the same, he is the one the Stolen Airship's bandits drop the
    Bandana for: level <= 70), Z-Mart (s030): Chicken Legs, the second food row (run 1: five Hamburgers), then the world
    map encounters until the Bandana (equipment 126) drops. (The stored piece = these goals' keys to the take-off spot
    + re/buu/ship/hunt_search.py's 0-punch run from there: its first try that got the Bandana before Trunks passed 70;
    this goal list's own hunt let him die.)"""
    return [
        nav(4, 1, False), ('unstick',), ('walk', 148, 594), *charsel(1),
        ('unstick',), ('walk', 264, 310), ('hold', U, 40), ('wait', 30),
        ('unstick',), ('walk', 102, 124), ('hold', L, 10),
        ('keys', [(A, 3, 60), (A, 3, 60), (A, 3, 100), (D, 3, 15)] + [(A, 3, 40)] * 6 +
         [(B, 3, 80), (B, 3, 60), (D, 3, 15), (D, 3, 15), (A, 3, 60), (A, 3, 60)]), ('events',), ('wait', 30),
        ('unstick',), ('walk', 112, 212), ('hold', D, 40), ('wait', 30),
        *take_off(416, 360),
        ('hunt', 126),
    ]


def g_grind_goten(e):
    """Southwest Forest 8-1's save point: Goten; his levels to 100 in 8-6 / 8-3 (run 1: Trunks to 80 first, then Goten
    to 95, 8-3 first) - the Airship first, which has no gate"""
    return [
        ('wait', 60), SW_FOREST, ('wait', 30),
        nav(8, 1, False), ('unstick',), ('walk', 148, 186), *charsel(-1),
        ('grind', 100, [(8, 6), (8, 3)]),
        nav(8, 1, False), ('unstick',), *take_off(280, 266),
    ]


def g_airship(e):
    """the Airship XJ-5 (12) first: the flight, then agent B's db5 route (the decks, the lift, the Warlord, the 5-Star
    Dragon Ball s387, out on the self-destruct countdown, the pad)"""
    return Chain([ship([('wait', 30), WEST_CITY, ('wait', 30), *zmart(3, 8), *take_off(416, 360),
                        AIRSHIP, ('wait', 200)]),
                  db57('db5', fix_db5, to='a01_12-27'),
                  # run 2: Save at 12-27's save point (run 1 never saved in the fat chapter); a game over from here
                  # (the Warlord, the escape on the countdown) continues from it
                  ship([*rest_at(180, 336), ('unstick',), ('walk', 180, 336), ('face', 'U'), SAVE, ('wait', 30)]),
                  lambda e: Retry([db57('db5', fix_db5, frm='a01_12-27')],
                                  [DEATH_CONTINUE, ('until', 12, 27), ('wait', 30)])])


g_airship.chain = True

def g_fukurou(e):
    """Fukurou Forest / the Ninja Stronghold (13) second (run 1: fifth): the flight, a Save at 13-1, agent B's db6
    route (the gate wants Goten L80; the Ninja Key, the boss, the 6-Star Dragon Ball s399)"""
    return Chain([ship([('wait', 30), FUKUROU, ('wait', 200), *save_at(13, 1)]),
                  retry_from(13, 1, [db57('db6')])])


g_fukurou.chain = True

def g_pyramid(e):
    """Kyodai Pyramid (14) third (run 1: sixth): Trunks to 85 at Southwest Forest first (the gate wants him at 80;
    run 1 took him to 80 before the Thieves Den), the flight, agent B's db7 route to Dr. Challenger with Trunks, then
    Goten from 14-1's save point (a Save there) for the rest (the four men, the switches, the pit maze, the boss,
    the Headdress, the 7-Star Dragon Ball s406)"""
    sx, sy = save_spot(14, 1)
    return Chain([
        ship([('wait', 30), SW_FOREST, ('wait', 30),
              nav(8, 1, False), ('unstick',), ('walk', 148, 186), *charsel(1),
              ('grind', 85, [(8, 3), (8, 6)]),
              nav(8, 1, False), *take_off(280, 266),
              PYRAMID, ('wait', 200)]),
        db57('db7', to='p01_challenger', real=True),
        ship([nav(14, 1, False), ('unstick',), ('walk', sx, sy), *charsel(-1), *save_at(14, 1)]),
        retry_from(14, 1, [db57('db7', fix_db7, frm='p01_challenger', real=True)]),
    ])


g_pyramid.chain = True

def dungeons_route(mod, fn, drvmod, drvcls):
    """an agent A route (tools/dungeons: db2 Diablo Desert, db4 Nataday) as a chain part, built from its state"""
    def make(e):
        if str(ROOT / 'tools' / 'dungeons') not in sys.path:
            sys.path.insert(0, str(ROOT / 'tools' / 'dungeons'))
        import importlib
        m = importlib.import_module(mod)
        drv = getattr(importlib.import_module(drvmod), drvcls)
        return drv(getattr(m, fn)(e))
    return make


def g_desert(e):
    """Diablo Desert + Express (11) fourth (run 1: third): Goten's 27 points a third each into STR, END and SPD (run
    1: never SPD), the flight, agent A's db2 route (the gate, the four switches, the train, the five
    robbers, the 2-Star Dragon Ball s378)"""
    pts = e.r16(0x03001560 + 0x68 * 2 + 0x2C)
    return Chain([ship([alloc_keys([(0, [pts // 3, 0, pts // 3, pts - 2 * (pts // 3)])]), ('wait', 30),
                        DESERT]),
                  dungeons_route('db2_route', 'db2', 'db2_driver', 'Db2Driver')])


g_desert.chain = True

def g_pilaf(e):
    """Pilaf's Castle (10) fifth (run 1: second): four Onigiri at West City's Z-Mart, the flight, run 1's goals from 10-1 to the pad (Trunks breaks the
    rock: s517 (3, 80); Goten through the castle, Pilaf's Guardian s365, the 3-Star Dragon Ball s366), but Goten's
    points into POW / SPD (run 1: STR / END), a rest + Save at 10-30 before the Guardian"""
    import ship_route
    g = ship_route.goals_fatbuu(e)
    g = g[g.index(('mark', 'd11_pilaf')):g.index(('mark', 'd15_map'))]
    pts = e.r16(0x03001560 + 0x68 * 2 + 0x2C)
    out = []
    for x in g:
        if x[0] == 'keys' and x is g[g.index(('mark', 'd11b_rock')) - 1]:
            x = alloc_keys([(0, [0, pts // 2, 0, pts - pts // 2])])
        out.append(x)
    k = out.index(('mark', 'd12_pilaf_boss'))
    # run 2: at 10-30's save point Goten rests (Trunks stands in) and the game is saved; a game over at the
    # Guardian continues from there
    return Chain([ship([('wait', 30), WEST_CITY, ('wait', 30), *zmart(3, 4), *take_off(416, 360),
                        PILAF, ('wait', 200)] + out[:k + 1]),
                  ship([*rest_at(60, 109), *save_at(10, 30)]),
                  retry_from(10, 30, [ship(out[k + 1:])])])


g_pilaf.chain = True


def g_thieves(e):
    """the Thieves Den (9) sixth (run 1: first): the flight; Trunks breaks 9-1's rock (s517 (3, 80)), Goten with the
    Bandana on (run 2 tried Trunks as the leader: the 9-5 Juggernaut killed him six times), a Save at 9-1: the guards
    (s350), the hideout, the Bandit King (s357), the 1-Star Dragon Ball (s362) and the gang, out to the pad
    (ship_route.goals_fatbuu's goals)"""
    import ship_route
    g = ship_route.goals_fatbuu(e)
    g = g[g.index(('mark', 'd06_thieves')):g.index(('mark', 'd10_map'))]
    k = g.index(('nav', 9, 10, True))
    # a Save at 9-1's save point with the Bandana on; a game over in the den continues from there
    return Chain([ship([('wait', 30), THIEVES] + g[:k]), ship(save_at(9, 1)), retry_from(9, 1, [ship(g[k:])])])


g_thieves.chain = True


def g_nataday(e):
    """Nataday Village / the Volcano (15) last, as it must be (it appears with the other six balls; the seventh ball's
    pad flight took us there: s019): a Save at 15-1, agent A's db4 route (the priest, the elder, the gate, the
    Wilderness, the Volcano, Broly s435 / s436, back down, the 4-Star Dragon Ball s439, the pad; the way back without the flame / block
    guards, as run 1), Goten's new points into END / POW after Broly (run 1: STR / END)"""
    def make(e):
        if str(ROOT / 'tools' / 'dungeons') not in sys.path:
            sys.path.insert(0, str(ROOT / 'tools' / 'dungeons'))
        import db4_route
        from db4_driver import Db4Driver
        g = db4_route.db4(e)
        k = g.index(('mark', 'j_down'))
        n = e.r16(0x03001560 + 0x68 * 2 + 0x2C)

        class Alloc(Db4Driver):
            # the way back from j_down without the flame / block guards (run 1's db4b: the two Destroyers in the open
            # in 15-17 kill a Goten who stands behind a guard; running through costs less)
            back = False

            def flame_guard(self, ee, s):
                return None if self.back else super().flame_guard(ee, s)

            def block_guard(self, ee, s):
                return None if self.back else super().block_guard(ee, s)

            def in_flame(self, ee, px, py, margin=44):
                return False if self.back else super().in_flame(ee, px, py, margin)

            def step(self, ee):
                gg = self.goals[self.k]
                if gg == ('alloc_now',):
                    self.back = True
                    pts = ee.r16(0x03001560 + 0x68 * ee.r8(0x03001068) + 0x2C)
                    self.goals[self.k] = alloc_keys([(0, [0, pts // 2, pts - pts // 2, 0])])
                    return 0
                return super().step(ee)
        del n
        return Alloc(g[:k + 1] + [('wait', 10), ('alloc_now',), ('wait', 30)] + g[k + 1:])
    return Chain([ship([('until', 15, 1), ('wait', 30), *save_at(15, 1)]), retry_from(15, 1, [make])])


g_nataday.chain = True

def g_dbx(e):
    """the seventh ball's aftermath: West City's Z-Mart (five Hamburgers; run 1: six Onigiri), the flight to the
    airship marker (s017: Shenron, the fusion training, Goku on the Lookout), Goku's 42 points a third STR / END and
    the rest POW / SPD (run 1: STR / END only), agent B's dbx route (s026, Goku vs Majin Buu s027, s218 Gotenks,
    s020 Gotenks vs Buu) to the Janemba saga's prologue (0-96)"""
    def goku_points(e):
        n = e.r16(0x03001560 + 0x2C)
        return Driver2([('wait', 20), alloc_keys([(0, [n // 3, n // 6, n // 3, n - 2 * (n // 3) - n // 6])]), ('wait', 30)])
    return Chain([ship([('wait', 30), WEST_CITY, ('wait', 30), *zmart(2, 5), *take_off(416, 360),
                        ('fly', 736 - 15, 1072 - 11, 31, 23, 'near')]),
                  db57('dbx', frm='x01_map', to='x03_goku'),
                  goku_points,
                  db57('dbx', frm='x03_goku', emergency=0.45)])


g_dbx.chain = True

def g_j_intro(e):
    """the Janemba saga's prologue (0-96 -> 0-16 -> s022 ...) played through to Snakeway 1-1, then 1-5 (s066:
    King Yemma, flag 121), its Instant Transmission pad -> the Other World map (LEFT: HFIL) -> 18-1"""
    return [('until', 0, 16), ('hold', U, 400), ('until', 1, 1), ('wait', 30),      # the ogre walks up into s022
            nav(1, 5, False), ('events',), ('wait', 30),
            ('unstick',), ('walk', 344, 214), ('talk', 'U'), ('wait', 190),
            ('hold', L, 40), ('wait', 10), ('hold', A, 3), ('until', 18, 1), ('wait', 30)]


def g_j_grind(e):
    """Goku against HFIL's level 116 .. 125 rooms: to 92 on the Janemba Hands of 18-12 / 18-1 (run 1: 90), then to
    116 on 18-2's Living Dead (run 1: 120)"""
    return [('grind', 92, [(18, 12), (18, 1)]), ('grind', 116, [(18, 2), (18, 1)]), nav(18, 1, False), ('wait', 30)]


class Unjam:
    """around a driver: standing still in one spot (field, no event) for 900 frames - the driver pushing into a wall
    beside an exit (HFIL 18-8's east strip: the exit to 18-9 takes the player only higher up the bank) - holds the
    diagonals in turn (UP/DOWN with RIGHT/LEFT, 60 frames each), then hands back"""
    PROG = [U | R, D | R, U | L, D | L]

    def __init__(self, inner):
        self.inner, self.pos, self.n, self.prog, self.tries = inner, None, 0, [], 0

    k = property(lambda self: self.inner.k)
    goals = property(lambda self: self.inner.goals)

    def done(self):
        return self.inner.done()

    def keys(self, e):
        if self.prog:
            return self.prog.pop(0)
        s = status(e)
        # (within 8 px of where it stood still: the driver's own turning and nudging moves a pixel or two)
        near = (self.pos is not None and s['p'] is not None and s['scene'] == 0 and self.pos[0] == tuple(s['area'])
                and abs(self.pos[1][0] - s['p'][0]) <= 8 and abs(self.pos[1][1] - s['p'][1]) <= 8)
        if near:
            self.n += 1
        else:
            self.n = 0
            self.pos = (tuple(s['area']), tuple(s['p'][:2])) if s['p'] else None
        if self.n > 900:
            self.n = 0
            self.tries += 1
            if self.tries > 20:
                raise RuntimeError('unjam: still stuck')
            m = self.PROG[(self.tries - 1) % 4]
            print('  unjam (diagonal)', s, flush=True)
            self.prog = [m] * 60 + [0] * 4
            return self.prog.pop(0)
        return self.inner.keys(e)


def hfil_part(frm, to, extra_before=()):
    """the side agent's HFIL route (tools/side/hfil.py ROUTE) from mark `frm` to mark `to`, by re/buu/ship/jan_side's
    RealSide driver (run 1's), `extra_before` goals first"""
    def make(e):
        for d in ('tools/side', 're/buu/ship'):
            if str(ROOT / d) not in sys.path:
                sys.path.insert(0, str(ROOT / d))
        import hfil
        import jan_side  # noqa: F401  (hfil.SideDriver = RealSide)
        import maze
        R = hfil.ROUTE
        i, j = R.index(('mark', frm)), R.index(('mark', to))
        for g in R[:i]:
            if g[0] == 'openwall':
                maze.open_wall(g[1], g[2])
        return Unjam(hfil.SideDriver(list(extra_before) + [g for g in R[i + 1:j + 1] if g[0] != 'mark']))
    return make


def g_j_hfil(e):
    """HFIL 18-2's switch (Instant Transmission), the corridor 18-3 .. 18-10's entry - the side agent's route as
    run 1, from Goku 116"""
    return Chain([hfil_part('hfil', 'maze')])


g_j_hfil.chain = True
MAZE_PUNCHES = 0


def g_j_maze(e):
    """18-10's gate maze (Instant Transmission toggles; its four Poltergeists drain Goku: run 1 punched the air 8 times
    first, a luck search), the boulder, 18-11's four Vegetas (s446, flag 123). MAZE_PUNCHES = run 2's count"""
    return Chain([hfil_part('maze', 'vegeta', [('keys', [(A, 2, 34)] * MAZE_PUNCHES + [(0, 1, 1)])])])


g_j_maze.chain = True

def side_goals(make_goals, frm):
    """a SideDriver (run 1's RealSide) on goals `make_goals(hfil_route)`, with the HFIL walls opened before mark
    `frm` registered (the maze planner's state)"""
    def make(e):
        for d in ('tools/side', 're/buu/ship'):
            if str(ROOT / d) not in sys.path:
                sys.path.insert(0, str(ROOT / d))
        import hfil
        import jan_side  # noqa: F401
        import maze
        R = hfil.ROUTE
        for g in R[:R.index(('mark', frm))]:
            if g[0] == 'openwall':
                maze.open_wall(g[1], g[2])
        return Unjam(hfil.SideDriver(make_goals(R)))
    return make


def g_j_back(e):
    """back from 18-11 to 18-1's save point (run 1's work/ship/jan/back.py goals: the route's boulder steps, 18-9,
    18-8's east strip, straight down in 18-7 first, 18-6 .. 18-1 with room fights), Vegeta (four RIGHT; UP 6 frames
    first to stand in reach)"""
    def goals(R):
        i = R.index(('mark', 'vegeta'))
        k = next(n for n, g in enumerate(R) if g[0] == 'goto' and g[1] == '18-1')
        return list(R[i + 1:k]) + [('goto', '18-8', 200, 85), ('exitat', 200, 140, 'D'), ('keys', [(128, 70, 0)]),
                                   ('goto', '18-6', 270, 60), ('goto', '18-1', 104, 272), ('walk', 107, 240),
                                   ('keys', [(U, 6, 0)]), *R[R.index(('walk', 107, 240)) + 1:R.index(('mark', 'veg1'))]]
    return Chain([side_goals(goals, 'vegeta')])


g_j_back.chain = True


def g_j_vegeta(e):
    """Vegeta's levels for 18-12's gate (100) and Super Janemba's first round: to 108 on 18-2's Living Dead from 18-1
    (run 1: 120), back to 18-1"""
    return [('grind', 108, [(18, 2), (18, 1)]), nav(18, 1, False), ('wait', 30)]


def g_j_gate(e):
    """18-12's gate (Vegeta, s517: flag 183), 18-13 (s448, Pikkon), its save point: Goku again (four LEFT), his points
    a quarter each into STR / POW / END / SPD (run 1: STR / END), a Save there"""
    def goals(R):
        return [g for g in R[R.index(('mark', 'veg1')) + 1:R.index(('mark', 'h13'))] if g[0] != 'mark']

    def goku(e):
        n = e.r16(0x03001560 + 0x2C)
        q = n // 4
        return Driver2([*charsel(-4), ('wait', 20), alloc_keys([(0, [q, q, q, n - 3 * q])]), ('wait', 30)])
    return Chain([side_goals(goals, 'veg1'), goku])


g_j_gate.chain = True

ARENA_WAIT = 0


def arena_part(wait):
    """the side agent's arena route (tools/side/jroute.py ARENA, without its first expect: s448 is behind us, as run
    1's jan_arena.py): 18-14, s449, Janemba, Super Janemba's two rounds (Vegeta's, then Gogeta's after `wait` frames)"""
    def make(e):
        for d in ('tools/side', 'tools/superbuu', 'tools/story_playthrough'):
            if str(ROOT / d) not in sys.path:
                sys.path.insert(0, str(ROOT / d))
        import jroute
        goals = [g if g != ('wait', 'ARENA_WAIT') else ('wait', wait) for i, g in enumerate(jroute.ARENA)
                 if i not in (1, 2)]
        return jroute.Driver(goals)
    return make


def g_j_arena(e):
    """a Save at 18-13's save point (run 1: none), then the arena; a game over continues from the save"""
    sx, sy = save_spot(18, 13)
    return Chain([ship([('unstick',), ('walk', sx, sy), ('face', 'U'), SAVE, ('wait', 30)]),
                  retry_from(18, 13, [arena_part(ARENA_WAIT)])])


g_j_arena.chain = True

PIECES = {
    'j_arena': g_j_arena,
    'j_back': g_j_back,
    'j_vegeta': g_j_vegeta,
    'j_gate': g_j_gate,
    'j_maze': g_j_maze,
    'j_hfil': g_j_hfil,
    'j_grind': g_j_grind,
    'j_intro': g_j_intro,
    'dbx': g_dbx,
    'nataday': g_nataday,
    'thieves': g_thieves,
    'pilaf': g_pilaf,
    'desert': g_desert,
    'pyramid': g_pyramid,
    'fukurou': g_fukurou,
    'airship': g_airship,
    'grind_goten': g_grind_goten,
    'radar': g_radar,
    'bandana': g_bandana,
}


def keys(*prog):
    return ('keys', list(prog))


S, E = KEY.START, KEY.SELECT
# (ship_route2's programs) the save point menu: Save -> GAME 1 -> "Game Saved" closed
SAVE = keys((A, 3, 80), (A, 3, 80), (A, 3, 160), (0, 0, 60), (A, 3, 80))
# the death message -> the title -> Start -> Single Player -> GAME 1 (the last save)
DEATH_CONTINUE = keys((A, 3, 300), (S, 4, 200), (A, 4, 120), (A, 4, 300))


class Driver2(ShipDriver):
    """the live ShipDriver, plus:
      ('talkto', x, y[, side])  EarthDriver's: beside the live NPC nearest (x, y), face it, A, the talk played through
      ('needflag', n)           stop with an error unless story flag n is set (a piece's own check)"""
    from earth_driver import EarthDriver
    talkto = EarthDriver.talkto
    del EarthDriver

    def step(self, e):
        g = self.goals[self.k]
        if g[0] == 'talkto':
            return self.talkto(e, status(e), g)
        if g[0] == 'nav' and getattr(self, 'unjam', 0) > 0:
            self.unjam -= 1
            return self.unjam_key
        if g[0] == 'nav':
            # run 2: a nav that has stood still in one room for 1,200 frames (the driver's exit approach can loop on
            # a doorway: 10-27's door up to 10-28, punching the air) walks straight into the next exit for 60 frames
            s = status(e)
            pos = (tuple(s['area'][:2]), tuple(s['p'][:2]) if s['p'] else None)
            if pos == getattr(self, 'jam_pos', None) and s['scene'] == 0 and s['p'] is not None:
                self.jam_n += 1
            else:
                self.jam_pos, self.jam_n = pos, 0
            if self.jam_n > 1200:
                import nav as navm
                hops = navm.route('%d-%d' % (s['area'][0], s['area'][1]), '%d-%d' % (g[1], g[2]))
                if hops:
                    push = hops[0][1][2] or 'up'
                    self.unjam_key = {'up': U, 'down': D, 'left': L, 'right': R}[push]
                    self.unjam, self.jam_n = 60, 0
                    print('  unjam', s, push, flush=True)
                    return self.unjam_key
        if g[0] == 'needflag':
            if not e.r8(0x03001068 + 0x55 + g[1] // 8) >> (g[1] % 8) & 1:
                raise RuntimeError(f'flag {g[1]} not set')
            self.next_goal()
            return None
        return super().step(e)


class Chain:
    """drivers one after another: `parts` = [factory(e) -> driver], each built from the state its turn starts in"""

    def __init__(self, parts):
        self.parts, self.i, self.cur = parts, 0, None

    @property
    def k(self):
        return self.cur.k if self.cur else 0

    @property
    def goals(self):
        return self.cur.goals if self.cur else [None]

    def done(self):
        return self.i >= len(self.parts)

    def keys(self, e):
        if self.cur is None:
            self.cur = self.parts[self.i](e)
        if self.cur.done():
            self.i += 1
            self.cur = None
            return 0 if self.done() else self.keys(e)
        return self.cur.keys(e)


class Retry:
    """a stretch after an in-game save: when the game is over (area 0-2) the death -> title -> Continue program runs
    (`cont` goals) and the stretch (`parts`, rebuilt from the state then) starts over; at most `tries` game overs"""

    def __init__(self, parts, cont, tries=6):
        self.parts, self.cont, self.tries = parts, cont, tries
        self.chain, self.recover, self.deaths = Chain(parts), None, 0

    k = property(lambda self: (self.recover or self.chain).k)
    goals = property(lambda self: (self.recover or self.chain).goals)

    def done(self):
        return self.recover is None and self.chain.done()

    def keys(self, e):
        if self.recover is not None:
            if self.recover.done():
                self.recover = None
                self.chain = Chain(self.parts)
            else:
                return self.recover.keys(e)
        if status(e)['area'][:2] == [0, 2]:
            self.deaths += 1
            print(f'  game over #{self.deaths}: Continue', status(e), flush=True)
            if self.deaths > self.tries:
                raise RuntimeError('retry: too many game overs')
            self.recover = Driver2(list(self.cont))
            return self.recover.keys(e)
        return self.chain.keys(e)


def db57(route, fix=None, frm=None, to=None, real=False, emergency=None):
    """a dungeon agent B route (tools/db57: db5 Airship, db6 Fukurou, db7 Pyramid, dbx) as a chain part; `fix(goals)`
    adjusts its goal list (as run 1 did by hand)"""
    def make(e):
        if str(ROOT / 'tools' / 'db57') not in sys.path:
            sys.path.insert(0, str(ROOT / 'tools' / 'db57'))
        import run as db57run
        mk, drv = db57run.find_route(route)
        if real:
            drv = real_pyramid_driver()
        if emergency is not None:
            # (fat_db57's RealXDriver: the real Goku eats before Majin Buu's last hits)
            drv = type('RealXDriver', (drv,), {'EMERGENCY': emergency})
        g = mk()
        g = fix(g) if fix else g
        if frm:
            g = g[g.index(('mark', frm)):]
        if to:
            g = g[:g.index(('mark', to))]
        return drv(g)
    return make


def real_pyramid_driver():
    """re/buu/ship/fat_db57.py's RealPyramidDriver (that module runs its command line on import): the pit maze's raw
    'path' walks clear a visible Ghost out of the lane first (run 1's real Goten was drained dry behind one)"""
    import routes7
    from dbdriver import enemies as db_enemies

    class RealPyramidDriver(routes7.PyramidDriver):
        def step(self, e):
            g = self.goals[self.k]
            if g[0] == 'path' and self.t > 0:
                s = status(e)
                if s['scene'] == 0 and s['p'] is not None:
                    k = self.grab_guard(e)
                    if k is None:
                        k = self.blast(e, s, [o for o in db_enemies(e) if o[5] in self.GHOSTS], turn_ok=False)
                    if k is not None:
                        return k
            try:
                return super().step(e)
            except RuntimeError as ex:
                if g[0] == 'shoot' and 'no event' in str(ex):
                    # run 2: the bridge switch left in the other state (the shot before toggled it once more than
                    # meant): that switch again, then this goal
                    # (a shoot goal becomes ('events',) once its event starts: the route's own list)
                    sh = [x for x in routes7.BRIDGES_21 if x[0] == 'shoot']
                    prev = sh[:sh.index(g)] if g in sh else []
                    self.reshots = getattr(self, 'reshots', 0) + 1
                    if not prev or self.reshots > 6:
                        raise
                    print('  bridge switch again', prev[-1], status(e), flush=True)
                    self.goals[self.k:self.k] = [prev[-1]]
                    self.t = 0
                    self.shoot_t = None
                    return 0
                if g[0] != 'path' or 'left the area' not in str(ex):
                    raise
                # run 2: a fall into the pits below (14-12 .. 14-15) - back up to 14-17 and walk the maze's lanes
                # from its entrance again: every earlier path of the maze, then this one (the switches already
                # pressed stay pressed)
                self.falls = getattr(self, 'falls', 0) + 1
                if self.falls > 8:
                    raise
                seen = []
                for x in self.goals[:self.k]:
                    if x[0] == 'path' and x not in seen:
                        seen.append(x)
                redo = [('go', 14, 17, False)] + seen
                print('  pit fall', self.falls, status(e), '-> again', flush=True)
                self.goals[self.k:self.k] = redo
                self.t = 0
                return 0
    return RealPyramidDriver


def fix_db7(g):
    """man 2 (s414, flag 102): run 1's single A came too early and s406 then gave no ball (db7fix went back):
    three talks"""
    k = g.index(('walk', 200, 104))
    assert g[k + 1] == ('talk', 'U')
    return g[:k + 1] + [('talk', 'U'), ('wait', 30), ('talk', 'U'), ('wait', 30), ('talk', 'U'), ('events',)] + g[k + 2:]


def fix_db5(g):
    """the Warlord's lift: the route's `trig` stands 1 px left of the pad (run 1 took it by hand the same way)"""
    k = g.index(('trig', 104, 356, U))
    g = g[:k] + [('walk', 108, 358), ('hold', U, 280), ('events',)] + g[k + 1:]
    # run 2: the escape on the self-destruct countdown rushed (no fights at all; run 1 fought what came near)
    k = g.index(('go', 12, 1, 'near'))
    return g[:k] + [('go', 12, 1, False)] + g[k + 1:]


def ship(goals):
    return lambda e: Driver2(goals)


def driver(name, e):
    p = PIECES[name]
    return p(e) if isinstance(p, type(lambda: 0)) and getattr(p, 'chain', False) else Driver2(p(e))


def main():
    import sbemu
    from route import start_emu
    name, start = sys.argv[1], str(Path(sys.argv[2]).resolve())
    out = ROOT / 'work' / 'run2fat'
    out.mkdir(parents=True, exist_ok=True)
    e = start_emu(start)
    if len(sys.argv) > 3:
        sbemu.sd_write(e, Path(sys.argv[3]).read_bytes())
    d = driver(name, e)
    keys = []
    prev = None
    cap = int(sys.argv[sys.argv.index('--cap') + 1]) if '--cap' in sys.argv else 300000
    try:
        while not d.done():
            if len(keys) >= cap:
                raise RuntimeError('cap')
            k = d.keys(e) or 0
            e.set_keys(k)
            e.run(1)
            keys.append(k)
            s = status(e)
            if (s['area'][:2], d.k) != prev:
                prev = (s['area'][:2], d.k)
                print(len(keys), s, 'goal', d.k, d.goals[d.k] if not d.done() and d.k < len(d.goals) else None, flush=True)
    except Exception as ex:  # noqa: BLE001
        e.save_file(str(out / f'{name}.fail.ss'))
        e.screenshot(str(out / f'{name}.fail.png'), 2)
        print('FAIL', name, len(keys), d.k, d.goals[d.k] if d.k < len(d.goals) else None, ex, status(e))
        (out / f'{name}.fail.keys.json').write_text(json.dumps(keys))
        sys.exit(1)
    e.set_keys(0)
    e.save_file(str(out / f'{name}.ss'))
    (out / f'{name}.eeprom').write_bytes(sbemu.sd_read(e))
    (out / f'{name}.keys.json').write_text(json.dumps(keys))
    e.screenshot(str(out / f'{name}.png'), 2)
    print('done', name, len(keys), status(e))


# the chain's parts (work/run2/parts/<name>.json, segments.md "Run 2 hand-off"), pieces in order of play
PARTS = {
    'f_fat': ['radar', 'bandana', 'grind_goten', 'airship', 'fukurou', 'pyramid', 'desert', 'pilaf', 'thieves',
              'nataday', 'dbx'],
    'j_janemba': ['j_intro', 'j_grind', 'j_hfil', 'j_maze', 'j_back', 'j_vegeta', 'j_gate', 'j_arena'],
}


def write_parts():
    """`--parts`: work/run2fat/<piece>.keys.json joined into work/run2/parts/<part>.json"""
    for part, pieces in PARTS.items():
        keys, sizes = [], {}
        for p in pieces:
            k = json.loads((ROOT / 'work' / 'run2fat' / f'{p}.keys.json').read_text())
            keys += k
            sizes[p] = len(k)
        (ROOT / 'work' / 'run2' / 'parts' / f'{part}.json').write_text(
            json.dumps({'keys': keys, 'pieces': sizes, 'by': 'dungeon agent B, tools/run2/fat2.py'}))
        print(part, len(keys), sizes)


if __name__ == '__main__':
    if '--parts' in sys.argv:
        write_parts()
    else:
        main()
