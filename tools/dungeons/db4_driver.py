"""Driver goals for Dragon Ball dungeon 4 (world 15), on top of DbDriver:
  ('broly', cap)   Broly (enemy 10 / 11) at the Volcano Summit: fightp with the boss rule (never skipped); done when
                   the defeat event starts or no Broly is left
"""
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from db_driver import DbDriver, status, enemies, KEY  # noqa: E402,F401

import nav as navm  # noqa: E402

BROLY = (10, 11)

_approach = navm.approach


def approach(rect, push):
    """nav.approach, but a 9 px wide door entered upward (the Volcano's cave mouths, the huts) is met at rect.x + 4:
    the wall beside it stops a footprint lined up on the rect's centre (in this process only; nav.py is untouched)"""
    x, y = _approach(rect, push)
    if push == 'up' and rect['w'] <= 9:
        x = rect['x'] + 4
    return x, y


navm.approach = approach


class Db4Driver(DbDriver):
    def step(self, e):
        g = self.goals[self.k]
        if g[0] == 'broly':
            return self.broly(e, status(e), g)
        return super().step(e)

    def broly(self, e, s, g):
        en = [o for o in enemies(e) if o[5] in BROLY]
        if s['scene'] > 0 or not en:
            self.idle += 1
            if s['scene'] > 0 or self.idle >= 30:
                self.in_nav = False
                self.next_goal()
                return None
            return 0
        self.idle = 0
        if self.t >= g[1]:
            raise RuntimeError(f'broly not over after {g[1]} frames: {en}')
        self.in_nav = True
        self.fight_done = False
        return self.fightp(e, s, ('fightp', 10 ** 9, 99999, 6, True))
