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
        self.walk_t = 0
        self.tgt = None
        self.tgt_at = -99
        self.bans = {}
        self.engaged = {}
        self.tmode = None
        self.hphase = 0

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
            self.walk_t = getattr(self, 'walk_t', 0) + (1 if s['scene'] == 0 else 0)
            if self.walk_t > 3000:
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
        if kind == 'herd':
            return self.herd(e, s, g)
        if kind == 'levelup':
            # ('levelup', n, cycle): repeat the goal list `cycle` (a round trip between areas with fights) until the
            # current character's level went up by n since this goal started (a level-up refills HP / EP)
            import json as _json
            from common_field import record_of
            if not hasattr(self, 'xp_table'):
                from grid import CONTENT
                self.xp_table = [l['total_xp'] for l in _json.loads((CONTENT / 'levels.json').read_text())['levels']]
            xp = record_of(e)['xp']
            lvl = max(i for i in range(len(self.xp_table)) if xp >= self.xp_table[i])
            if len(g) < 4:
                self.goals[self.k] = g = (g[0], g[1], g[2], lvl + g[1])
            if lvl >= g[3]:
                self.next_goal()
                return None
            self.goals[self.k:self.k] = list(g[2])
            self.k -= 1
            self.next_goal()
            return None
        if kind == 'dodge':
            # ('dodge', x0, x1, y): keep running between x0 and x1 on row y (Goten's thrown rocks land where the
            # player stood when thrown) until an event runs and ends (s121), then as 'events'
            if s['scene'] > 0:
                return self.mash_a()
            if e.r8(0x03001068 + 0x55 + (g[4] >> 3)) >> (g[4] & 7) & 1:
                self.goals[self.k] = ('events',)
                return 0
            if s['p'] is None:
                return 0
            x, y = s['p'][0], s['p'][1]
            if not hasattr(self, 'dodge_dir') or self.t == 0:
                self.dodge_dir = 1
            if x >= g[2]:
                self.dodge_dir = -1
            elif x <= g[1]:
                self.dodge_dir = 1
            # a rock in the air (object vt 0x0804CE50: +0x20 x 24.8, +0x40 vx, +0x48 frames left): run away from
            # where it lands
            from common_field import OBJW
            sv = lambda v: v - (1 << 32) if v & 0x80000000 else v
            for i in range(e.r32(OBJW + 4)):
                o = e.r32(OBJW + 8 + 4 * i)
                if e.r32(o) == 0x0804CE50:
                    lx = (sv(e.r32(o + 0x20)) + sv(e.r32(o + 0x40)) * e.r32(o + 0x48)) >> 8
                    away = 1 if x + 8 > lx else -1
                    if (away > 0 and x < g[2]) or (away < 0 and x > g[1]):
                        self.dodge_dir = away
                    break
            k = KEY.RIGHT if self.dodge_dir > 0 else KEY.LEFT
            if y < g[3] - 2:
                k |= KEY.DOWN
            elif y > g[3] + 2:
                k |= KEY.UP
            return k
        if kind == 'talkto':
            return self.talkto(e, s, g)
        if kind == 'fight2':
            return self.fight2(e, s, g)
        if kind == 'alloc':
            return self.alloc(e, s, g)
        if kind == 'menu':
            # ('menu', program): a fixed key program (route.program_keys syntax), then B until the field is back
            from route import program_keys
            if self.t == 0:
                self.menu_keys = program_keys(g[1])
            if self.t < len(self.menu_keys):
                return self.menu_keys[self.t]
            u = self.t - len(self.menu_keys)
            if s['p'] is None and u < 600:
                return KEY.B if u % 16 < 3 else 0
            self.next_goal()
            return None
        return super().step(e)

    def alloc(self, e, s, g):
        """('alloc', n_down): spend all stat points on one stat (Status page: START, A = Use Points, cursor DOWN
        n_down times from STR, RIGHT per point, A accepts, START closes)"""
        from common_field import record_of
        if self.t == 0:
            if self.k != getattr(self, 'alloc_k', None):
                self.alloc_k, self.alloc_try = self.k, 0
            self.pts = record_of(e)['points']
            if self.pts == 0 or s['p'] is None or s['scene'] > 0:
                self.next_goal()
                return None
        seq = [(KEY.START, 3), (0, 60), (KEY.A, 6), (0, 30)]
        for _ in range(g[1]):
            seq += [(KEY.DOWN, 5), (0, 20)]
        for _ in range(self.pts):
            seq += [(KEY.RIGHT, 6), (0, 12)]
        seq += [(KEY.A, 6), (0, 30), (KEY.START, 6), (0, 60)]
        t = self.t
        for k, n in seq:
            if t < n:
                return k
            t -= n
        # still in the menu (a press lost to a redraw): B until the field is back
        if s['p'] is None and t < 600:
            return KEY.B if t % 16 < 3 else 0
        # points left (a press lost): try again, at most 3 times
        if t < 30:
            return 0
        if record_of(e)['points'] > 0 and self.alloc_try < 2:
            self.alloc_try += 1
            self.t = -1
            return 0
        self.alloc_try = 0
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
            bl = set(blocked) | (self.exit_tiles(e, s)[1] if self.goals[self.k][0] == 'fight2' else set())
            self.path = self.grid.bfs(start, goal, 2, 1, bl - {start, goal})
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

    def exit_tiles(self, e, s):
        """the live exit triggers of the area: rects (px) and the walk-grid tiles they cover (+1 tile margin)"""
        from common_field import trigs
        key = tuple(s['area'])
        if getattr(self, 'ex_key', None) != key:
            rects = [t[2:6] for t in trigs(e) if t[0] in ('exit', 'edge_exit')]
            tiles = set()
            for x0, y0, x1, y1 in rects:
                for tx in range(x0 // 8 - 2, x1 // 8 + 1):
                    for ty in range(y0 // 8 - 1, y1 // 8 + 1):
                        tiles.add((tx, ty))
            self.ex_key, self.ex_rects, self.ex_tiles = key, rects, tiles
        return self.ex_rects, self.ex_tiles

    def fight2(self, e, s, g):
        """fight2 without walking into exits (keys that would bring the footprint near an exit are dropped); done
        when the area changes"""
        if self.walk_area is None:
            self.walk_area = s['area'][:2]
        elif s['area'][:2] != self.walk_area:
            self.next_goal()
            return None
        if s['p'] is not None:
            px, py = s['p'][0] // 8, s['p'][1] // 8
            self.exit_tiles(e, s)
            self.ex_tiles = {t for t in self.ex_tiles if abs(t[0] - px) > 3 or abs(t[1] - py) > 3}
        k = self._fight2(e, s, g)
        if not k or s['p'] is None or s['scene'] > 0:
            return k
        rects, _ = self.exit_tiles(e, s)
        x, y = s['p'][0], s['p'][1]
        near = lambda px, py: any(px + 16 > x0 - 4 and px < x1 + 4 and py + 8 > y0 - 4 and py < y1 + 4
                                  for x0, y0, x1, y1 in rects)
        here = near(x, y)
        for bit, dx, dy in ((KEY.RIGHT, 4, 0), (KEY.LEFT, -4, 0), (KEY.DOWN, 0, 4), (KEY.UP, 0, -4)):
            if k & bit and near(x + dx, y + dy) and not here:
                k &= ~bit
        return k

    FACE = {'R': 3, 'L': 2, 'U': 1, 'D': 0}

    @staticmethod
    def enemies(e):
        """live enemies in the object list: [x, y, foot w, foot h] (px, footprint top-left)"""
        from common_field import OBJW
        out = []
        n = e.r32(OBJW + 4)
        sv = lambda v: v - (1 << 32) if v & 0x80000000 else v
        for k in range(min(n, 255)):
            o = e.r32(OBJW + 8 + 4 * k)
            if e.r32(o) == 0x0804C484:
                out.append([sv(e.r32(o + 0x1A0)) >> 8, sv(e.r32(o + 0x1A4)) >> 8, e.r8(o + 0x19C) or 16, e.r8(o + 0x19D) or 8])
        return out

    @staticmethod
    def spots(en, ranged, far=False):
        """player positions (footprint top-left) to attack enemy en from: (x, y, facing)"""
        ex, ey, fw, fh = en
        cy = ey + fh // 2 - 4
        cx = ex + fw // 2 - 8
        d = (90 if far else 48) if ranged else 2
        return [(ex - 16 - d, cy, 'R'), (ex + fw + d, cy, 'L'), (cx, ey + fh + d, 'U'), (cx, ey - 8 - d, 'D')]

    def _fight2(self, e, s, g):
        """('fight2', cap[, 'melee']): fight every enemy reachable on the walk grid: with energy left line up on its
        row / column and tap B (Energy Blast), else punch from beside / above / below it (enemy footprints); done when
        none is left or none is reachable for 90 frames; events on the way are played through"""
        from common_field import record_of
        cap = g[1]
        if s['scene'] > 0:
            return self.mash_a()
        if s['p'] is None:
            return 0
        rc = record_of(e)
        # low on HP: stop fighting (the next goals walk on); enemies engaged for 600 frames are given up
        if rc['hp'] * 100 < rc['maxhp'] * 30 and 'stay' not in g[2:]:
            self.next_goal()
            return None
        eng = getattr(self, 'engaged', {})
        self.engaged = eng
        ens = [o for o in self.enemies(e) if eng.get((o[0] // 32, o[1] // 32), 0) < 600]
        if not ens or self.t >= cap:
            self.idle += 1
            if self.idle < 30:
                return 0
            self.next_goal()
            return None
        x, y = s['p'][0], s['p'][1]
        if getattr(self, 'tgt', None) is not None:
            tk = (self.tgt[0][0] // 32, self.tgt[0][1] // 32)
            eng[tk] = eng.get(tk, 0) + 1
        ranged = 'melee' not in g[2:] and rc['ep'] * 100 >= rc['maxep'] * 8
        snipe = 'snipe' in g[2:] and any(o[2] >= 48 for o in ens)
        if snipe:
            # keep blasting from range; out of energy (below 40%): wait out of every enemy's rows / columns until it is
            # back (no melee)
            if rc['ep'] * 100 < rc['maxep'] * (30 if getattr(self, "resting", False) else 8):
                self.resting = True
                from grid import Grid, map_of
                gk = tuple(s['area'])
                if gk != self.grid_key:
                    self.grid = Grid(map_of(*gk))
                    self.grid_key = gk
                def safe(px, py):
                    return all(not (py + 8 > ey - 4 and py < ey + fh + 4) and not (px + 16 > ex - 4 and px < ex + fw + 4)
                               for ex, ey, fw, fh in ens)
                if safe(x, y):
                    return 0
                for dd in range(8, 96, 4):
                    for sy in (y + dd, y - dd):
                        if self.grid.ok_box(x // 8, sy // 8, 2, 1) and self.grid.ok_box(x // 8, (sy + 7) // 8, 2, 1) and safe(x, sy):
                            return KEY.DOWN if sy > y else KEY.UP
                return 0
            self.resting = False
        if (x, y) == getattr(self, 'flast', None):
            self.fstuck = getattr(self, 'fstuck', 0) + 1
        else:
            self.fstuck = 0
        self.flast = (x, y)
        # attack when lined up with any enemy
        for ex, ey, fw, fh in sorted(ens, key=lambda o: abs(o[0] - x) + abs(o[1] - y)):
            rows = y + 8 > ey and y < ey + fh
            cols = x + 16 > ex and x < ex + fw
            gx = ex - (x + 16) if x + 8 < ex + fw // 2 else x - (ex + fw)
            gy = ey - (y + 8) if y + 4 < ey + fh // 2 else y - (ey + fh)
            fr = 'R' if x + 8 < ex + fw // 2 else 'L'
            fd = 'D' if y + 4 < ey + fh // 2 else 'U'
            att = None
            if rows and -4 <= gx <= 6:
                att = (fr, KEY.A, 10)
            elif cols and -4 <= gy <= 6:
                att = (fd, KEY.A, 10)
            elif ranged and y + 6 > ey and y + 2 < ey + fh and (84 if snipe else 12) <= gx <= (100 if snipe else 96):
                att = (fr, KEY.B, 14)
            elif ranged and x + 12 > ex and x + 4 < ex + fw and (84 if snipe else 12) <= gy <= (100 if snipe else 90):
                att = (fd, KEY.B, 14)
            if att is not None:
                self.fstuck = 0
                self.idle = 0
                if s['p'][2] != self.FACE[att[0]]:
                    return DIRS[att[0]]
                self.mash += 1
                return att[1] if self.mash % att[2] < (3 if att[1] == KEY.B else 2) else 0
        # (re)pick a spot: the shortest BFS path among the nearest enemies' spots; every 32 frames, when the mode
        # changes, or after 20 frames without moving (that spot is banned for a while)
        bans = getattr(self, 'bans', {})
        if (getattr(self, 'tgt_at', -99) + 32 <= self.t or getattr(self, 'tgt', None) is None
                or getattr(self, 'tmode', None) != ranged or self.fstuck >= 20):
            if self.fstuck >= 20 and getattr(self, 'tgt', None) is not None:
                bans[self.tgt[1:]] = self.t + 120
                self.fstuck = 0
            self.bans = bans
            self.tgt = None
            self.tgt_at = self.t
            self.tmode = ranged
            from grid import Grid, map_of
            gk = tuple(s['area'])
            if gk != self.grid_key:
                self.grid = Grid(map_of(*gk))
                self.grid_key = gk
            blocked = self.exit_tiles(e, s)[1] - {(x // 8, y // 8)}
            best = None
            for i, en in enumerate(sorted(ens, key=lambda o: abs(o[0] - x) + abs(o[1] - y))[:3]):
                for oi, (sx, sy, f) in enumerate(self.spots(en, ranged, snipe)):
                    if bans.get((i, oi), -1) > self.t:
                        continue
                    p = self.grid.bfs((x // 8, y // 8), (sx // 8, sy // 8), 2, 1, blocked)
                    if p is not None and (best is None or len(p) < best[0]):
                        best = (len(p), i, oi, en)
            if best is None:
                self.idle += 1
                if self.idle > 240:
                    self.next_goal()
                    return None
                # no spot reachable on the static grid: chase the nearest one directly
                en = min(ens, key=lambda o: abs(o[0] - x) + abs(o[1] - y))
                k = 0
                if en[0] > x + 2:
                    k |= KEY.RIGHT
                elif en[0] < x - 2:
                    k |= KEY.LEFT
                if en[1] > y + 2:
                    k |= KEY.DOWN
                elif en[1] < y - 2:
                    k |= KEY.UP
                self.tgt = None
                return k
            self.tgt = (best[3][:2], best[1], best[2])
            self.path = None
        self.idle = 0
        en = min(ens, key=lambda o: abs(o[0] - self.tgt[0][0]) + abs(o[1] - self.tgt[0][1]))
        self.tgt = (en[:2],) + self.tgt[1:]
        tx, ty, _ = self.spots(en, ranged, snipe)[self.tgt[2]]
        if abs(x - tx) + abs(y - ty) < 20:
            k = 0
            if tx > x + 1:
                k |= KEY.RIGHT
            elif tx < x - 1:
                k |= KEY.LEFT
            if ty > y + 1:
                k |= KEY.DOWN
            elif ty < y - 1:
                k |= KEY.UP
        else:
            k = self.steer(e, s, tx, ty)
            if k is None:
                self.tgt = None
                return 0
        self.mash = 0
        return k

    def herd(self, e, s, g):
        """('herd', cap): an enemy that keeps 60 px away and stands still between 60 and 100 px (Vegeta in the
        gravity chamber, steps away at 2 px/f): line up on its row 70..95 px away, then dash at it (double tap,
        ~3 px/f) and punch; done when an event starts (then as 'events')"""
        if s['scene'] > 0:
            self.goals[self.k] = ('events',)
            return self.mash_a()
        if s['p'] is None:
            return 0
        if self.t > g[1]:
            raise RuntimeError(f'herd not done: {s}')
        ens = self.enemies(e)
        if not ens:
            return 0
        x, y = s['p'][0], s['p'][1]
        ex, ey, fw, fh = ens[0]
        side = -1 if x + 8 < ex + fw // 2 else 1        # -1: we are left of it
        gx = ex - (x + 16) if side < 0 else x - (ex + fw)
        d = 'R' if side < 0 else 'L'
        ph = getattr(self, 'hphase', 0)
        if ph:
            # dashing: tap, release, hold; punch when close
            self.hphase += 1
            if ph < 3:
                return DIRS[d]
            if ph < 5:
                return 0
            if gx <= 12 or ph > 60:
                if ph > 75:
                    self.hphase = 0
                return DIRS[d] | (KEY.A if ph % 4 < 2 else 0)
            return DIRS[d]
        aligned = abs((y + 4) - (ey + fh // 2)) <= 2
        if aligned and 60 <= gx <= 95 and s['p'][2] == self.FACE[d]:
            self.hphase = 1
            return 0
        # get to the row, 75 px from it on the roomier side
        tx = ex - 16 - 75 if (ex - 16 - 75 >= 24) else ex + fw + 75
        ty = ey + fh // 2 - 4
        if abs(x - tx) <= 3 and abs(y - ty) <= 1:
            return DIRS['R' if tx < ex else 'L']
        if abs(x - tx) + abs(y - ty) > 12:
            k = self.steer(e, s, tx, ty)
            if k:
                return k
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
        if (x, y) == getattr(self, 'tlast', None):
            self.tstuck = getattr(self, 'tstuck', 0) + 1
        else:
            self.tstuck = 0
        self.tlast = (x, y)
        if self.t > 3000:
            raise RuntimeError(f'talkto {g} not done: p {s["p"]} npc {n[2:4]}')
        if abs(x - tx) <= 2 and abs(y - ty) <= 2 or self.tstuck > 10 and abs(x - tx) + abs(y - ty) <= 14:
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
