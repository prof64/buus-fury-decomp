"""Run 2: a second real playthrough, played differently from run 1 to reach code paths run 1 never did. One list of
(segment name, Run2Driver goals); tools/run2/play2.py plays it from the soft reset.

What differs from run 1 (and why), by segment - see COVERAGE below for the list of targeted code paths."""
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'tools' / 'story_playthrough'))
from route import KEY, ROUTE_GOALS  # noqa: E402

U, D, L, R, A, B = KEY.UP, KEY.DOWN, KEY.LEFT, KEY.RIGHT, KEY.A, KEY.B

# pause menu (START): Status -> D = Skills page; L shoulder -> Journal (D-pad R = Completed Goals tab); L shoulder ->
# Options: SFX volume -2 notches, Text Speed +2 (faster text for the rest of the run)
OPTIONS_1 = ('menu', 'S:3,0:60,D:5,0:40,U:5,0:30,LS:5,0:40,R:5,0:30,LS:5,0:40,L:5,0:20,L:5,0:20,D:5,0:20,D:5,0:20,'
                     'R:5,0:20,R:5,0:20,0:20')

def _ins(goals, before, extra):
    """goals with `extra` inserted before the first `before` goal"""
    k = goals.index(before)
    return goals[:k] + list(extra) + goals[k:]


_i10 = ROUTE_GOALS.index(('area', 1, 10))
_i11 = ROUTE_GOALS.index(('walk', 272, 392))       # 1-10 -> down to 1-11

DOWN_TO_1_12 = [('walk', 272, 392), ('hold', D, 30), ('area', 1, 11), ('expect', 200), ('wait', 30),
                ('walk', 584, 180), ('hold', R, 30), ('area', 1, 12), ('expect', 200), ('wait', 30)]

SEGMENTS = [
    # the title and the prologue's text with a slower A (every 37 frames, run 1: 30): other text-box timing, other
    # RNG draws from the first box on
    ('intro', [('intro', 37)]),
    # Snakeway -> Yemma -> the Higher Plane -> Grand Kai's Mansion 1-10 (run 1's goals)
    ('snakeway', [('events',)] + ROUTE_GOALS[:_i10 + 3]),
    # down to 1-12 and die among the fighters with no save: "You have died!" -> the title (a game over)
    ('gameover', DOWN_TO_1_12 + [('walk', 150, 140), ('die', 8000)]),
    # the same power-on: New Game again from the title (Game 1 still empty), the prologue's text at yet another rhythm
    ('newgame2', [('newgame', 45)]),
    # Snakeway again, walking into its edge for a while first (a wall push)
    ('snakeway2', [('events',), ('hold', U, 90), ('hold', U | L, 40)] + ROUTE_GOALS[:_i10 + 3]),
    # 1-10: the pause menu (Skills, Journal tabs, Options: SFX volume / text speed), then the save point: Save Game,
    # Game 1 (EEPROM write) - run 1 never saved
    ('save1', [OPTIONS_1, ('wait', 20), ('walk', 268, 362), ('save',), ('wait', 30)]),
    # down to 1-12 again and die, now with a save: the title again
    ('die', DOWN_TO_1_12 + [('walk', 150, 140), ('die', 8000)]),
    # title -> Single Player -> Game 1: Continue (the slot load) at 1-10's save point, in the power-on session that
    # has already played two games
    ('continue', [('continue',), ('wait', 30)]),
    # the rest of Other World as run 1 from 1-10 (its fights, the Character Gate, the cave, Pikkon) -> Earth 2-1;
    # the stat points go to SPD before the gate and to END before the tournament (run 1 spent none here)
    ('otherworld', _ins(_ins(ROUTE_GOALS[_i11:], ('walk', 256, 134), [('wait', 60), ('alloc', 3)]),
                        ('walk', 272, 172), [('wait', 60), ('alloc', 2)])),
]

import earth_route  # noqa: E402

S_, RS = KEY.START, KEY.R
# pause menu -> Equip -> Body -> the first item -> Equip to <leader> (run 1 never equipped on Earth)
EQUIP_BODY = ('keys', [(S_, 2), (0, 60), (RS, 3), (0, 40), (RS, 3), (0, 40), (A, 3), (0, 40), (A, 3), (0, 40), (A, 3),
                       (0, 40), (B, 3), (0, 40), (S_, 2), (0, 100)])
# pause menu -> Inventory -> the second supply -> its menu -> the second entry (Drop) (an item thrown away)
DROP_SECOND = ('keys', [(S_, 2), (0, 60), (RS, 3), (0, 40), (D, 3), (0, 30), (A, 3), (0, 40), (D, 3), (0, 30), (A, 3),
                        (0, 60), (B, 3), (0, 40), (S_, 2), (0, 100)])
