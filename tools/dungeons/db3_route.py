"""Dragon Ball dungeon 3: Pilaf's Castle (world 10; the 3-Star Dragon Ball s366 at 10-31, story flag 114), played by
the real game from work/db/db3_start.ss (tools/dungeons/db3_save.py).

usage: .venv/bin/python tools/dungeons/db3_route.py db3 [-v] [--from MARK] [--to MARK]
  -> work/db/db3/{start.ss, <mark>.ss, end.ss, keys.json}
cut:   .venv/bin/python re/buu/ship/rec_segments.py work/db/db3 work/db/db3/segs --prefix db3"""
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'tools' / 'dungeons'))
import db_route  # noqa: E402
from db_route import nav, U, D, L, R, A, B  # noqa: E402,F401
from db3_driver import Db3Driver  # noqa: E402
from ship_driver import alloc_keys  # noqa: E402


def db3(e):
    """Pilaf's Castle: Trunks opens 10-1's gate (s517(3, 80, 0): flag 166), the yard 10-2 .. 10-5, the hall 10-12,
    the courtyard 10-14, 10-15's wall button (flag 260) -> 10-20 -> 10-21 -> 10-27 -> 10-28 -> 10-30 -> the throne
    room 10-31: Pilaf (s364), Pilaf's Guardian (s365: flag 94), the 3-Star Dragon Ball (s366: flag 114), then the
    same way back to 10-1 and the pad to the world map."""
    return [
        ('mark', 'a_start'),
        ('wait', 300),
        alloc_keys([(0, [5, 0, 4, 0])]), ('wait', 30),                  # Trunks' 9 unspent stat points: STR 5, END 4
        ('walk', 250, 86), ('hold', U, 12), ('press', A), ('expect', 60), ('wait', 30),   # the gate
        nav(10, 2), ('mark', 'b_10-2'),
        # the gap left of the lower ridge: the walker's tile (x // 8) is one off for the 17 px footprint there
        ('walk', 32, 152), ('walk', 32, 112),
        nav(10, 3), ('mark', 'b2_10-3'),
        nav(10, 5), ('mark', 'c_10-5'),
        nav(10, 12), ('mark', 'd_10-12'),
        nav(10, 14), ('mark', 'e_10-14'),
        nav(10, 15), ('mark', 'f_10-15'),
        ('walk', 108, 82), ('hold', U, 12), ('press', A), ('wait', 120),                  # the wall button (260)
        ('walk', 112, 68), ('hold', U, 60), ('until', 10, 20),                            # the 16 px breach
        nav(10, 20), ('mark', 'g_10-20'),
        nav(10, 21), ('mark', 'h_10-21'),
        nav(10, 27), ('mark', 'i_10-27'),
        nav(10, 28), ('mark', 'j_10-28'),
        nav(10, 30), ('mark', 'k_10-30'),
        nav(10, 31, False), ('mark', 'l_10-31'),
        ('expect', 200), ('wait', 20),                                                    # s364
        ('guardian', 63, 30000), ('mark', 'm_boss'),
        ('events',), ('wait', 30),                                     # s365 (played inside the guardian goal)
        ('walk', 200, 246), ('hold', U, 12), ('press', A), ('expect', 60), ('events',),   # s366: the Dragon Ball
        ('mark', 'n_ball'),
        nav(10, 21), ('mark', 'o_10-21'),
        nav(10, 20), ('mark', 'o2_10-20'),
        ('walk', 112, 212), ('hold', D, 60), ('until', 10, 15),                           # the breach, from above
        nav(10, 14), ('mark', 'p_10-14'),
        nav(10, 5), ('mark', 'q_10-5'),
        nav(10, 1), ('mark', 'r_10-1'),
        ('walk', 92, 100), ('hold', U, 12), ('press', A), ('wait', 300),                  # the world map
        ('mark', 's_map'),
    ]


ROUTES = {'db3': db3}

if __name__ == '__main__':
    db_route.main(ROUTES, Db3Driver)
