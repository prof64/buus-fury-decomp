"""Side-content driver goals (on top of tools/superbuu/sbdriver.py):
  ('bossx', cap)   'boss' with every exit of the current area blocked for the walk (fights never leave the area)
"""
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent.parent
sys.path.insert(0, str(ROOT / 'tools' / 'superbuu'))
sys.path.insert(0, str(ROOT / 'tools' / 'story_playthrough'))
import sbdriver  # noqa: E402
import driver  # noqa: E402
from route import status  # noqa: E402

_step = driver.Driver.step


def step(self, e):
    g = self.goals[self.k]
    if g[0] == 'bossx':
        s = status(e)
        here = f"{s['area'][0]}-{s['area'][1]}"
        if getattr(self, '_bx', None) != (self.k, here):
            sbdriver.avoid_exits(here, None)
            self._bx = (self.k, here)
        idx = self.k
        self.goals[idx] = ('boss', g[1])
        try:
            r = _step(self, e)
        finally:
            self.goals[idx] = g
        if self.k != idx:
            sbdriver.AVOID.clear()
            self._bx = None
        return r
    return _step(self, e)


driver.Driver.step = step
