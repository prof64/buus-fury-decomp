"""Run 2's fat-Buu (and Janemba) chapters: the pieces after `s_ship` (route2.SEGMENTS entries ('f_<name>',
('EXT', '<name>'))), each a goal list for a driver built from the state the piece starts in. Owner: dungeon agent B
(engine/docs/segments.md "Run 2"). Input only - no RAM writes.

Choices that differ from run 1 (ship_route.goals_fatbuu + the dungeon agents' routes):
- Bulma's Dragon Radar talked to on the first visit (run 1's talk reached nobody: a second trip to West City)
- other food at West City's Z-Mart, other stat splits
- dungeon order Airship -> Fukurou Forest -> Kyodai Pyramid -> Diablo Desert -> Pilaf's Castle -> Thieves Den ->
  Nataday (run 1: Thieves, Pilaf, Desert, Airship, Fukurou, Pyramid, Nataday)

`driver(name, e)` -> a driver (done() / keys(e) / k / goals), as play2.py's loop wants it.
`python3 tools/run2/fat2.py NAME START.ss [EEPROM]` runs one piece (exploration): work/run2fat/<NAME>.{ss,eeprom,
keys.json}, a screenshot on failure."""
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
    Bandana for: level <= 70), Z-Mart (s030): six of the second food row (run 1: five Hamburgers, the third row),
    then the world map encounters until the Bandana (equipment 126) drops"""
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

PIECES = {
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


def db57(route, fix=None, frm=None, to=None, real=False):
    """a dungeon agent B route (tools/db57: db5 Airship, db6 Fukurou, db7 Pyramid, dbx) as a chain part; `fix(goals)`
    adjusts its goal list (as run 1 did by hand)"""
    def make(e):
        if str(ROOT / 'tools' / 'db57') not in sys.path:
            sys.path.insert(0, str(ROOT / 'tools' / 'db57'))
        import run as db57run
        mk, drv = db57run.find_route(route)
        if real:
            drv = real_pyramid_driver()
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
                    prev = [x for x in self.goals[:self.k] if x[0] == 'shoot']
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


if __name__ == '__main__':
    main()
