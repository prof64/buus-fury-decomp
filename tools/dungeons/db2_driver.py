"""Driver goals for Dragon Ball dungeon 2 (Diablo Desert + Express), on top of DbDriver:
  ('trainwalk', x, y)   walk toward (x, y) until an event starts or the area changes (a touch trigger on the way: the
                        train's car-to-car jumps); done on arrival as well
"""
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from db_driver import DbDriver, status, enemies, KEY  # noqa: E402,F401
from ship_driver import MAPOBJ  # noqa: E402


class Db2Driver(DbDriver):
    def step(self, e):
        g = self.goals[self.k]
        if g[0] == 'trainwalk':
            s = status(e)
            if s['scene'] > 0 or s['p'] is None and self.t > 0:
                self.next_goal()
                return None
            if self.tw_area is None:
                self.tw_area = s['area'][:2]
            elif s['area'][:2] != self.tw_area:
                self.next_goal()
                return None
            if s['p'] is None:
                return 0
            x, y = s['p'][0], s['p'][1]
            if abs(x - g[1]) <= 1 and abs(y - g[2]) <= 1 or self.stuck > 90:
                self.next_goal()
                return None
            return self.walk_keys(e, s, g[1], g[2])
        if g[0] == 'rock':
            # ('rock', x, y): Energy Blasts (B, one every 50 frames; facing right) until the live walk grid is open
            # at pixel (x, y) (a breakable rock's box is written blocked into the grid while it stands)
            s = status(e)
            if s['scene'] > 0 or s['p'] is None:
                return 0
            raw = e.read(e.r32(MAPOBJ + 0xA8) + (g[2] // 8) * 32, 32)
            tx = g[1] // 8
            ph = self.t % 50
            if (raw[tx >> 3] >> (tx & 7)) & 1 and ph == 0:
                self.next_goal()
                return None
            if self.t >= 600:
                raise RuntimeError('rock not broken')
            if s['p'][2] != 3:
                return KEY.RIGHT if ph < 2 else 0
            return KEY.B if 8 <= ph < 11 else 0
        return super().step(e)

    tw_area = None

    def maybe_heal(self, e, s, frac=0.35):
        # Goten at level 80 (361 HP) loses a third of his HP to one Hessian / Destroyer hit: eat earlier
        return super().maybe_heal(e, s, max(frac, 0.6))

    def next_goal(self):
        super().next_goal()
        self.tw_area = None
