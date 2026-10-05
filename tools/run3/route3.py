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
    frames through the whole transition (90 frames more: the fade-out, the load, the fade-in)"""
    out = []
    for g in goals:
        if g[0] == 'hold' and g[1] in (U, D, L, R) and g[2] >= 10:
            out.append(('pushmash', g[1], S, 2, g[2] + 90))
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

COVERAGE = """
Opening (Other World):
- title / menus / New Game with keys on alternate frames; the prologue's text mashed on alternate frames
- the pause menu opened and closed at the fastest rate it accepts, eight times; every page R then L
- START on alternate frames through every Snakeway / Yemma / Higher Plane transition (push, fade-out, load, fade-in)
- the rest of Other World the same way, the pause menu spun three times before every fight
"""
