"""Closed-loop driver for the Babidi's ship / Majin Buu chapter (deterministic: keys are a pure function of the
emulator state and the goal progress). Builds on tools/story_playthrough/driver.py's Driver and adds:
  ('walk', x, y)        as driver.py, but also done when the area changes (an event on the way warped)
  ('until', w, a)       mash A through events until the area becomes (w, a), no event runs and the player exists
  ('press', keys)       press keys for 3 frames, release 3
  ('choose', n)         in a choice box: DOWN n times then A (8 frames apart), then play the event through
  ('face', dir)         tap a direction 2 frames
  ('beam', cap)         energy attack fight: line up with the nearest enemy on its row / column, hold B to fire the
                        selected skill (released every `hold` frames), punch when adjacent
"""
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(ROOT / 'tools/story_playthrough'))
sys.path.insert(0, str(ROOT / 'tools/field'))
from driver import Driver, DIRS  # noqa: E402
from route import KEY, status  # noqa: E402
from common_field import objs  # noqa: E402

MAPOBJ = 0x030019A4


def alloc_keys(chars):
    """Status page stat allocation: chars = [(tab_moves, [str, pow, end, spd]), ...] in order; tab_moves = RIGHT
    presses (negative: LEFT) from the previous tab (the first from the active character's). START opens, B closes."""
    k = [(KEY.START, 2, 60)]
    for moves, pts in chars:
        d = KEY.RIGHT if moves > 0 else KEY.LEFT
        k += [(d, 3, 14)] * abs(moves)
        k += [(KEY.A, 3, 20)]
        for i, n in enumerate(pts):
            k += [(KEY.RIGHT, 3, 14)] * n
            if i < 3:
                k += [(KEY.DOWN, 3, 14)]
        k += [(KEY.A, 3, 40)]
    k += [(KEY.B, 2, 60)]
    return ('keys', k)


class ShipDriver(Driver):
    def __init__(self, goals):
        super().__init__(goals)
        self.walk_area = None
        self.path_goal = None

    def next_goal(self):
        super().next_goal()
        self.walk_area = None

    def step(self, e):
        g = self.goals[self.k]
        kind = g[0]
        s = status(e)
        if kind == 'walk':
            if self.walk_area is None:
                self.walk_area = s['area'][:2]
            elif s['area'][:2] != self.walk_area:
                self.next_goal()
                return None
            return super().step(e)
        if kind == 'until':
            if s['area'][:2] == [g[1], g[2]] and s['scene'] == 0 and s['p'] is not None:
                self.idle += 1
                if self.idle >= 20:
                    self.next_goal()
                    return None
                return 0
            self.idle = 0
            return self.mash_a()
        if kind == 'press':
            if self.t < 3:
                return g[1]
            if self.t < 6:
                return 0
            self.next_goal()
            return None
        if kind == 'face':
            if self.t < 2:
                return DIRS[g[1]]
            if self.t < 4:
                return 0
            self.next_goal()
            return None
        if kind == 'choose':
            n = g[1]
            i, ph = self.t // 8, self.t % 8
            if i < n:
                return KEY.DOWN if ph < 2 else 0
            if i == n:
                return KEY.A if ph < 2 else 0
            self.goals[self.k] = ('events',)
            return 0
        if kind == 'keys':
            # ('keys', [(mask, on, off), ...]): a fixed key program (menus)
            t = self.t
            for m, on, off in g[1]:
                if t < on:
                    return m
                if t < on + off:
                    return 0
                t -= on + off
            self.next_goal()
            return None
        if kind == 'fightp':
            if s['scene'] > 0:
                return self.mash_a()
            return self.fightp(e, s, g)
        return super().step(e)

    # --- live walk grid (doors / gates opened by scripts) ---------------------------------------------------------

    def live_walk(self, e):
        raw = e.read(e.r32(MAPOBJ + 0xA8), 32 * self.grid.h)
        w = self.grid.w
        self.grid.walk = bytearray((raw[ty * 32 + (tx >> 3)] >> (tx & 7)) & 1
                                   for ty in range(self.grid.h) for tx in range(w))

    def walk(self, e, s, tx, ty):
        if s['p'] is not None and tuple(s['area']) != self.grid_key:
            from grid import Grid, map_of
            self.grid = Grid(map_of(*s['area']))
            self.grid_key = tuple(s['area'])
            self.path = None
            self.live_walk(e)
        elif s['p'] is not None and self.grid is not None and (self.path is None or self.t - self.path_at >= 16):
            self.live_walk(e)
        return super().walk(e, s, tx, ty)

    def fightp(self, e, s, g):
        """('fightp', cap[, radius]): punch enemies (nearest first) until none is left within `radius` px of the
        player (default: the whole area); approach along the live walk grid"""
        cap = g[1]
        radius = g[2] if len(g) > 2 else 99999
        if s['p'] is None:
            return 0
        x, y = s['p'][0], s['p'][1]
        ens = [o for o in objs(e) if o[0] == 'enemy' and o[2] is not None and abs(o[2] - x) + abs(o[3] - y) <= radius]
        if not ens or self.t >= cap:
            if self.t >= cap and ens:
                raise RuntimeError(f'fight not over after {cap} frames: {ens}')
            self.idle += 1
            if self.idle < 30:
                return 0
            self.next_goal()
            return None
        self.idle = 0
        en = min(ens, key=lambda o: abs(o[2] - x) + abs(o[3] - y))
        ex, ey = en[2], en[3]
        side = -1 if x < ex else 1
        tx, ty = ex + side * 18, ey
        if abs(x - tx) > 12 or abs(y - ty) > 6:
            self.mash = 0
            return self.walk_keys(e, s, tx, ty)
        k = 0
        if abs(x - tx) > 2:
            k |= KEY.RIGHT if tx > x else KEY.LEFT
        if abs(y - ty) > 2:
            k |= KEY.DOWN if ty > y else KEY.UP
        if k:
            self.mash = 0
            return k
        want = KEY.RIGHT if side < 0 else KEY.LEFT
        if s['p'][2] != (3 if side < 0 else 2):
            return want
        self.mash += 1
        return KEY.A if self.mash % 10 < 2 else 0

    def walk_keys(self, e, s, tx, ty):
        """walk()'s steering toward (tx, ty) without completing the goal"""
        saved = self.next_goal
        hit = []
        self.next_goal = lambda: hit.append(1)
        try:
            if self.path_goal != (tx // 8, ty // 8):
                self.path = None
            self.path_goal = (tx // 8, ty // 8)
            k = self.walk(e, s, tx, ty)
        finally:
            self.next_goal = saved
        if hit:
            self.path = None
            self.stuck = 0
            return 0
        return k or 0
