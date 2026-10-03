"""Driver goals for the Dragon Ball dungeons, on top of the ship chapter's ShipDriver (re/buu/ship/ship_driver.py):
  ('tank', type, cap)   a Juggernaut (enemy 58: a tank that fills its canyon, drives at the player and shells its own
                        column - TankShoot, ~100 HP a shell): walk up under its body, block (R, facing it) while a
                        shell (object vt 0x0804A368) is in the air, punch upward otherwise; done when no enemy of the
                        type is left (its crew climbs out: fight them with 'fightp')
"""
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 're' / 'buu' / 'ship'))
sys.path.insert(0, str(ROOT / 'tools' / 'field'))
from ship_driver import ShipDriver, status, enemies, KEY  # noqa: E402,F401
import common_field as cf  # noqa: E402

SHELL = 0x0804A368


def world_vts(e):
    n = e.r32(cf.OBJW + 4)
    return [e.r32(e.r32(cf.OBJW + 8 + 4 * k)) for k in range(min(n, 255))]


class DbDriver(ShipDriver):
    def step(self, e):
        g = self.goals[self.k]
        if g[0] == 'tank':
            return self.tank(e, status(e), g)
        return super().step(e)

    def tank(self, e, s, g):
        if s['scene'] > 0:
            return self.mash_a()
        if s['p'] is None:
            return 0
        if self.maybe_heal(e, s):
            return self.heal_step(e) or 0
        en = [o for o in enemies(e) if o[5] == g[1]]
        if not en:
            self.idle += 1
            if self.idle >= 10:
                self.next_goal()
                return None
            return 0
        self.idle = 0
        if self.t >= g[2]:
            raise RuntimeError(f'tank not over after {g[2]} frames')
        x, y, f = s['p']
        ex, ey = en[0][1], en[0][2]
        tx = ex + 23
        above = y < ey + 30                     # coming from the far side: punch down from above its body
        face, fk = (0, KEY.DOWN) if above else (1, KEY.UP)
        gap = ey - y if above else y - ey
        near = 10 if above else 74
        if SHELL in world_vts(e) and gap > near + 6 and abs(x - tx) <= 24:
            return KEY.R if f == face else fk
        if abs(x - tx) > 6:
            return (KEY.RIGHT if x < tx else KEY.LEFT) | (fk if gap > near + 4 else 0)
        if gap > near:
            self.mash = 0
            return fk
        if f != face:
            return fk
        self.mash += 1
        return KEY.A if self.mash % 10 < 2 else 0
