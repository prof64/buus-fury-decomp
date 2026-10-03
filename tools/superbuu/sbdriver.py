"""Extra driver goals (monkeypatched into tools/story_playthrough/driver.py's Driver):
  ('walke', x, y)     walk like 'walk', but the goal ends as soon as an event starts (follow with ('events',))
  ('areaw', w, a)     wait (no keys) until the area is (w, a) and the player exists
  ('mash', n)         A mash for n frames whatever happens
"""
import sys
from pathlib import Path
sys.path.insert(0, str(Path(__file__).resolve().parent.parent.parent / 'tools' / 'story_playthrough'))
import driver  # noqa: E402
from route import status  # noqa: E402

_step = driver.Driver.step
_objs = driver.objs
AVOID = []   # fake obstacles (exit rects the walk must not cross): [(x, y)] px


def _objs_avoid(e):
    return _objs(e) + [['npc', 'AVOID', x, y, 0] for x, y in AVOID]


driver.objs = _objs_avoid


def avoid_exits(here, keep):
    """block the area's exit rects except the one to `keep` (fake NPC footprints every 8 px)"""
    import mapgraph
    G, _ = mapgraph.load_cached()
    AVOID.clear()
    for x in G.get(here, []):
        if x[0] == keep:
            continue
        r = x[1]
        for yy in range(r['y'] - 8, r['y'] + r['h'] + 8, 8):
            for xx in range(r['x'] - 16, r['x'] + r['w'] + 8, 8):
                AVOID.append((xx, yy))
_cat = None


def node_addr(ev, node):
    global _cat
    if _cat is None:
        import json
        c = json.load(open(Path(__file__).resolve().parent.parent.parent / 'engine' / 'annotations' / 'script_catalog.json'))
        _cat = {s['id']: s for s in c['scripts']}
    for n in _cat[ev]['nodes']:
        if n['index'] == node:
            return int(n['addr'], 16)
    raise KeyError((ev, node))


