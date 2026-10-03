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
ENEMY_LIST = 0x03001BB0


def ai_state(e, o):
    """the enemy's AI state stack (state ids, bottom first): obj+0x238 +4 depth, +8 {runtime, def} entries"""
    st = o + 0x238
    n = e.r32(st + 4)
    return [(e.r32(st + 12 + 8 * i) - 0x0870F514) // 8 for i in range(min(n, 8))]


def enemies(e):
    """[(obj, x px, y px, facing, hp)] from the enemy list"""
    out = []
    n = e.r32(ENEMY_LIST)
    for k in range(min(n, 64)):
        o = e.r32(ENEMY_LIST + 4 + 4 * k)
        sv = lambda v: v - (1 << 32) if v & 0x80000000 else v  # noqa: E731
        out.append((o, sv(e.r32(o + 0x1A0)) >> 8, sv(e.r32(o + 0x1A4)) >> 8, e.r8(o + 0x10), sv(e.r32(o + 0x28C))))
    return out


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
        self.reach_at = None
        self.skip = set()
        self.target = None
        self.prog = []

    HEALS = (21, 18, 17)        # Senzu Bean, Super Elixir, Elixir (supply ids), in order of preference

    def keys(self, e):
        if self.prog:
            return self.prog.pop(0)
        return super().keys(e)

    def maybe_heal(self, e, s, frac=0.35):
        """below `frac` of max HP in the field: the pause menu's Inventory, use the best healing supply
        (START, R to Inventory, DOWN to its row, A, A Use, B back to the field)"""
        if self.prog or s['p'] is None or s['scene'] > 0:
            return False
        rec = 0x03001560 + 0x68 * e.r8(0x03001068)
        hp, mx = e.r16(rec + 0x16), e.r16(rec + 0x18)
        if hp == 0 or hp * 100 >= mx * int(frac * 100):
            return False
        sup = e.read(0x03001068 + 2, 25)
        for it in self.HEALS:
            if sup[it]:
                row = sum(1 for i in range(it) if sup[i])
                prog = [(KEY.START, 2, 60), (KEY.R, 3, 50)] + [(KEY.DOWN, 3, 20)] * row + \
                    [(KEY.A, 3, 40), (KEY.A, 3, 80), (KEY.B, 3, 60)]
                for m, on, off in prog:
                    self.prog += [m] * on + [0] * off
                return True
        return False

    def next_goal(self):
        super().next_goal()
        self.walk_area = None
        self.reach_at = None

    def step(self, e):
        g = self.goals[self.k]
        kind = g[0]
        s = status(e)
        if kind == 'walk':
            if self.maybe_heal(e, s):
                return self.prog.pop(0)
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
        if kind == 'skill':
            # ('skill', slot): L (2 frames on, 10 off) until the selected skill slot is `slot`
            rec = 0x03001560 + 0x68 * e.r8(0x03001068)
            if e.r8(rec + 0x15) == g[1] and self.t % 12 == 0:
                self.next_goal()
                return None
            if self.t > 200:
                raise RuntimeError('skill slot not reached')
            return KEY.L if self.t % 12 < 2 else 0
        if kind == 'yakon':
            if s['scene'] > 0:
                return self.mash_a()
            return self.yakon(e, s, g)
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
        if self.maybe_heal(e, s):
            return self.prog.pop(0)
        x, y = s['p'][0], s['p'][1]
        ens = [o for o in enemies(e) if abs(o[1] - x) + abs(o[2] - y) <= radius and o[0] not in self.skip]
        ens = [o for o in ens if self.reachable(e, s, o[1], o[2])]
        if not ens or self.t >= cap:
            if self.t >= cap and ens:
                raise RuntimeError(f'fight not over after {cap} frames: {ens}')
            self.idle += 1
            if self.idle < 30:
                return 0
            self.next_goal()
            return None
        self.idle = 0
        en = min(ens, key=lambda o: abs(o[1] - x) + abs(o[2] - y))
        ex, ey = en[1], en[2]
        # an enemy that takes no damage for 900 frames of being the target (out of reach, invulnerable) is skipped
        if self.target is None or self.target[0] != en[0] or en[4] < self.target[1]:
            self.target = [en[0], en[4], self.t]
        elif self.t - self.target[2] > 900:
            self.skip.add(en[0])
            self.target = None
        dx, dy = ex - x, ey - y
        per = g[3] if len(g) > 3 else 10
        # in reach of the melee box (right: (16,-24)-(24,8), left: (-8,-24)-(0,8) around the footprint)
        if -20 <= dy <= 4 and 3 <= abs(dx) <= 21:
            face = 3 if dx > 0 else 2
            if s['p'][2] != face:
                self.mash = 0
                return KEY.RIGHT if dx > 0 else KEY.LEFT
            self.mash += 1
            return KEY.A if self.mash % per < 2 else 0
        if abs(dx) < 3 and abs(dy) <= 8:
            # on top of it: step aside to the free side
            self.mash = 0
            if self.grid is not None and self.grid.ok_box((x + 12) // 8, y // 8):
                return KEY.RIGHT
            return KEY.LEFT
        sides = sorted((-1, 1), key=lambda sd: abs(ex + sd * 16 - x))
        tx, ty = ex + sides[0] * 16, ey
        if self.grid is not None and not self.grid.ok_box(tx // 8, ty // 8):
            tx = ex + sides[1] * 16
        self.mash = 0
        if abs(x - tx) + abs(y - ty) > 24:
            return self.walk_keys(e, s, tx, ty)
        k = 0
        if abs(x - tx) > 1:
            k |= KEY.RIGHT if tx > x else KEY.LEFT
        if abs(y - ty) > 1:
            k |= KEY.DOWN if ty > y else KEY.UP
        return k

    def flee(self, e, s, ex, ey):
        """keys moving away from (ex, ey) on the live walk grid (the free direction that gains the most distance)"""
        x, y = s['p'][0], s['p'][1]
        if self.grid is None or tuple(s['area']) != self.grid_key:
            from grid import Grid, map_of
            self.grid = Grid(map_of(*s['area']))
            self.grid_key = tuple(s['area'])
        if self.t % 8 == 0:
            self.live_walk(e)
        best, bk = None, 0
        for k, dx, dy in ((KEY.LEFT, -1, 0), (KEY.RIGHT, 1, 0), (KEY.UP, 0, -1), (KEY.DOWN, 0, 1),
                          (KEY.LEFT | KEY.UP, -1, -1), (KEY.LEFT | KEY.DOWN, -1, 1), (KEY.RIGHT | KEY.UP, 1, -1),
                          (KEY.RIGHT | KEY.DOWN, 1, 1)):
            nx, ny = x + dx * 24, y + dy * 24
            if not all(self.grid.ok_box((x + dx * j) // 8, (y + dy * j) // 8) for j in (8, 16, 24)):
                continue
            d = abs(nx - ex) + abs(ny - ey)
            if best is None or d > best:
                best, bk = d, k
        return bk

    def yakon(self, e, s, g):
        """('yakon', cap): Yakon (re/buu/YAKON.md): flee his punch combo (0x321 / 0x322) and his mouth beam's column
        (0x326), Energy Blasts in the pause after the combo (0x323: full damage), the Mini-Yakons when summoned,
        punches otherwise (melee always hurts him). Done when no enemy is left."""
        if s['p'] is None:
            return 0
        if self.maybe_heal(e, s):
            return self.prog.pop(0)
        ens = enemies(e)
        if not ens:
            self.idle += 1
            if self.idle < 30:
                return 0
            self.next_goal()
            return None
        if self.t >= g[1]:
            raise RuntimeError(f'yakon not over after {g[1]} frames')
        self.idle = 0
        x, y = s['p'][0], s['p'][1]
        boss = [o for o in ens if ai_state(e, o[0])[-1:] and 0x31D <= ai_state(e, o[0])[-1] <= 0x326]
        if not boss:
            return self.fightp(e, s, ('fightp', 99999, 99999, 6))
        b = boss[0]
        st = ai_state(e, b[0])[-1]
        bx, by = b[1], b[2]
        minis = [o for o in ens if o[0] != b[0]]
        rec = 0x03001560 + 0x68 * e.r8(0x03001068)
        ep = e.r16(rec + 0x1A)
        if st in (0x321, 0x322):
            self.mash = 0
            return self.flee(e, s, bx, by)
        if minis:
            return self.fightp(e, s, ('fightp', 99999, 99999, 6))
        if st == 0x326:
            # the mouth beam goes down his column: stay beside it
            if abs(x - bx) < 40:
                return KEY.LEFT if x < bx or x > 400 else KEY.RIGHT
            return 0
        if st == 0x323 and ep >= 11:
            # below him, facing up, a blast every 12 frames
            tx, ty = bx, by + 40
            if abs(x - tx) > 3 or abs(y - ty) > 16:
                k = 0
                if abs(x - tx) > 3:
                    k |= KEY.RIGHT if tx > x else KEY.LEFT
                if abs(y - ty) > 16:
                    k |= KEY.DOWN if ty > y else KEY.UP
                self.mash = 0
                return k
            if s['p'][2] != 1:
                return KEY.UP
            self.mash += 1
            return KEY.B if self.mash % 12 < 2 else 0
        return self.fightp(e, s, ('fightp', 99999, 99999, 6))

    def reachable(self, e, s, ex, ey):
        """an enemy footprint tile reachable on the live walk grid (flood fill cached for 30 frames)"""
        if self.grid is None or tuple(s['area']) != self.grid_key:
            from grid import Grid, map_of
            self.grid = Grid(map_of(*s['area']))
            self.grid_key = tuple(s['area'])
            self.path = None
        if getattr(self, 'reach_at', None) is None or self.t - self.reach_at >= 30 or self.t < self.reach_at:
            self.live_walk(e)
            from collections import deque
            st = (s['p'][0] // 8, s['p'][1] // 8)
            seen = {st}
            q = deque([st])
            while q:
                cx, cy = q.popleft()
                for d in ((1, 0), (-1, 0), (0, 1), (0, -1)):
                    n = (cx + d[0], cy + d[1])
                    if n not in seen and self.grid.ok(*n):
                        seen.add(n)
                        q.append(n)
            self.reach = seen
            self.reach_at = self.t
        if len(self.reach) < 64:
            return True          # standing in a doorway / on a closed tile: unknown
        tx, ty = ex // 8, ey // 8
        return any((tx + a, ty + b) in self.reach for a in (-2, -1, 0, 1, 2) for b in (-1, 0, 1))

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
