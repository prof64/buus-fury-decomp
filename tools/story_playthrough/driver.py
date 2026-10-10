"""Closed-loop route driver: picks each frame's keys from the game state and a list of goals (deterministic: a pure
function of the emulator state and the goal progress), so the playthrough's input log can be replayed exactly.

Goals:
  ('walk', x, y)        steer the player's footprint top-left to (x, y) px along a BFS path on the area's walk grid
                        (content collision.json); done within 1 px (or when the area changes)
  ('hold', keys, n)     hold keys for n frames
  ('wait', n)           no keys for n frames
  ('talk', dir)         face dir (U/D/L/R) and press A once, then play the event through (A mash) until it ends
  ('events',)           mash A while an event runs (scene depth > 0); done after 20 frames at depth 0
  ('expect', n)         wait up to n frames for an event to start, then as 'events'
  ('area', w, a)        keep the previous direction held until the area becomes (w, a)  (exits)
  ('fight', cap)        punch the nearest enemy (line up 18 px beside it) until the area has none
"""
from grid import Grid, map_of
from route import KEY, status
from common_field import objs

DIRS = {'U': KEY.UP, 'D': KEY.DOWN, 'L': KEY.LEFT, 'R': KEY.RIGHT}
FOOT = (16, 8)


class Driver:
    def __init__(self, goals):
        self.goals = list(goals)
        self.k = 0
        self.t = 0            # frames in the current goal
        self.mash = 0         # A-mash phase
        self.idle = 0         # frames at depth 0 (events goal)
        self.grid = None
        self.grid_key = None
        self.path = None
        self.path_at = -99
        self.last = None
        self.stuck = 0
        self.held = 0

    def done(self):
        return self.k >= len(self.goals)

    def next_goal(self):
        self.k += 1
        self.t = 0
        self.idle = 0
        self.path = None
        self.stuck = 0

    def keys(self, e):
        """this frame's keys"""
        while not self.done():
            k = self.step(e)
            if k is not None:
                self.t += 1
                return k
        return 0

    def mash_a(self):
        self.mash += 1
        return KEY.A if self.mash % 30 < 4 else 0

    def step(self, e):
        g = self.goals[self.k]
        s = status(e)
        kind = g[0]
        # an event that starts on the way (trigger, NPC) is played through first: A mash
        if kind not in ('hold', 'wait', 'talk', 'expect') and s['scene'] > 0:
            return self.mash_a()
        if kind == 'hold':
            if self.t >= g[2]:
                self.next_goal()
                return None
            return g[1]
        if kind == 'wait':
            if self.t >= g[1]:
                self.next_goal()
                return None
            return 0
        if kind == 'expect':
            # wait (up to g[1] frames) for an event to start, then play it through like 'events'
            if s['scene'] > 0:
                self.goals[self.k] = ('events',)
                return self.mash_a()
            if self.t >= g[1]:
                raise RuntimeError(f'expected an event within {g[1]} frames (goal {self.k})')
            return 0
        if kind == 'events':
            if s['scene'] > 0:
                self.idle = 0
                return self.mash_a()
            self.idle += 1
            if self.idle >= 20:
                self.next_goal()
                return None
            return 0
        if kind == 'talk':
            d = DIRS[g[1]]
            if self.t < 2:
                return d
            if self.t < 4:
                return 0
            if self.t < 7:
                return KEY.A
            if s['scene'] > 0:
                self.idle = 0
                return self.mash_a()
            self.idle += 1
            if self.idle >= 20 or self.t > 200 and s['scene'] == 0 and self.idle >= 5:
                self.next_goal()
                return None
            return 0
        if kind == 'area':
            if s['area'][:2] == [g[1], g[2]]:
                self.next_goal()
                return None
            return self.held
        if kind == 'walk':
            return self.walk(e, s, g[1], g[2])
        if kind == 'fight':
            return self.fight(e, s, g[1])
        raise ValueError(g)

    def fight(self, e, s, cap):
        """punch the nearest enemy until the area has none (or cap frames)"""
        ens = [o for o in objs(e) if o[0] == 'enemy' and o[2] is not None]
        if not ens or self.t >= cap or s['p'] is None:
            if self.t >= cap and ens:
                raise RuntimeError(f'fight not over after {cap} frames: {ens}')
            if not ens:
                self.idle += 1
                if self.idle < 30:
                    return 0
            self.next_goal()
            return None
        self.idle = 0
        x, y = s['p'][0], s['p'][1]
        en = min(ens, key=lambda o: abs(o[2] - x) + abs(o[3] - y))
        ex, ey = en[2], en[3]
        side = -1 if x < ex else 1
        tx, ty = ex + side * 18, ey
        k = 0
        if abs(x - tx) > 2 or abs(y - ty) > 2:
            if tx > x + 2:
                k |= KEY.RIGHT
            elif tx < x - 2:
                k |= KEY.LEFT
            if ty > y + 2:
                k |= KEY.DOWN
            elif ty < y - 2:
                k |= KEY.UP
            self.mash = 0
            return k
        # in place: face the enemy, then punch rhythmically
        want = KEY.RIGHT if side < 0 else KEY.LEFT
        facing = s['p'][2]
        if facing != (3 if side < 0 else 2):
            return want
        self.mash += 1
        return KEY.A if self.mash % 10 < 2 else 0

    def walk(self, e, s, tx, ty):
        if s['p'] is None:
            return 0
        x, y = s['p'][0], s['p'][1]
        if abs(x - tx) <= 1 and abs(y - ty) <= 1 or self.stuck > 90:
            self.next_goal()
            return None
        gk = tuple(s['area'])
        if gk != self.grid_key:
            self.grid = Grid(map_of(*gk))
            self.grid_key = gk
            self.path = None
        # the next waypoint: a BFS over 8-px tiles (the footprint's top-left tile; 16 x 8 -> 2 x 1 tiles) around
        # the live NPCs / enemies (their footprints mark the walk grid at runtime)
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
            self.path = self.grid.bfs(start, goal, 2, 1, blocked) or self.grid.bfs(start, goal, 2, 1) or [goal]
            self.path_at = self.t
        here = (x // 8, y // 8)
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
        # stuck (an NPC or an object in the way): sidestep vertically for a while
        if (x, y) == self.last:
            self.stuck += 1
        else:
            self.stuck = 0
        self.last = (x, y)
        if self.stuck > 12 and k:
            # sidestep perpendicular to the blocked direction
            if k & (KEY.UP | KEY.DOWN) and not k & (KEY.LEFT | KEY.RIGHT):
                k = KEY.LEFT if (self.stuck // 24) % 2 == 0 else KEY.RIGHT
            elif k & (KEY.LEFT | KEY.RIGHT) and not k & (KEY.UP | KEY.DOWN):
                k = KEY.DOWN if (self.stuck // 24) % 2 == 0 else KEY.UP
            else:
                k &= KEY.LEFT | KEY.RIGHT
        self.held = k
        return k