def step(self, e):
    g = self.goals[self.k]
    kind = g[0]
    if kind == 'mark':
        cb = getattr(self, 'on_mark', None)
        if cb:
            cb(e, g[1])
        self.next_goal()
        return None
    if kind == 'walke':
        s = status(e)
        if s['scene'] > 0:
            self.next_goal()
            return None
        return self.walk(e, s, g[1], g[2])
    if kind == 'areaw':
        s = status(e)
        if s['area'][:2] == [g[1], g[2]] and s['p'] is not None and s['scene'] == 0:
            self.next_goal()
            return None
        return self.mash_a() if s['scene'] > 0 else 0
    if kind == 'fastmash':     # A every other frame (button-mash mini-games)
        if self.t >= g[1]:
            self.next_goal()
            return None
        return 1 if self.t % 2 == 0 else 0
    if kind == 'prog':         # explicit key program (route.program_keys syntax)
        if not hasattr(self, '_prog') or self._prog_k != self.k:
            from route import program_keys
            self._prog, self._prog_k = program_keys(g[1]), self.k
        if self.t >= len(self._prog):
            self.next_goal()
            return None
        return self._prog[self.t]
    if kind == 'node':         # ('node', event, node, delay, keys, n): A-mash events until the scene starts that
        # node (breakpoint on the scene step 0x08012538, r1 = node), then no keys for `delay` frames, then keys n frames
        if self.t == 0:
            self._bp = e.breakpoint(0x08012538)
            self._at = None
            e.hits()
        if self._at is None:
            want = node_addr(g[1], g[2])
            for h in e.hits():
                if h['kind'] == 'break' and h['regs'][1] == want:
                    self._at = self.t
            if self._at is None:
                return self.mash_a()
        d = self.t - self._at
        if d < g[3]:
            return 0
        if d < g[3] + g[5]:
            return g[4]
        e.clear(self._bp)
        self.next_goal()
        return None
    if kind == 'eventsw':      # like 'events', and plays Watch This! (0xA7) when its mode runs: wait g[1] frames
        # after the mode starts (the icons are shown), then press the drawn sequence (2 frames each, 6 apart)
        m = e.r32(0x03000F90)
        if e.r32(m) == 0x0804A3E8:
            gobj = e.r32(m + 4)
            if getattr(self, '_w_at', None) is None:
                self._w_at = self.t
                self._w_seq = None
            d = self.t - self._w_at
            if d < g[1]:
                return 0
            if self._w_seq is None:
                self._w_seq = [e.r8(gobj + 0x24 + k) for k in range(5)]
            i, r = divmod(d - g[1], 8)
            if i < 5 and r < 2:
                return [1, 2, 16, 32, 64, 128][self._w_seq[i]]
            return 0
        self._w_at = None
        s = status(e)
        if s['scene'] > 0:
            self.idle = 0
            return self.mash_a()
        self.idle += 1
        if self.idle >= 20:
            self.next_goal()
            return None
        return 0
    if kind == 'boss':         # ('boss', cap): like 'fight' but robust: keeps going while the player object is briefly
        # absent (knock-down) or an event runs (A mash); ends when no enemy is left for 30 frames (or cap)
        s = status(e)
        if s['scene'] > 0:
            return self.mash_a()
        from common_field import objs
        ens = [o for o in objs(e) if o[0] == 'enemy' and o[2] is not None]
        if not ens:
            self.idle += 1
            if self.idle >= 30 or self.t >= g[1]:
                self.next_goal()
                return None
            return 0
        self.idle = 0
        if self.t >= g[1]:
            raise RuntimeError(f'boss not over after {g[1]} frames')
        if s['p'] is None:
            return 0
        return self.fight(e, s, 10 ** 9)
    if kind == 'fly':          # ('fly', x, y): world map 0-50 flight to (x, y) px, then A (descend) until the area
        # changes (the landing spot's altitude trigger). Steers by heading (22.10, 0 = north, clockwise).
        s = status(e)
        if s['scene'] > 0:
            self.next_goal()
            return None
        if s['area'][:2] != [0, 50]:
            if self.t > 0 and getattr(self, '_fly_started', False):
                self._fly_started = False
                self.next_goal()
                return None
            return 0
        self._fly_started = True
        import math
        p = e.r32(0x03001C34)
        if not p:
            return 0
        s32 = lambda v: v - (1 << 32) if v & 0x80000000 else v
        x, y = s32(e.r32(p + 0x1A0)) / 256, s32(e.r32(p + 0x1A4)) / 256
        h = (s32(e.r32(p + 0x1AC)) >> 10) & 255
        W, H = 3584, 2560
        dx = (g[1] - x + W / 2) % W - W / 2
        dy = (g[2] - y + H / 2) % H - H / 2
        dist = math.hypot(dx, dy)
        if dist < 6:
            return 1          # A: descend / land
        want = int(round(math.atan2(dx, -dy) / (2 * math.pi) * 256)) & 255
        diff = (want - h + 128) % 256 - 128
        k = 0
        if diff > 3:
            k |= 16
        elif diff < -3:
            k |= 32
        if abs(diff) < 24 and dist > 40:
            k |= 64
        elif dist <= 40 and abs(diff) < 24:
            k |= 64 if self.t % 3 == 0 else 0
        return k
    if kind == 'exitto':       # ('exitto', 'W-A'): walk to the current area's exit toward W-A and push through
        s = status(e)
        if s['scene'] > 0:
            return self.mash_a()
        here = f"{s['area'][0]}-{s['area'][1]}"
        if self.t == 0 or not hasattr(self, '_ex'):
            self._ex = None
        if self._ex and self._ex[0] != here:
            # arrived
            if s['area'][:2] == [int(v) for v in g[1].split('-')]:
                if s['p'] is not None:
                    self._ex = None
                    AVOID.clear()
                    self.next_goal()
                    return None
                return 0
            if s['p'] is None:
                return 0
            self._ex = None   # pushed elsewhere: plan again from here
        if self._ex is None:
            import mapgraph
            G, _ = mapgraph.load_cached()
            ex = [x for x in G[here] if x[0] == g[1]]
            if not ex:
                raise RuntimeError(f'no exit {here} -> {g[1]}')
            r, push = ex[0][1], ex[0][6]
            avoid_exits(here, g[1])
            cx, cy = r['x'] + r['w'] // 2 - 8, r['y'] + r['h'] // 2 - 4
            d = {'up': (0, 12), 'down': (0, -12), 'left': (12, 0), 'right': (-12, 0)}[push]
            self._ex = (here, (max(0, cx + d[0]), max(0, cy + d[1])), {'up': 64, 'down': 128, 'left': 32, 'right': 16}[push], 0)
        here0, (tx, ty), key, phase = self._ex
        if s['p'] is None:
            return key
        if phase == 0:
            x, y = s['p'][0], s['p'][1]
            if abs(x - tx) <= 1 and abs(y - ty) <= 1 or self.stuck > 90:
                self._ex = (here0, (tx, ty), key, 1)
                self.stuck = 0
                return key
            return self.walk(e, s, tx, ty) if self.k == self.k else 0
        return key
    if kind == 'mash':
        if self.t >= g[1]:
            self.next_goal()
            return None
        return self.mash_a()
    return _step(self, e)


driver.Driver.step = step
