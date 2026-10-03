"""Driver goals for the Earth chapters (Gohan / Great Saiyaman / tournament): driver.py's Driver plus
  ('walk', x, y)        as driver.py, but also done when the area changes (an event on the way warped)
  ('until', w, a)       mash A through events (no direction) until the area becomes (w, a) and no event runs
  ('press', keys)       press keys for 3 frames then release 3 (menus)
  ('choose', n)         in a choice box: DOWN n times then A (8 frames apart), then play the event through
  ('face', dir)         tap a direction 2 frames (turn in place)"""
from driver import Driver, DIRS
from route import KEY, status


class EarthDriver(Driver):
    def __init__(self, goals):
        super().__init__(goals)
        self.walk_area = None

    def next_goal(self):
        super().next_goal()
        self.walk_area = None
        self.tgt = None
        self.tgt_at = -99

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
            if self.t > 3000 and s['scene'] == 0:
                raise RuntimeError(f'walk {g} not done after 3000 frames: {s}')
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
            i = self.t // 8
            ph = self.t % 8
            if i < n:
                return KEY.DOWN if ph < 2 else 0
            if i == n:
                return KEY.A if ph < 2 else 0
            self.goals[self.k] = ('events',)
            return 0
        if kind == 'fly':
            return self.fly(e, s, g)
        if kind == 'talkto':
            return self.talkto(e, s, g)
        if kind == 'fight2':
            return self.fight2(e, s, g)
        if kind == 'alloc':
            return self.alloc(e, s, g)
        return super().step(e)

    def alloc(self, e, s, g):
        """('alloc', n_down): spend all stat points on one stat (Status page: START, A = Use Points, cursor DOWN
        n_down times from STR, RIGHT per point, A accepts, START closes)"""
        from common_field import record_of
        if self.t == 0:
            self.pts = record_of(e)['points']
            if self.pts == 0 or s['p'] is None or s['scene'] > 0:
                self.next_goal()
                return None
        seq = [(KEY.START, 3), (0, 60), (KEY.A, 3), (0, 30)]
        for _ in range(g[1]):
            seq += [(KEY.DOWN, 6), (0, 12)]
        for _ in range(self.pts):
            seq += [(KEY.RIGHT, 6), (0, 12)]
        seq += [(KEY.A, 3), (0, 30), (KEY.START, 3), (0, 60)]
        t = self.t
        for k, n in seq:
            if t < n:
                return k
            t -= n
        self.next_goal()
        return None

    def steer(self, e, s, tx, ty, blocked=()):
        """keys toward (tx, ty) px along a BFS path on the walk grid (None when unreachable)"""
        x, y = s['p'][0], s['p'][1]
        gk = tuple(s['area'])
        if gk != self.grid_key:
            from grid import Grid, map_of
            self.grid = Grid(map_of(*gk))
            self.grid_key = gk
            self.path = None
        start, goal = (x // 8, y // 8), (tx // 8, ty // 8)
        if self.path is None or self.t - self.path_at >= 16 or self.path[-1] != goal:
            self.path = self.grid.bfs(start, goal, 2, 1, set(blocked) - {start, goal})
            self.path_at = self.t
            if self.path is None:
                return None
        here = start
        while len(self.path) > 1 and (self.path[0] == here or abs(self.path[0][0] - here[0]) + abs(self.path[0][1] - here[1]) <= 1
                                      or abs(self.path[0][0] * 8 - x) <= 1 and abs(self.path[0][1] * 8 - y) <= 1):
            self.path.pop(0)
        wx, wy = (self.path[0][0] * 8, self.path[0][1] * 8) if len(self.path) > 1 else (tx, ty)
        k = 0
        if wx > x + 1:
            k |= KEY.RIGHT
        elif wx < x - 1:
            k |= KEY.LEFT
        if wy > y + 1:
            k |= KEY.DOWN
        elif wy < y - 1:
            k |= KEY.UP
        return k

    def fight2(self, e, s, g):
        """('fight2', cap): punch every enemy reachable on the walk grid (BFS to 18 px beside it); done when none is
        left or none is reachable for 90 frames; events on the way are played through"""
        from common_field import objs
        cap = g[1]
        if s['scene'] > 0:
            return self.mash_a()
        if s['p'] is None:
            return 0
        ens = [o for o in objs(e) if o[0] == 'enemy' and o[2] is not None]
        if not ens or self.t >= cap:
            self.idle += 1
            if self.idle < 30:
                return 0
            self.next_goal()
            return None
        x, y = s['p'][0], s['p'][1]
        # the current target: nearest by straight distance among the reachable ones (re-picked every 32 frames)
        if getattr(self, 'tgt_at', -99) + 32 <= self.t or getattr(self, 'tgt', None) is None:
            self.tgt = None
            self.tgt_at = self.t
            from grid import Grid, map_of
            gk = tuple(s['area'])
            if gk != self.grid_key:
                self.grid = Grid(map_of(*gk))
                self.grid_key = gk
            best = None
            for en in sorted(ens, key=lambda o: abs(o[2] - x) + abs(o[3] - y)):
                for side in ((-18 if x < en[2] else 18), (18 if x < en[2] else -18)):
                    spot = (en[2] + side, en[3])
                    p = self.grid.bfs((x // 8, y // 8), (spot[0] // 8, spot[1] // 8), 2, 1)
                    if p is not None and (best is None or len(p) < best[0]):
                        best = (len(p), en, side)
                        break
                if best is not None and best[0] < 6:
                    break
            if best is None:
                self.idle += 1
                if self.idle > 90:
                    self.next_goal()
                    return None
                return 0
            self.tgt = (best[1][2], best[1][3], best[2])
            self.path = None
        self.idle = 0
        en = min(ens, key=lambda o: abs(o[2] - self.tgt[0]) + abs(o[3] - self.tgt[1]))
        self.tgt = (en[2], en[3], self.tgt[2])
        side = -18 if x < en[2] else 18
        tx, ty = en[2] + side, en[3]
        if abs(x - tx) > 3 or abs(y - ty) > 2:
            if abs(x - tx) + abs(y - ty) < 24:
                k = 0
                if tx > x + 2:
                    k |= KEY.RIGHT
                elif tx < x - 2:
                    k |= KEY.LEFT
                if ty > y + 2:
                    k |= KEY.DOWN
                elif ty < y - 2:
                    k |= KEY.UP
            else:
                k = self.steer(e, s, tx, ty)
                if k is None:
                    self.tgt = None
                    return 0
            self.mash = 0
            return k
        want = KEY.RIGHT if side < 0 else KEY.LEFT
        if s['p'][2] != (3 if side < 0 else 2):
            return want
        self.mash += 1
        return KEY.A if self.mash % 10 < 2 else 0

    def talkto(self, e, s, g):
        """('talkto', x, y[, side]): walk next to the live NPC nearest (x, y) (side U/D/L/R of it, default D = stand
        below, face up), face it, press A, play the event through"""
        from common_field import objs
        if s['scene'] > 0:
            self.held = 1
            self.idle = 0
            return self.mash_a()
        if getattr(self, 'held', 0) == 1:
            self.idle += 1
            if self.idle >= 20:
                self.held = 0
                self.next_goal()
                return None
            return 0
        if s['p'] is None:
            return 0
        side = g[3] if len(g) > 3 else 'D'
        npcs = [o for o in objs(e) if o[0] in ('npc', 'npc_talk') and o[2] is not None]
        if not npcs:
            return 0
        n = min(npcs, key=lambda o: abs(o[2] - g[1]) + abs(o[3] - g[2]))
        off = {'D': (0, 10), 'U': (0, -10), 'L': (-18, 0), 'R': (18, 0)}[side]
        face = {'D': 'U', 'U': 'D', 'L': 'R', 'R': 'L'}[side]
        tx, ty = n[2] + off[0], n[3] + off[1]
        x, y = s['p'][0], s['p'][1]
        if abs(x - tx) <= 2 and abs(y - ty) <= 2:
            ph = self.t % 12
            if ph < 2:
                return DIRS[face]
            if ph < 6:
                return 0
            if ph < 8:
                return KEY.A
            return 0
        k = 0
        if tx > x + 1:
            k |= KEY.RIGHT
        elif tx < x - 1:
            k |= KEY.LEFT
        if ty > y + 1:
            k |= KEY.DOWN
        elif ty < y - 1:
            k |= KEY.UP
        return k

    def fly(self, e, s, g):
        """('fly', x, y, w, h): world map 0-50: climb above the encounters (z ~110 px), steer (LEFT / RIGHT turn,
        UP thrust) to the rect's centre, then descend (A) inside it until the landing script warps"""
        import math
        x0, y0, rw, rh = g[1], g[2], g[3], g[4]
        if s['area'][:2] != [0, 50]:
            # landed (or an encounter): play the events through, done once controllable elsewhere
            if s['scene'] > 0 or s['p'] is None:
                self.idle = 0
                return self.mash_a()
            self.next_goal()
            return None
        p = e.r32(0x03001C34)
        if not p or s['scene'] > 0:
            return self.mash_a() if s['scene'] > 0 else 0
        sv = lambda v: v - (1 << 32) if v & 0x80000000 else v
        x, y = sv(e.r32(p + 0x1A0)) / 256, sv(e.r32(p + 0x1A4)) / 256
        z, h = sv(e.r32(p + 0x1A8)) / 1024, sv(e.r32(p + 0x1AC)) / 1024
        tx, ty = x0 + rw / 2, y0 + rh / 2
        dx, dy = tx - x, ty - y
        dist = math.hypot(dx, dy)
        want = math.degrees(math.atan2(dx, -dy)) * 256 / 360
        diff = (want - h + 128) % 256 - 128
        k = 0
        if dist < min(rw, rh) / 2 - 4:
            return KEY.A
        if diff > 3:
            k |= KEY.RIGHT
        elif diff < -3:
            k |= KEY.LEFT
        if abs(diff) < 40 or dist > 120:
            k |= KEY.UP
        if z < 105 and dist > 150:
            k |= KEY.B
        elif dist < 400 and z > 20:
            k |= KEY.A
        return k
