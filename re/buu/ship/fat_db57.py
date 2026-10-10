"""fat_db57.py ROUTE START.ss OUTDIR [--from MARK] [--to MARK]  -- tools/db57/run.py for the REAL route's states: the
same routes, with the Pyramid's raw 'path' walks (14-17's pit maze) clearing a visible Ghost out of the lane first
(the real Goten, weaker than the dungeon agent's level-100 Trunks, is drained dry behind one otherwise)."""
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(ROOT / 'tools' / 'db57'))
import run  # noqa: E402
import routes7  # noqa: E402
from dbdriver import status, enemies  # noqa: E402


class RealPyramidDriver(routes7.PyramidDriver):
    def step(self, e):
        g = self.goals[self.k]
        if g[0] == 'path' and self.t > 0:
            s = status(e)
            if s['scene'] == 0 and s['p'] is not None:
                k = self.grab_guard(e)
                if k is None:
                    k = self.blast(e, s, [o for o in enemies(e) if o[5] in self.GHOSTS], turn_ok=False)
                if k is not None:
                    return k
        return super().step(e)


def db7fix():
    """the real run's man 2 (s414, flag 102) was never spoken to (the route's single A press came too early), so
    s406 did not hand the ball over: from 14-1 back in to 14-10, twice A, out to Dr. Challenger, the pad"""
    A, U = routes7.A, routes7.U
    return [
        ('mark', 'q00_start'),
        ('go', 14, 10, 'near'),
        ('walk', 200, 104), ('talk', 'U'), ('wait', 30), ('talk', 'U'), ('wait', 30), ('talk', 'U'), ('events',),
        ('mark', 'q01_man2'),
        ('go', 14, 2, 'near'),
        ('walk', 264, 72), ('events',),            # s406: the 7-Star Dragon Ball
        ('mark', 'q02_ball'),
        ('go', 14, 1, 'near'),
        ('trig', 144, 216, A),                     # the pad: the world map (six balls: s515 -> s019 -> Nataday)
        ('wait', 120), ('events',), ('wait', 600), ('events',), ('wait', 120),
        ('mark', 'q03_map'),
    ]


routes7.ROUTES['db7fix'] = db7fix

import routesx  # noqa: E402


class RealXDriver(routesx.XDriver):
    # the real Goku (level 70, 399 HP) against Majin Buu's 45,000: eat well before the last hit
    EMERGENCY = float(__import__('os').environ.get('EMERGENCY', '0.45'))


routesx.DRIVER = RealXDriver
routes7.DRIVER = RealPyramidDriver
run.main()
