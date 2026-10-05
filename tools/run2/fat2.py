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
    return Chain([ship([('wait', 30), WEST_CITY, ('wait', 30), *zmart(3, 5), *take_off(416, 360),
                        AIRSHIP, ('wait', 200)]),
                  db57('db5', fix_db5)])


g_airship.chain = True

PIECES = {
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


def db57(route, fix=None):
    """a dungeon agent B route (tools/db57: db5 Airship, db6 Fukurou, db7 Pyramid, dbx) as a chain part; `fix(goals)`
    adjusts its goal list (as run 1 did by hand)"""
    def make(e):
        if str(ROOT / 'tools' / 'db57') not in sys.path:
            sys.path.insert(0, str(ROOT / 'tools' / 'db57'))
        import run as db57run
        mk, drv = db57run.find_route(route)
        g = mk()
        return drv(fix(g) if fix else g)
    return make


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
