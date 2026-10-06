"""field coverage tours of the world map's encounters: the Rogue Submarine 0-52 (never entered by a run; the
treasure-ship / enemy encounter of the five submarine patrols on 0-50 - enemy type 141, graphics def 0x086B43FC,
on-defeat s509 per patrol, flags 393 .. 397). State: work/side/pg_start.ss (the real post-game's first frame, Goku
flying on 0-50).

Adds one driver goal (patched into tours.TourDriver.step; other goals untouched):
  ('sub',)   on 0-50 climb, steer to the nearest submarine patrol and drop onto it; done once controllable in 0-52"""
import math
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(Path(__file__).resolve().parent))
sys.path.insert(0, str(ROOT / 'tools' / 'side'))
import tours  # noqa: E402
from route import KEY  # noqa: E402

SUB_DEF = 0x086B43FC
_step0 = tours.TourDriver.step


def _sub(self, e):
    from shipdrv import enemies
    s = tours.status(e)
    if s['area'][:2] == [0, 52]:
        if s['scene'] > 0 or s['p'] is None:
            self.idle = 0
            return self.mash_a()
        self.idle += 1
        if self.idle >= 40:
            self.next_goal()
            return None
        return 0
    if s['area'][:2] != [0, 50]:
        return self.mash_a()
    p = e.r32(0x03001C34)
    if not p:
        return 0
    sv = lambda v: v - (1 << 32) if v & 0x80000000 else v  # noqa: E731
    x, y = sv(e.r32(p + 0x1A0)) / 256, sv(e.r32(p + 0x1A4)) / 256
    z, h = sv(e.r32(p + 0x1A8)) / 1024, sv(e.r32(p + 0x1AC)) / 1024
    pats = [o for o in enemies(e) if o[5] == 141 and e.r32(o[0] + 0x58) == SUB_DEF]
    if not pats:
        # none in the object world: thrust on (the patrols are created as the camera nears them)
        return KEY.UP | (KEY.B if z < 100 else 0)
    o = min(pats, key=lambda o: math.hypot(o[1] - x, o[2] - y))
    dx, dy = o[1] - x, o[2] - y
    diff = (math.degrees(math.atan2(dx, -dy)) * 256 / 360 - h + 128) % 256 - 128
    k = 0
    if diff > 3:
        k |= KEY.RIGHT
    elif diff < -3:
        k |= KEY.LEFT
    if abs(diff) < 40:
        k |= KEY.UP
    dist = math.hypot(dx, dy)
    if dist > 90 and z < 100:
        k |= KEY.B
    elif dist <= 90 and z > 2:
        k |= KEY.A
    return k


def _step(self, e):
    g = self.goals[self.k]
    if g[0] == 'sub':
        if self.t > 6000:
            self.next_goal()
            return 0
        return _sub(self, e)
    return _step0(self, e)


tours.TourDriver.step = _step

# 0-50 -> a submarine patrol -> 0-52 (s509: a treasure ship - zenie on the deck - or an enemy group), fight what is
# there, the zenie, then A on the deck's pad (the script trigger at 65, 121, 47 x 39) back to the map
SUB = [('wait', 30), ('sub',), ('fight', 3000), ('wait', 60), ('a_at', 80, 140, 'U'), ('wait', 120)]

TOURS = {'world_sub': (str(ROOT / 'work' / 'side' / 'pg_start.ss'), SUB)}
