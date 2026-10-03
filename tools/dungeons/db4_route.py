"""Dragon Ball dungeon 4: Nataday Village / Northern Wilderness / Volcano (world 15), played by the real game from
work/db/db4_start.ss (tools/dungeons/db4_save.py).

usage: .venv/bin/python tools/dungeons/db4_route.py db4 [-v] [--from MARK] [--to MARK]
  -> work/db/db4/{start.ss, <mark>.ss, end.ss, keys.json}
     .venv/bin/python re/buu/ship/rec_segments.py work/db/db4 work/db/db4/segs --prefix db4"""
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import db_route  # noqa: E402
from db_route import nav, U, D, L, R, A, B  # noqa: E402,F401
from db4_driver import Db4Driver  # noqa: E402


def db4(e):
    """Nataday Village: the priest's hut 15-33 (s437: flag 108), the elder at the north gate (s431: flag 109), Goten
    opens the gate (s517(2, 100): flag 178), the Northern Wilderness 15-2 .. 15-9, 15-18, the Volcano 15-19 (the
    bridge trigger: flag 110), Broly at the Volcano Summit 15-32 (s435, defeat s436: flag 111), back down to the
    village, the priest hands over the 4-Star Dragon Ball (s439: flag 115), the pad to the world map."""
    return [
        ('mark', 'a_start'),
        ('wait', 200),
        # the priest's hut (its door is 9 px wide: x = 156): s437
        ('walk', 350, 400), ('walk', 180, 300), ('walk', 180, 240), ('walk', 156, 226), ('hold', U, 50),
        ('expect', 200), ('wait', 20),
        ('mark', 'b_priest'),
        nav(15, 1, False),
        ('walk', 220, 240), ('walk', 272, 200), ('walk', 272, 144),
        ('talk', 'U'), ('wait', 30),                                                     # s431: the elder steps aside
        ('mark', 'c_elder'),
        ('walk', 272, 138), ('hold', U, 12), ('press', A), ('expect', 60), ('wait', 30),  # the gate (Goten, level 100)
        ('mark', 'd_gate'),
        nav(15, 2), ('mark', 'e_15-2'),
        nav(15, 3), ('mark', 'e_15-3'),
        nav(15, 9), ('mark', 'e_15-9'),
        # 15-9's bridge is broken: around the river's east end (15-10 south bank, 15-11 .. 15-17, 15-10 north bank)
        nav(15, 10), ('mark', 'e1_15-10'), nav(15, 11), nav(15, 12), ('mark', 'e2_15-12'), nav(15, 13), nav(15, 14),
        ('mark', 'e3_15-14'), nav(15, 15), nav(15, 16), ('mark', 'e4_15-16'), nav(15, 17), nav(15, 10), nav(15, 9),
        ('mark', 'e5_15-9n'),
        nav(15, 18), nav(15, 19, False), ('mark', 'f_volcano'),
        # the Volcano's face 15-19 is a set of ledges joined by caves: the cave at the foot 15-20 -> the ledge with
        # the first bridge -> 15-22 -> the east ledge and its stairs -> 15-23 .. 15-25, 15-27, 15-28 -> the ledge of
        # the bridge switch (touch trigger 0x0808AEC0: flag 110, the second bridge appears)
        nav(15, 20), ('mark', 'f1_15-20'), nav(15, 19, False),
        ('walk', 164, 370), ('hold', R, 70),                                             # the first bridge
        nav(15, 22), ('mark', 'f2_15-22'),
        nav(15, 19, False), nav(15, 23), ('mark', 'f3_15-23'), nav(15, 24), nav(15, 25), ('mark', 'f4_15-25'),
        nav(15, 27), nav(15, 28), nav(15, 19, False),
        ('mark', 'f5_ledge'),
        ('walk', 336, 108), ('wait', 400),            # the floor switch: ~300 frames, the player held, no scene
        ('mark', 'g_bridge'),
        # back to 15-23, its other door -> the second bridge -> 15-29 .. 15-31 -> the top-left ledge -> the summit
        nav(15, 28, False), nav(15, 27), nav(15, 25), nav(15, 24), nav(15, 23),
        ('mark', 'g1_15-23'),
        nav(15, 19, False), ('mark', 'g1b_ledge'),
        ('walk', 404, 196), ('hold', L, 100),                                            # the second bridge
        nav(15, 29), ('mark', 'g2_15-29'), nav(15, 30), nav(15, 31), ('mark', 'g3_15-31'),
        nav(15, 19, False),
        nav(15, 32, False), ('mark', 'h_summit'),
        ('expect', 200), ('broly', 40000), ('events',), ('wait', 60),                    # s435, Broly, s436
        ('mark', 'i_broly'),
        # the way back: the same ledges, caves and river loop (enemies respawn on every entry)
        nav(15, 19, False), nav(15, 31), nav(15, 30), nav(15, 29), nav(15, 19, False),
        ('mark', 'j0_ledge'),
        ('walk', 260, 196), ('hold', R, 100),
        nav(15, 23), ('walk', 336, 280), ('hold', D, 60), ('wait', 40),                  # 15-23's east door
        nav(15, 22), ('walk', 68, 280), ('hold', D, 60), ('wait', 40),                   # 15-22's west door
        ('mark', 'j1_ledge'),
        ('walk', 270, 370), ('hold', L, 70),
        nav(15, 20), ('walk', 112, 360), ('hold', D, 60), ('wait', 40),                  # 15-20's south door
        ('mark', 'j_down'),
        nav(15, 18, False), nav(15, 9), nav(15, 10), nav(15, 17), nav(15, 16),
        nav(15, 15), nav(15, 14), nav(15, 13), nav(15, 12), nav(15, 11),
        nav(15, 10), nav(15, 9), ('mark', 'j_15-9'),
        nav(15, 8, False),          # 15-9's south bank is left at once (its two enemies can stand out of reach)
        nav(15, 1), ('mark', 'k_village'),
        ('walk', 272, 200), ('walk', 220, 240), ('walk', 156, 226), ('hold', U, 50),
        ('expect', 200), ('wait', 20),                                                   # s439: the Dragon Ball
        ('mark', 'l_ball'),
        nav(15, 1, False),
        ('walk', 180, 240), ('walk', 180, 300), ('walk', 350, 400),
        ('walk', 384, 652), ('hold', U, 12), ('press', A), ('wait', 400),                # the pad: the world map
        ('mark', 'm_map'),
    ]


if __name__ == '__main__':
    db_route.main({'db4': db4}, Db4Driver)
