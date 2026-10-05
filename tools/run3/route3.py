"""Run 3: the adversarial playthrough (engine/docs/segments.md "Run 3"). One list of (segment name, Run3Driver goals);
tools/run3/play3.py plays it from the soft reset. Each segment is the story as runs 1 / 2 play it, with stresses on
the frames the engine is busy (see COVERAGE)."""
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'tools' / 'story_playthrough'))
from route import KEY, ROUTE_GOALS  # noqa: E402

U, D, L, R, A, B, S = KEY.UP, KEY.DOWN, KEY.LEFT, KEY.RIGHT, KEY.A, KEY.B, KEY.START

_i10 = ROUTE_GOALS.index(('area', 1, 10))
_i11 = ROUTE_GOALS.index(('walk', 272, 392))       # 1-10 -> down to 1-11


def stressed(goals):
    """run 1's goals with every exit push ('hold' a direction) turned into a push with START pressed on alternate
    frames through the whole transition (90 frames more: the fade-out, the load, the fade-in); run 1's L taps (the
    skill cursor from slot 0: the n-th tap selects slot n) become closed-loop selections of that slot - the pause tour's
    early L / R presses move the cursor in the field"""
    out = []
    taps = 0
    pushed = False
    for g in goals:
        # an arrival event the push's START presses already met (or played through behind the closing B): any event
        # there is played, none is required
        if g[0] == 'expect' and pushed:
            out.append(('events',))
            continue
        if g[0] not in ('area', 'expect'):
            pushed = False
        if g[0] == 'hold' and g[1] in (U, D, L, R) and g[2] >= 10:
            out.append(('pushmash', g[1], S, 2, g[2] + 90))
            pushed = True
            continue
        elif g[0] == 'hold' and g[1] == KEY.L and g[2] <= 3:
            taps += 1
            out.append(('skillslot', taps))
        else:
            out.append(g)
    return out


def fights_spun(goals):
    """three fast pause open / close spins before each fight goal"""
    out = []
    for g in goals:
        if g[0] == 'fight':
            out += [('pausespin', 3)]
        out.append(g)
    return out


SEGMENTS = [
    # the logos / title / Single Player / Select Game with START on alternate frames, then A on alternate frames
    # through New Game and every text box of the prologue (the typewriter's skip and the page turn on their first
    # accepted frames) until Goku walks on Snakeway
    ('intro3', [('mashkeys', S, 2, 240), ('textmash', 2, (1, 1))]),
    # Snakeway: the pause menu opened / closed as fast as it takes input, every page both ways, then run 1's route to
    # Grand Kai's mansion with START mashed through every area transition
    ('snakeway3', [('events',), ('pausespin', 8), ('menutour',)] + stressed(ROUTE_GOALS[:_i10 + 3])),
    # the rest of Other World (the fights of 1-12 .. 1-17, the Character Gate, the cave, Pikkon) -> Earth, START
    # mashed through every transition and the pause menu spun before every fight (an enemy's first frames with the
    # world frozen and released at once)
    ('ow3', fights_spun(stressed(ROUTE_GOALS[_i11:]))),
]

# ---- Earth (e3_*): run 1's Earth route (tools/story_playthrough/earth_route.py) with the stresses below; the Z-Mart
# stretch after Videl is the ship fix agent's e3_shops (tools/run3/shops3.py, ShopDriver)
for _d in ('tools/field', 'tools/run2', 'tools/run3'):
    if str(ROOT / _d) not in sys.path:
        sys.path.insert(0, str(ROOT / _d))
import earth_route  # noqa: E402
from shops3 import SHOPS3_GOALS  # noqa: E402

# pause menu -> Equip -> Body -> the first item -> Equip to the leader (route2.EQUIP_BODY)
EQUIP_BODY = ('keys', [(S, 2), (0, 60), (KEY.R, 3), (0, 40), (KEY.R, 3), (0, 40), (A, 3), (0, 40), (A, 3), (0, 40),
                       (A, 3), (0, 40), (B, 3), (0, 40), (S, 2), (0, 100)])


def stressed_exits(goals):
    """as stressed(), but only a hold followed by an ('area', ..) goal is an exit push (Earth's holds also face NPCs
    and walls); an expect right after a push is played as events"""
    out = []
    pushed = False
    for j, g in enumerate(goals):
        if g[0] == 'expect' and pushed:
            out.append(('events',))
            continue
        if g[0] not in ('area', 'expect'):
            pushed = False
        nxt = goals[j + 1] if j + 1 < len(goals) else None
        if g[0] == 'hold' and g[1] in (U, D, L, R) and g[2] >= 10 and nxt is not None and nxt[0] == 'area':
            out.append(('pushmash', g[1], S, 2, g[2] + 90))
            pushed = True
            continue
        out.append(g)
    return out


def fights_spun_all(goals):
    """three fast pause spins before every fight / fight2 / herd goal (but a snipe)"""
    out = []
    for g in goals:
        # (not before a 'snipe' fight: the tank's shells keep coming through the spins' unpaused frames)
        if g[0] in ('fight', 'fight2', 'herd') and 'snipe' not in g[2:]:
            out.append(('pausespin', 3))
        out.append(g)
    return out


def _earth3():
    segs = []
    for name, goals in earth_route.SEGMENTS:
        g = stressed_exits(list(goals))
        if name != 'tournament':    # (the ring refuses the pause)
            g = fights_spun_all(g)
        if name == 'house':
            # the pause pages both ways, then the Cotton Gi on, before leaving the house
            g = [('wait', 30), ('menutour',), EQUIP_BODY, ('bout',)] + g
        if name == 'videl':
            # after Videl (s132 / s133) the Z-Mart stretch is its own segment (e3_shops), then the rest
            k = g.index(('walk', 40, 150))
            segs += [('e3_videl', g[:k]), ('e3_shops', list(SHOPS3_GOALS)), ('e3_videl2', g[k:])]
            continue
        segs.append(('e3_' + name, g))
    return segs


SEGMENTS += _earth3()

# ---- the ship chapter (s3_ship, real-route agent): run 2's ship goals with run 3's stresses (tools/run3/ship3.py,
# ShipDriver3), played from Earth's end (ship3.py work/run3/end.ss end.eeprom OUT) in one session; its verified key
# log is stored (work/run3/parts/s3_ship.json, into keys.json): play3 replays it
SEGMENTS.append(('s3_ship', ('KEYS', 'work/run3/parts/s3_ship.json')))


def chapter_of(name):
    """a segment's chapter (routing: segments.md's owners)"""
    for pre, ch in (('e3_', 'earth'), ('s3_', 'ship'), ('f3_', 'fat'), ('j3_', 'janemba'), ('b3_', 'superbuu'),
                    ('p3_', 'postgame')):
        if name.startswith(pre):
            return ch
    return 'opening'


COVERAGE = """
Opening (Other World):
- title / menus / New Game with keys on alternate frames; the prologue's text mashed on alternate frames
- the pause menu opened and closed at the fastest rate it accepts, eight times; every page R then L
- START on alternate frames through every Snakeway / Yemma / Higher Plane transition (push, fade-out, load, fade-in)
- the rest of Other World the same way, the pause menu spun three times before every fight
Earth:
- START on alternate frames through every exit push; the pause spun three times before every fight
- the pause pages both ways and the Cotton Gi equipped in the house
- e3_shops (ship fix agent): every Buy row while affordable, the refusals, Sell, a supply used and one dropped
"""