# Hercule City 3-1 -> the Z-Mart 0-91: Buy -> Items -> the first (Miso Soup), back, Sell -> the third row, out
ZMART = [('walk', 136, 82), ('hold', U, 30), ('area', 0, 91), ('wait', 30), ('walk', 100, 110), ('hold', L, 20),
         ('keys', [(A, 3), (0, 120), (A, 3), (0, 90), (A, 3), (0, 90), (A, 3), (0, 60), (B, 3), (0, 60), (B, 3), (0, 60),
                   (D, 3), (0, 30), (A, 3), (0, 90), (D, 3), (0, 20), (D, 3), (0, 20), (A, 3), (0, 60)]),
         ('bout',), ('wait', 30),
         ('walk', 116, 200), ('hold', D, 30), ('area', 3, 1), ('wait', 30)]
# 3-1's save point (rect 136,128 49x41): Save Game; then a soft reset and Continue (the title's slot load)
SAVE_3_1 = [('walk', 152, 172), ('save',), ('walk', 152, 200), ('wait', 20), ('reset',), ('continue',), ('wait', 30)]


def _earth():
    segs = []
    for name, goals in earth_route.SEGMENTS:
        g = list(goals)
        if name == 'house':
            # the Cotton Gi on before leaving the house
            g = [('wait', 30), EQUIP_BODY, ('bout',)] + g
        if name == 'westcity':
            # West City 4-1: talk to the woman by the road (s179, a side NPC run 1 passed)
            g = _ins(g, ('walk', 850, 360), [('walk', 272, 96), ('talk', 'U'), ('wait', 30)])
        if name == 'training':
            # two more levels than run 1 before the circus (stat points into STR as run 1: the five fighters need it)
            g = [('levelup', 6, x[2]) if x[0] == 'levelup' else x for x in g]
        if name == 'circus':
            # a fighter can outlast fight2's first cap here (weaker STR): fight on
            k = g.index(('until', 3, 2))
            g = g[:k] + [('menu', earth_route.USE_FIRST_SUPPLY), ('fight2', 12000, 'stay'), ('wait', 30)] + g[k:]
        if name in ('tank', 'gate'):
            # END (tank) / SPD (gate) - run 1: POW both times
            g = [x if x != ('alloc', 1) else ('alloc', 2 if name == 'tank' else 3) for x in g]
        if name == 'videl':
            # after Videl (s132 / s133): the Z-Mart, a supply dropped, then 3-1's save point -> soft reset ->
            # Continue, before the world map circle
            k = g.index(('walk', 40, 150))
            g = g[:k] + ZMART + [DROP_SECOND, ('bout',)] + SAVE_3_1 + g[k:]
        segs.append(('e_' + name, g))
    return segs


SEGMENTS += _earth()
# the ship chapter (7-2 -> Babidi's ship -> Majin Buu): re/buu/ship/ship_route2.py goals_run2, built from the
# state reached here (play2.py drives it with its ShipDriver2)
SEGMENTS.append(('s_ship', 'SHIP2'))


def chapter_of(name):
    """the chapter a segment belongs to (routing: segments.md's owners)"""
    if name.startswith('e_'):
        return 'earth'
    if name.startswith('s_'):
        return 'ship'
    return 'opening'


COVERAGE = """
Opening (Other World):
- a game over with no save: death among 1-12's fighters -> "You have died!" -> the title
- a second New Game in the same power-on session (Game 1 still empty), the prologue at another A rhythm (37 / 45
  frames; run 1: 30) - and the zenie the first game left (100 at the second 1-1, the game does not clear it)
- pause menu pages run 1 never opened: Status -> Skills, Journal -> Completed Goals; Options: SFX volume -2, text
  speed +2 (every later text box types at another rate; the sound requests' volume)
- in-game Save Game at 1-10's save point (EEPROM write); a second death, now with a save -> title -> Single Player
  -> Game 1: Continue (the boot + slot load path) in a session that has played two games
- stat points spent (SPD before the Character Gate, END before the tournament; run 1 spent none in Other World)
- a wall push on Snakeway (blocked walking)
Earth:
- Equip (pause menu): the Cotton Gi on (run 1 never equipped on Earth)
- a side NPC talk in West City 4-1 (s179)
- six level-ups of training in 2-9 (run 1: four); the circus fight with a supply used mid-fight and fought on at low
  HP ('stay')
- the Hercule City Z-Mart (0-91): Buy -> Items (Miso Soup), Sell (a supply); an Inventory supply Dropped
- a second save (3-1) -> soft reset (A+B+START+SELECT) -> title -> Continue mid-chapter
- stat points into END (circus tank) and SPD (Goten's gate) - run 1: POW
"""
