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
_walk = driver.Driver.walk


def _walk_tol(self, e, s, tx, ty):
    # fast walkers (high SPD) step more than 1 px a frame: done within 2 px
    if s['p'] is not None and abs(s['p'][0] - tx) <= 2 and abs(s['p'][1] - ty) <= 2 and self.goals[self.k][0] == 'walk':
        self.next_goal()
        return None
    return _walk(self, e, s, tx, ty)


driver.Driver.walk = _walk_tol


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
        if e.r32(m) == 0x0804B5D8:
            # the power struggle (0xC6): any button pressed adds 10 to the gauge (menu/minigame.rs)
            return 1 if self.t % 2 == 0 else 0
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
        # absent (knock-down) or an event runs (A mash); ends when no enemy is left for 30 frames (or cap);
        # ('boss', cap, 'timer'): ends when the countdown (GameData +1 bit 2, Kid Buu's timed phase) runs
        if len(g) > 2 and g[2] == 'timer':
            # GameData +1 bit 2: the countdown runs (Kid Buu's timed phase)
            if e.r8(0x03001069) & 4:
                self.next_goal()
                return None
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
            # the first exit toward the target that the walk grid can reach from here
            from grid import Grid, map_of
            gr = Grid(map_of(*s['area']))
            pick = ex[0]
            if s['p'] is not None:
                for cand in ex:
                    rr, pu = cand[1], cand[6]
                    dd = {'up': (0, 12), 'down': (0, -12), 'left': (12, 0), 'right': (-12, 0)}[pu]
                    tx0 = max(0, rr['x'] + rr['w'] // 2 - 8 + dd[0])
                    ty0 = max(0, rr['y'] + rr['h'] // 2 - 4 + dd[1])
                    if gr.bfs((s['p'][0] // 8, s['p'][1] // 8), (tx0 // 8, ty0 // 8), 2, 1):
                        pick = cand
                        break
            r, push = pick[1], pick[6]
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
    if kind == 'evade':        # ('evade', cap): keep away from the nearest enemy (Kid Buu's timed phase: Vegeta must
        # last until the Spirit Bomb): each frame the walkable 8-direction step (16 px look-ahead on the walk grid)
        # that ends farthest from it; ends when an event starts
        s = status(e)
        if s['scene'] > 0 and self.t > 5:
            # done once the countdown is over (GameData +1 bit 2 clear); a dialog meanwhile is played through
            if not e.r8(0x03001069) & 4:
                self.next_goal()
                return None
            return self.mash_a()
        if self.t >= g[1]:
            raise RuntimeError('evade: no event')
        if s['p'] is None:
            return 0
        from common_field import objs
        from grid import Grid, map_of
        if getattr(self, '_ev_grid_key', None) != tuple(s['area']):
            self._ev_grid = Grid(map_of(*s['area']))
            self._ev_grid_key = tuple(s['area'])
        gr = self._ev_grid
        ens = [o for o in objs(e) if o[0] == 'enemy' and o[2] is not None]
        if not ens:
            return 0
        x, y = s['p'][0], s['p'][1]
        en = min(ens, key=lambda o: abs(o[2] - x) + abs(o[3] - y))
        best, bk = None, 0
        for kx, ky, key in ((1, 0, 16), (-1, 0, 32), (0, 1, 128), (0, -1, 64), (1, 1, 144), (1, -1, 80), (-1, 1, 160), (-1, -1, 96)):
            nx, ny = x + kx * 16, y + ky * 16
            if not gr.ok_box(nx // 8, ny // 8, 2, 1) or not gr.ok_box((x + kx * 8) // 8, (y + ky * 8) // 8, 2, 1):
                continue
            # farther from the enemy, and keep off the edges (more free tiles around)
            free = sum(gr.ok(nx // 8 + a, ny // 8 + b) for a in (-3, 0, 3) for b in (-3, 0, 3))
            sc = (nx - en[2]) ** 2 + (ny - en[3]) ** 2 + 400 * free
            if best is None or sc > best:
                best, bk = sc, key
        return bk
    if kind == 'itbuu':        # ('itbuu', cap, slot, x, y, type): Super Buu 0x77's phase advance = a hit while he
        # glides, the player in anim slot 0x13 (raw 20: Instant Transmission). Wait at (x, y); when he starts a glide,
        # hold B (IT cursor from the player's centre, v = v*3/4 +- 256 per held direction, 24.8), steer it onto him,
        # release: Goku appears at him and punches. Ends at an event (#474 / #475 / #476).
        if self.t == 0:
            self._it, self._it_cd, self._glide, self._buu_last = None, None, None, None
        s = status(e)
        if s['scene'] > 0:
            self._it = None
            self.next_goal()
            return None
        if self.t >= g[1]:
            raise RuntimeError('itbuu: no event')
        if s['p'] is None:
            return 0
        rec = 0x03001560 + 0x68 * e.r8(0x03001068)
        if e.r8(rec + 0x15) != g[2] and getattr(self, '_it', None) is None:
            return 512 if self.t % 8 < 2 else 0
        from common_field import enemy_detail
        buu = [(d['x'] >> 8, d['y'] >> 8) for d in enemy_detail(e) if d.get('type') == g[5]]
        it = getattr(self, '_it', None)
        import os
        if os.environ.get('DBG_IT'):
            print('IT', self.t, s['p'], buu, it, getattr(self, '_it_cd', None), getattr(self, '_buu_last', None), flush=True)
        if it is not None:
            it['t'] += 1
            if it['phase'] == 'aim':
                if not buu or it['t'] > 90:
                    it['phase'] = 'go'
                    it['t'] = 0
                    return 0
                tx, ty = (buu[0][0] + 16) << 8, (buu[0][1] - 8) << 8
                k = 2
                for ax, (lo, hi) in (('x', (32, 16)), ('y', (64, 128))):
                    c, v, tgt = it[ax], it['v' + ax], (tx if ax == 'x' else ty)
                    # brake when the stopping distance (v * 3) reaches the gap
                    want = 0
                    if tgt - c > 3 * v + 512:
                        want = 1
                    elif tgt - c < 3 * v - 512:
                        want = -1
                    k |= hi if want > 0 else lo if want < 0 else 0
                    v = (v * 3) // 4 + 256 * want
                    it['v' + ax] = v
                    it[ax] = c + v
                if abs(it['x'] - tx) < 4 << 8 and abs(it['y'] - ty) < 4 << 8:
                    it['near'] += 1
                if it['near'] > 3:
                    it['phase'] = 'go'
                    it['t'] = 0
                    return 0
                return k
            # released: the vanish / appear / punch run
            if it['t'] > 90:
                self._it = None
            return 0
        if not buu:
            return 0
        x, y = s['p'][0], s['p'][1]
        # dodge Buu's thrown ball (object vt 0x0804A33C, +0x44 == 1 while flying): run away from it
        W = 0x03001C4C
        for kk in range(min(e.r32(W + 4), 200)):
            o = e.r32(W + 8 + 4 * kk)
            if e.r32(o) == 0x0804A33C and e.r32(o + 0x44) == 1:
                bx = (e.r32(o + 0x28) + e.r32(o + 0x30)) // 2
                by = (e.r32(o + 0x2C) + e.r32(o + 0x34)) // 2
                return (16 if x + 8 >= bx else 32) | (128 if y - 8 >= by else 64)
        if (abs(x - g[3]) > 3 or abs(y - g[4]) > 3) and self.stuck <= 90:
            return _walk(self, e, s, g[3], g[4]) or 0
        last = getattr(self, '_buu_last', None)
        self._buu_last = buu[0]
        cd = getattr(self, '_it_cd', None)
        if cd is not None:
            if cd > 0:
                self._it_cd = cd - 1
                return 0
            # the pick: the world freezes while B is held; the IT cursor starts at the player's body centre
            self._it_cd = None
            self._it = {'phase': 'aim', 't': 0, 'x': (x + 8) << 8, 'y': (y - 8) << 8, 'vx': 0, 'vy': 0, 'near': 0}
            return 2
        gs = getattr(self, '_glide', None)
        if last is not None and last != buu[0]:
            if gs is None:
                self._glide = {'start': buu[0] if last is None else last, 't': 0}
            else:
                gs['t'] += 1
                if gs['t'] == 4:
                    dx, dy = buu[0][0] - gs['start'][0], buu[0][1] - gs['start'][1]
                    # a glide toward the bottom of the screen: Goku appears below Buu and Buu moves into his
                    # punch (tools/superbuu/itsearch.py: releases 27..59 frames after the start work)
                    if dy > 0 and dy >= abs(dx):
                        self._it_cd = (g[6] if len(g) > 6 else 32) - 5
        elif last == buu[0]:
            self._glide = None
        return 0
    if kind == 'stationbeam':  # ('stationbeam', cap, slot, hold, x, y, type): stand at (x, y), face the boss of
        # `type` (left / right) and keep firing the skill in `slot` (B held `hold` frames, short gaps); ends at an event
        s = status(e)
        if s['scene'] > 0:
            self.next_goal()
            return None
        if self.t >= g[1]:
            raise RuntimeError('stationbeam: no event')
        if s['p'] is None:
            return 0
        rec = 0x03001560 + 0x68 * e.r8(0x03001068)
        if e.r8(rec + 0x15) != g[2]:
            return 512 if self.t % 8 < 2 else 0
        hb = getattr(self, '_hold', 0)
        if hb > 0:
            self._hold = hb - 1
            return 2 if hb > 1 else 0
        x, y = s['p'][0], s['p'][1]
        if abs(x - g[4]) > 3 or abs(y - g[5]) > 3:
            k = 0
            if g[4] > x + 2:
                k |= 16
            elif g[4] < x - 2:
                k |= 32
            if g[5] > y + 2:
                k |= 128
            elif g[5] < y - 2:
                k |= 64
            return k
        from common_field import enemy_detail
        buu = [(d['x'] >> 8, d['y'] >> 8) for d in enemy_detail(e) if d.get('type') == g[6]]
        if not buu:
            return 0
        ex = buu[0][0]
        face, key = (3, 16) if ex > x else (2, 32)
        if s['p'][2] != face:
            return key
        self._hold = g[3] + 4
        return 2
    if kind == 'glidebeam':    # ('glidebeam', cap, slot, hold, dist): Super Buu 0x77's phase advance: a hit while he
        # glides between waypoints (GlideToWaypoint on_hit 0x0803C08D) with the player in anim slot 0x13 (the beam
        # skill's firing anim): stay `dist` px from him on his row; fire (hold B) while he moves. Ends at an event.
        s = status(e)
        if s['scene'] > 0:
            self.next_goal()
            return None
        if self.t >= g[1]:
            raise RuntimeError('glidebeam: no event')
        if s['p'] is None:
            return 0
        rec = 0x03001560 + 0x68 * e.r8(0x03001068)
        if e.r8(rec + 0x15) != g[2]:
            return 512 if self.t % 8 < 2 else 0
        hb = getattr(self, '_hold', 0)
        if hb > 0:
            self._hold = hb - 1
            return 2 if hb > 1 else 0
        from common_field import enemy_detail
        buu = [(d['x'] >> 8, d['y'] >> 8) for d in enemy_detail(e) if d.get('type') == 119]
        if not buu:
            return 0
        ex, ey = buu[0]
        last = getattr(self, '_buu_last', None)
        self._buu_last = (ex, ey)
        moving = last is not None and last != (ex, ey)
        x, y = s['p'][0], s['p'][1]
        side = -1 if x < ex else 1
        tx, ty = ex + side * g[4], ey
        if abs(y - ty) > 6 or abs(x - tx) > 40:
            k = 0
            if tx > x + 2:
                k |= 16
            elif tx < x - 2:
                k |= 32
            if ty > y + 2:
                k |= 128
            elif ty < y - 2:
                k |= 64
            return k
        if s['p'][2] != (3 if side < 0 else 2):
            return 16 if side < 0 else 32
        if moving:
            self._hold = g[3] + 4
            return 2
        return 0
    if kind == 'ballbeam':     # ('ballbeam', cap, slot, hold, dist): Super Buu 0x77: while his ball charges (object
        # vt 0x0804A33C, +0x44 == 0) line up on his row `dist` px away and fire the skill in `slot` (hold B); while
        # the ball flies (+0x44 == 1) run away from it; ends when an event starts (#474 / #475 / #476)
        s = status(e)
        if s['scene'] > 0:
            self.next_goal()
            return None
        if self.t >= g[1]:
            raise RuntimeError('ballbeam: no event')
        if s['p'] is None:
            return 0
        rec = 0x03001560 + 0x68 * e.r8(0x03001068)
        if e.r8(rec + 0x15) != g[2]:
            return 512 if self.t % 8 < 2 else 0
        hb = getattr(self, '_hold', 0)
        if hb > 0:
            self._hold = hb - 1
            return 2 if hb > 1 else 0
        x, y = s['p'][0], s['p'][1]
        W = 0x03001C4C
        ball = None
        for i in range(min(e.r32(W + 4), 200)):
            o = e.r32(W + 8 + 4 * i)
            if e.r32(o) == 0x0804A33C:
                bx = (e.r32(o + 0x28) + e.r32(o + 0x30)) // 2
                by = (e.r32(o + 0x2C) + e.r32(o + 0x34)) // 2
                ball = (bx, by, e.r32(o + 0x44))
        from common_field import enemy_detail
        buu = [(d['x'] >> 8, d['y'] >> 8) for d in enemy_detail(e) if d.get('type') == 119]
        if not buu:
            return 0
        ex, ey = buu[0]
        if ball and ball[2] == 1 and len(g) > 5 and g[5] == 'flee':
            # flee: away from the ball's point
            k = 0
            k |= 16 if x + 8 >= ball[0] else 32
            k |= 128 if y - 8 >= ball[1] else 64
            return k
        side = -1 if x < ex else 1
        tx, ty = ex + side * g[4], ey
        if abs(y - ty) > 3 or abs(x - tx) > 30:
            k = 0
            if tx > x + 2:
                k |= 16
            elif tx < x - 2:
                k |= 32
            if ty > y + 2:
                k |= 128
            elif ty < y - 2:
                k |= 64
            return k
        if s['p'][2] != (3 if side < 0 else 2):
            return 16 if side < 0 else 32
        # fire when the ball state matches the mode: 'launch' = the ball flies (+0x44 == 1), else while it exists
        launch = len(g) > 6 and g[6] == 'launch'
        if ball and (not launch or ball[2] == 1) and getattr(self, '_fired_for', None) != id(ball) // 1 * 0 + self.t // 400:
            self._hold = g[3] + 4
            return 2
        return 0
    if kind == 'bat':          # ('bat', cap): Super Buu's big ball (object vt 0x0804A33C, box +0x28..+0x34): face it
        # and punch it back as it arrives; ends when an event starts (the phase's cutscene #474 / #475 / #476)
        s = status(e)
        if s['scene'] > 0:
            self.next_goal()
            return None
        if self.t >= g[1]:
            raise RuntimeError('bat: no event')
        if s['p'] is None:
            return 0
        x, y = s['p'][0], s['p'][1]
        W = 0x03001C4C
        ball = None
        for i in range(min(e.r32(W + 4), 200)):
            o = e.r32(W + 8 + 4 * i)
            if e.r32(o) == 0x0804A33C and e.r32(o + 0x44) == 1:
                bx = (e.r32(o + 0x28) + e.r32(o + 0x30)) // 2
                by = (e.r32(o + 0x2C) + e.r32(o + 0x34)) // 2
                ball = (bx, by)
        if ball is None:
            self._bat_cool = 0
            return 0
        cx, cy = x + 8, y - 8
        dx, dy = ball[0] - cx, ball[1] - cy
        if abs(dx) >= abs(dy):
            face, key = (3, 16) if dx > 0 else (2, 32)
        else:
            face, key = (0, 128) if dy > 0 else (1, 64)
        if s['p'][2] != face:
            return key
        cool = getattr(self, '_bat_cool', 0)
        if cool > 0:
            self._bat_cool = cool - 1
            return 0
        thr = g[2] if len(g) > 2 else 30
        if abs(dx) + abs(dy) < thr:
            self._bat_cool = 20
            return 1
        return 0
    if kind == 'beam':         # ('beam', cap, skill_slot, hold, dist): select skill slot (record +0x15) with L, then
        # line up `dist` px from the nearest enemy on its row, facing it, hold B `hold` frames, release; repeat
        s = status(e)
        if s['scene'] > 0:
            return self.mash_a()
        rec = 0x03001560 + 0x68 * e.r8(0x03001068)
        if e.r8(rec + 0x15) != g[2]:
            return 512 if self.t % 8 < 2 else 0
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
            raise RuntimeError(f'beam not over after {g[1]} frames')
        if s['p'] is None:
            return 0
        hb = getattr(self, '_hold', 0)
        if hb > 0:
            self._hold = hb - 1
            return 2 if hb > 1 else 0
        x, y = s['p'][0], s['p'][1]
        if len(g) > 5:
            # only enemies of type g[5] (the boss; story NPC-enemies like Vegeta 137 are left alone)
            from common_field import enemy_detail
            want = [(d['x'] >> 8, d['y'] >> 8) for d in enemy_detail(e) if d.get('type') == g[5]]
            ens = [o for o in ens if (o[2], o[3]) in want] or ens
        en = min(ens, key=lambda o: abs(o[2] - x) + abs(o[3] - y))
        ex, ey = en[2], en[3]
        side = -1 if x < ex else 1
        tx, ty = ex + side * g[4], ey
        if abs(y - ty) > 3 or abs(x - tx) > 30:
            k = 0
            if tx > x + 2:
                k |= 16
            elif tx < x - 2:
                k |= 32
            if ty > y + 2:
                k |= 128
            elif ty < y - 2:
                k |= 64
            return k
        if s['p'][2] != (3 if side < 0 else 2):
            return 16 if side < 0 else 32
        self._hold = g[3] + 6
        return 2
    if kind in ('zap', 'zapx'):  # ('zap', cap, dist): like 'boss' but keeps `dist` px away on the enemy's row and fires
        # the selected skill (B) while lined up; 'zapx' ends at the first event instead of when no enemy is left
        s = status(e)
        if s['scene'] > 0:
            if kind == 'zapx':
                self.next_goal()
                return None
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
            raise RuntimeError(f'zap not over after {g[1]} frames')
        if s['p'] is None:
            return 0
        x, y = s['p'][0], s['p'][1]
        en = min(ens, key=lambda o: abs(o[2] - x) + abs(o[3] - y))
        ex, ey = en[2], en[3]
        dist = g[2] if len(g) > 2 else 64
        side = -1 if x < ex else 1
        tx, ty = ex + side * dist, ey
        k = 0
        if abs(y - ty) > 3 or abs(x - tx) > 24:
            if tx > x + 2:
                k |= 16
            elif tx < x - 2:
                k |= 32
            if ty > y + 2:
                k |= 128
            elif ty < y - 2:
                k |= 64
            return k
        want = 16 if side < 0 else 32
        if s['p'][2] != (3 if side < 0 else 2):
            return want
        self.mash += 1
        return 2 if self.mash % 12 < 2 else 0
    if kind == 'mash':
        if self.t >= g[1]:
            self.next_goal()
            return None
        return self.mash_a()
    return _step(self, e)


driver.Driver.step = step
