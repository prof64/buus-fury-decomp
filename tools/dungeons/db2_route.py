"""Dragon Ball dungeon 2: Diablo Desert + Diablo Desert Express (world 11; the 2-Star Dragon Ball, s378, flag 113),
played by the real game from work/db/db2_start.ss (db2_save.py).

usage: .venv/bin/python tools/dungeons/db2_route.py db2 [-v] [--from MARK] [--to MARK]
  -> work/db/db2/{start.ss, <mark>.ss, end.ss, keys.json}
  cut: .venv/bin/python re/buu/ship/rec_segments.py work/db/db2 work/db/db2/segs --prefix db2"""
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import db_route  # noqa: E402
from db_route import nav, U, D, L, R, A  # noqa: E402,F401
from db2_driver import Db2Driver  # noqa: E402


B = db_route.B


def switch(w, a, x, y):
    """a floor switch under a rock; (x, y) = its trigger rect's top-left. The rock is a breakable with 2 HP that only
    energy attacks hurt (attack mask 2: "Perhaps if I used some kind of energy on it", s368): stand left of it on
    its row, face it, three Energy Blasts (B), then walk onto the switch (touch_once: the player's footprint top-left
    inside the rect). Its cutscene shows a pillar sinking in 11-15 and returns to the field."""
    return [('walk', x - 29, y + 9), ('rock', x + 20, y + 18),
            ('walk', x + 7, y + 9), ('wait', 120), ('until', w, a), ('wait', 10)]


def car(w, a, x=8):
    """the train: walk into the car's end trigger (the jump to the next car), until the next car is entered"""
    return [('trainwalk', x, 112), ('until', w, a), ('wait', 10)]


def db2(e):
    """Diablo Desert: Goten opens 11-1's gate (s517(2, 80, 1): flag 169); the desert is a wrapping 4 x 3 grid of
    areas (11-1 .. 11-12) under the plateau row 11-13 .. 11-16. The stairs up the plateau in 11-15 are blocked by four
    pillars, sunk by the floor switches of 11-2 (flag 89), 11-4 (90), 11-5 (91) and 11-10 (92). Up the stairs, west
    along the plateau to 11-13, the jump onto the Diablo Desert Express (11-17: the robbery s372, flag 93), through
    the cars 11-18 .. 11-20 (roof), down into 11-21 .. 11-24: the five robbers of 11-24 (flags 83 .. 87) -> s378, the
    2-Star Dragon Ball (flag 113). Back to 11-20's roof, whose pad now leads to the world map."""
    return [
        ('mark', 'a_start'),
        ('wait', 300),
        ('walk', 224, 204), ('hold', U, 12), ('press', A), ('expect', 60), ('wait', 30),      # the gate
        ('mark', 'b_gate'),
        nav(11, 2), *switch(11, 2, 409, 281), ('mark', 'c_sw1'),
        nav(11, 4), *switch(11, 4, 193, 233), ('mark', 'd_sw2'),
        nav(11, 6), nav(11, 5), *switch(11, 5, 81, 89), ('mark', 'e_sw3'),    # not through 11-3 (a Juggernaut)
        nav(11, 10, False), *switch(11, 10, 401, 41), ('mark', 'f_sw4'),
        nav(11, 15, False), ('mark', 'g_11-15'),
        ('walk', 312, 170),                                                                   # the stairs
        nav(11, 13, False), ('mark', 'h_11-13'),
        ('walk', 124, 40), ('hold', U, 12), ('press', A), ('until', 11, 17), ('wait', 10),    # onto the train
        ('mark', 'i_train'),
        ('fightp', 20000, 99999, 6), *car(11, 18),
        ('fightp', 20000, 99999, 6), *car(11, 19),
        ('fightp', 20000, 99999, 6), *car(11, 20),
        ('mark', 'j_11-20'),
        ('fightp', 20000, 99999, 6),
        ('trainwalk', 48, 100), ('until', 11, 21), ('wait', 10),                              # down into the cars
        ('mark', 'k_11-21'),
        nav(11, 24), ('mark', 'l_11-24'),          # the nav's room fight = the five robbers; the last one's defeat -> s378
        ('events',), ('wait', 30),
        ('mark', 'm_ball'),
        nav(11, 21), ('mark', 'n_11-21'),
        ('walk', 40, 200), ('until', 11, 20), ('wait', 10),
        ('walk', 224, 130), ('hold', U, 12), ('press', A), ('wait', 300),                     # the world map
        ('mark', 'o_map'),
    ]


ROUTES = {'db2': db2}

if __name__ == '__main__':
    db_route.main(ROUTES, Db2Driver)
