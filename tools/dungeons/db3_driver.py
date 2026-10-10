"""Driver for Dragon Ball dungeon 3 (Pilaf's Castle), on top of DbDriver:
  ('guardian', type, cap)   Pilaf's Guardian (enemy 63): fight it with the room-fight logic until it is gone

walk(): the player's position is 24.8 fixed, so a footprint "at" tile-aligned x = 8k can hang over into tile k + 2
(and y = 8k into row k + 1): the base walker (2 x 1 tile box, waypoints at 8k) wedges on ridge corners (10-2). Here
the path search prefers tiles whose 3 x 2 box is free (others cost more) and aims at the middle of that box.
"""
import heapq
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'tools' / 'dungeons'))
from db_driver import DbDriver, status, enemies, KEY  # noqa: E402,F401
from ship_driver import objs  # noqa: E402

DIRS8 = ((1, 0), (-1, 0), (0, 1), (0, -1), (1, 1), (1, -1), (-1, 1), (-1, -1))


class Db3Driver(DbDriver):
    def step(self, e):
        g = self.goals[self.k]
        if g[0] == 'guardian':
            return self.guardian(e, status(e), g)
        return super().step(e)

    def keys(self, e):
        # watchdog: a nav / walk goal that keeps the player within 24 px for 1500 frames is wedged
        s = status(e)
        h = getattr(self, 'hist', None)
        if h is None:
            h = self.hist = []
        if not self.done() and self.goals[self.k][0] in ('nav', 'walk') and s['p'] is not None and s['scene'] == 0 \
                and not enemies(e):
            h.append((s['p'][0], s['p'][1]))
            if len(h) > 1500:
                del h[0]
                xs, ys = [p[0] for p in h], [p[1] for p in h]
                if max(xs) - min(xs) < 24 and max(ys) - min(ys) < 24:
                    raise RuntimeError(f'wedged at goal {self.k} {self.goals[self.k]} {s}')
        else:
            del h[:]
        return super().keys(e)

    def guardian(self, e, s, g):
        if s['scene'] > 0:
            return self.mash_a()
        if s['p'] is None:
            return 0
        en = [o for o in enemies(e) if o[5] == g[1]]
        if not en:
            self.idle += 1
            if self.idle >= 10:
                self.next_goal()
                return None
            return 0
        if self.t >= g[2]:
            raise RuntimeError(f'guardian not over after {g[2]} frames')
        return self.fightp(e, s, ('fightp', 10 ** 9, 99999, 6, True)) or 0

    # --- walking ------------------------------------------------------------------------------------------------

    def roomy(self, t):
        return self.grid.ok_box(t[0], t[1], 3, 2)

    def search(self, start, goal, blocked):
        """cheapest tile path (2 x 1 box must be free; a tile without a free 3 x 2 box costs 12 instead of 1)"""
        g = self.grid
        dist = {start: 0}
        prev = {start: None}
        pq = [(0, start)]
        while pq:
            d, p = heapq.heappop(pq)
            if p == goal:
                break
            if d > dist[p]:
                continue
            for dx, dy in DIRS8:
                n = (p[0] + dx, p[1] + dy)
                if n in blocked or not g.ok_box(n[0], n[1], 2, 1):
                    continue
                if dx and dy and not (g.ok_box(p[0] + dx, p[1], 2, 1) and g.ok_box(p[0], p[1] + dy, 2, 1)):
                    continue
                nd = d + (1 if g.ok_box(n[0], n[1], 3, 2) else 12)
                if nd < dist.get(n, 1 << 30):
                    dist[n] = nd
                    prev[n] = p
                    heapq.heappush(pq, (nd, n))
        if goal not in prev:
            return None
        path = []
        p = goal
        while p is not None:
            path.append(p)
            p = prev[p]
        return path[::-1]

    def find(self, start, goal, blocked):
        g = self.grid
        r = self.search(start, goal, blocked)
        if r is None and getattr(g, 'union', None) is not None:
            keep = g.walk                       # a closed door (opens when touched): the map's own grid
            g.walk = g.union
            try:
                r = self.search(start, goal, blocked)
            finally:
                g.walk = keep
        if r is None:
            for rad in (1, 2, 3):               # standing on a blocked tile: start from the nearest free one
                for dx in range(-rad, rad + 1):
                    for dy in range(-rad, rad + 1):
                        if max(abs(dx), abs(dy)) != rad:
                            continue
                        st = (start[0] + dx, start[1] + dy)
                        if g.ok_box(st[0], st[1]):
                            r = self.search(st, goal, blocked)
                            if r is not None:
                                return [start] + r
        return r

    def wp(self, t):
        return (t[0] * 8 + 4, t[1] * 8 + 4) if self.roomy(t) else (t[0] * 8, t[1] * 8)

    def walk(self, e, s, tx, ty):
        if s['p'] is None:
            return 0
        if tuple(s['area']) != self.grid_key:
            from grid import Grid, map_of
            self.grid = Grid(map_of(*s['area']))
            self.grid_key = tuple(s['area'])
            self.path = None
            self.live_walk(e)
        elif self.path is None or self.t - self.path_at >= 16:
            self.live_walk(e)
        x, y = s['p'][0], s['p'][1]
        if abs(x - tx) <= 1 and abs(y - ty) <= 1 or self.stuck > 90:
            self.next_goal()
            return None
        if self.path is None or self.t - self.path_at >= 16:
            start = (x // 8, y // 8)
            goal = (tx // 8, ty // 8)
            blocked = set()
            for o in objs(e):
                if o[0] in ('npc', 'npc_talk', 'enemy') and o[2] is not None:
                    nx, ny = o[2], o[3]
                    for bx in range((nx - 15) // 8, (nx + 15) // 8 + 1):
                        for by in range((ny - 7) // 8, (ny + 7) // 8 + 1):
                            blocked.add((bx, by))
            blocked.discard(start)
            blocked.discard(goal)
            self.path = self.find(start, goal, blocked) or self.find(start, goal, set()) or [goal]
            self.path_at = self.t
        while len(self.path) > 1:
            wx, wy = self.wp(self.path[0])
            if abs(wx - x) <= 5 and abs(wy - y) <= 5:
                self.path.pop(0)
            else:
                break
        wx, wy = self.wp(self.path[0]) if len(self.path) > 1 else (tx, ty)
        k = 0
        if wx > x + 1:
            k |= KEY.RIGHT
        elif wx < x - 1:
            k |= KEY.LEFT
        if wy > y + 1:
            k |= KEY.DOWN
        elif wy < y - 1:
            k |= KEY.UP
        if (x, y) == self.last:
            self.stuck += 1
        else:
            self.stuck = 0
        self.last = (x, y)
        if self.stuck > 12 and k:
            if k & (KEY.UP | KEY.DOWN) and not k & (KEY.LEFT | KEY.RIGHT):
                k = KEY.LEFT if (self.stuck // 24) % 2 == 0 else KEY.RIGHT
            elif k & (KEY.LEFT | KEY.RIGHT) and not k & (KEY.UP | KEY.DOWN):
                k = KEY.DOWN if (self.stuck // 24) % 2 == 0 else KEY.UP
            else:
                k &= KEY.LEFT | KEY.RIGHT
        self.held = k
        return k
