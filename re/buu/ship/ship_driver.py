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


LEVELS = None


def level_of(e, c=None):
    """the level of character c (default: the active one) from its XP (data/buu/levels.json)"""
    global LEVELS
    if LEVELS is None:
        import json
        LEVELS = [x['total_xp'] for x in json.load(open(ROOT / 'data/buu/levels.json'))['levels']]
    if c is None:
        c = e.r8(0x03001068)
    xp = e.r32(0x03001560 + 0x68 * c + 0x24)
    return 50 + max(i for i, t in enumerate(LEVELS) if xp >= t)


def enemies(e):
    """[(obj, x px, y px, facing, hp, type)] from the enemy list"""
    out = []
    n = e.r32(ENEMY_LIST)
    for k in range(min(n, 64)):
        o = e.r32(ENEMY_LIST + 4 + 4 * k)
        sv = lambda v: v - (1 << 32) if v & 0x80000000 else v  # noqa: E731
        out.append((o, sv(e.r32(o + 0x1A0)) >> 8, sv(e.r32(o + 0x1A4)) >> 8, e.r8(o + 0x10), sv(e.r32(o + 0x28C)),
                    (e.r32(o + 0x224) - 0x08711274) // 28))
    return out


PICKUP_VT = (0x08049CC4, 0x08049C98)    # coin, dropped item (supply / equipment)


def pickups(e):
    """[(vt, box x0, y0, x1, y1)] of the pickups lying in the object world (0x03001C4C)"""
    out = []
    n = e.r32(0x03001C4C + 4)
    sv = lambda v: v - (1 << 32) if v & 0x80000000 else v  # noqa: E731
    for k in range(min(n, 255)):
        o = e.r32(0x03001C4C + 8 + 4 * k)
        vt = e.r32(o)
        if vt in PICKUP_VT:
            out.append((vt, sv(e.r32(o + 0x14)), sv(e.r32(o + 0x18)), sv(e.r32(o + 0x1C)), sv(e.r32(o + 0x20))))
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
        self.tgt = {}
        self.prog = []
        self.heal = None
        self.heal_cool = 0
        self.nop = 0
        self.on_mark = None
        self.grind_k = None
        self.in_nav = False
        self.fight_done = False
        self.nav_area = None
        self.nav_t = 0
        self.push_t = None

    # not worth fighting in passing: Destroyer (3762 HP, flames), Laser Turret (in wall niches); Babidi (4 / 29: beside
    # the boss in 8-71, not a target)
    AVOID = (20, 64, 4, 29)
    HEALS = (21, 9, 8, 7, 6, 5, 4, 3, 2, 1, 0)  # the HP ones: Senzu, then food (10 .. 19 are drinks: EP only)
    FLAMERS = (20,)             # Destroyer: FlameBreath (state 0x86) along its facing once the player is within 88 px

    THROWS = (0x156, 0x1E2, 0x1E8)   # Majin Soldier / Shield Soldier: AlignWithPlayer + SoldierThrow (an energy wave)

    def block_guard(self, e, s):
        """a soldier lined up for its wave throw: face it and hold R (a block from the front takes 1/16 damage) until
        40 frames after the throw state; None otherwise"""
        if s['p'] is None:
            return None
        x, y, pf = s['p']
        for o in enemies(e):
            st = ai_state(e, o[0])
            if not st or st[-1] not in self.THROWS:
                continue
            dx, dy = o[1] - x, o[2] - y
            # close by, closing in and punching beats waiting behind the guard
            if abs(dy) <= 16 and 56 < abs(dx) < 170:
                want = 3 if dx > 0 else 2
            elif abs(dx) <= 16 and 56 < abs(dy) < 170:
                want = 0 if dy > 0 else 1
            else:
                continue
            self.block = (self.t + 40, want)
            break
        if getattr(self, 'block', None) is None or self.t >= self.block[0] or self.block[0] - self.t > 40:
            self.block = None
            return None
        want = self.block[1]
        if pf != want and not self.blocking(e):
            return (KEY.DOWN, KEY.UP, KEY.LEFT, KEY.RIGHT)[want]
        return KEY.R

    def blocking(self, e):
        p = e.r32(0x03001C34)
        return p != 0 and e.r8(p + 0x1C8) != 0

    def flame_guard(self, e, s):
        """a Destroyer breathing fire (0x86, 124 frames along its facing) with the player in front of it (the flames
        reach ~100 px): step out of the line (perpendicular, to a side free of walls and enemies), else run from it
        when close; None otherwise"""
        if s['p'] is None or self.grid is None:
            return None
        x, y = s['p'][0], s['p'][1]
        ens = enemies(e)
        for o in ens:
            if o[5] not in self.FLAMERS:
                continue
            st = ai_state(e, o[0])
            if not st or st[-1] != 0x86:
                continue
            ex, ey, f = o[1], o[2], o[3]
            if f >= 2:
                ahead, side = (x - ex) * (1 if f == 3 else -1), y - ey
            else:
                ahead, side = (y - ey) * (1 if f == 0 else -1), x - ex
            if not (-16 < ahead < 260 and abs(side) < 44):
                continue

            def free(px, py):
                return self.grid.ok_box(px // 8, py // 8) and not any(
                    q[0] != o[0] and abs(q[1] - px) < 16 and abs(q[2] - py) < 12 for q in ens)
            # a direction that did not move the player (a wall the grid missed) is out for a while
            dead = getattr(self, 'guard_dead', {})
            need = 44 - abs(side) + 8
            for sd in ((1, -1) if side >= 0 else (-1, 1)):
                ddx, ddy = (0, sd) if f >= 2 else (sd, 0)
                k = (KEY.DOWN if sd > 0 else KEY.UP) if f >= 2 else (KEY.RIGHT if sd > 0 else KEY.LEFT)
                if dead.get(k, -1) > self.t:
                    continue
                if all(free(x + ddx * j, y + ddy * j) for j in range(8, need + 8, 8)):
                    return self.guard_key(k, x, y)
            k = self.flee(e, s, ex, ey)
            return self.guard_key(k, x, y) if k else None
        return None

    def guard_key(self, k, x, y):
        """remember the guard's key; held 8 frames without moving: that direction is blocked for 90 frames"""
        last = getattr(self, 'guard_last', None)
        if last and last[0] == k and last[1] == (x, y):
            n = last[2] + 1
            if n >= 8:
                self.guard_dead = {**getattr(self, 'guard_dead', {}), k: self.t + 90}
        else:
            n = 0
        self.guard_last = (k, (x, y), n)
        return k

    def keys(self, e):
        # a pause menu left open (a menu program that went wrong): B out of it
        if not self.prog and self.heal is None:
            st = status(e)
            if st['p'] is None and st['scene'] == 0 and e.r32(0x03001C34) == 0:
                self.nop += 1
            else:
                self.nop = 0
            if self.nop > 240 and self.goals[self.k][0] in ('nav', 'fightp', 'yakon') if not self.done() else False:
                return KEY.B if self.nop % 20 < 3 else 0
        if self.heal is not None:
            k = self.heal_step(e)
            if k is not None:
                return k
        if self.prog:
            return self.prog.pop(0)
        return super().keys(e)

    def heal_step(self, e):
        """the healing menu, opened only when START took (a hurt / grabbed player cannot pause)"""
        h = self.heal
        h['t'] += 1
        if h['phase'] == 'open':
            if h['t'] <= 2:
                return KEY.START
            if status(e)['p'] is None:          # the pause menu is up
                h['phase'] = 'menu'
                prog = [(0, 0, 70), (KEY.R, 3, 60)] + [(KEY.DOWN, 3, 20)] * h['row'] + \
                    [(KEY.A, 3, 40), (KEY.A, 3, 80), (KEY.B, 3, 60)]
                self.prog = []
                for m, on, off in prog:
                    self.prog += [m] * on + [0] * off
                self.heal = None
                return self.prog.pop(0)
            if h['t'] > 40:
                self.heal = None
                self.heal_cool = 20
            return 0
        self.heal = None
        return None

    def maybe_heal(self, e, s, frac=0.35):
        """below `frac` of max HP in the field: the pause menu's Inventory, use the best healing supply
        (START, R to Inventory, DOWN to its row, A, A Use, B back to the field)"""
        if self.heal_cool > 0:
            self.heal_cool -= 1
            return False
        if self.prog or self.heal is not None or s['p'] is None or s['scene'] > 0:
            return False
        rec = 0x03001560 + 0x68 * e.r8(0x03001068)
        hp, mx = e.r16(rec + 0x16), e.r16(rec + 0x18)
        if hp == 0 or hp * 100 >= mx * int(frac * 100):
            return False
        sup = e.read(0x03001068 + 2, 25)
        for it in self.HEALS:
            if sup[it]:
                self.heal = {'phase': 'open', 't': 0, 'row': sum(1 for i in range(it) if sup[i])}
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
                return self.heal_step(e) or 0
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
        if kind == 'nav':
            return self.nav(e, s, g)
        if kind == 'grind':
            # ('grind', level, [(w, a), ...]): walk the areas in a cycle (nav with room fights; reloading an area
            # brings its enemies back) until the active character reaches `level`
            if s['p'] is not None and s['scene'] == 0 and level_of(e) >= g[1]:
                self.next_goal()
                return None
            route = g[2]
            if self.grind_k is None:
                self.grind_k = 0
            w, a = route[self.grind_k % len(route)]
            if s['area'][:2] == [w, a] and s['p'] is not None and self.nav_area is None:
                # spend the level-ups' stat points between rooms (Status page: half STR, half END)
                c = e.r8(0x03001068)
                pts = e.r16(0x03001560 + 0x68 * c + 0x2C)
                if pts >= 6 and not enemies(e):
                    for m, on, off in alloc_keys([(0, [pts - pts // 2, 0, pts // 2, 0])])[1]:
                        self.prog += [m] * on + [0] * off
                    return self.prog.pop(0)
                self.grind_k += 1
                w, a = route[self.grind_k % len(route)]
            return self.nav(e, s, ('nav', w, a, True), inner=True)
        if kind == 'hunt':
            return self.hunt(e, s, g)
        if kind == 'rest':
            # ('rest', c): stand still until character c (in reserve: +5% HP / EP every ~596 frames) is full again
            r = 0x03001560 + 0x68 * g[1]
            if e.r16(r + 0x16) >= e.r16(r + 0x18) and e.r16(r + 0x1A) >= e.r16(r + 0x1C):
                self.next_goal()
                return None
            return 0
        if kind == 'story':
            # ('story', w, a[, boss]): play events through (A mash) and fight whatever enemies the story puts in front
            # of the player, until area w-a is reached with the player controllable and no event for 20 frames
            # (boss: never skip an enemy that takes no damage)
            if s['area'][:2] == [g[1], g[2]] and s['scene'] == 0 and s['p'] is not None and not enemies(e):
                self.idle += 1
                if self.idle >= 20:
                    self.next_goal()
                    return None
                return 0
            if s['scene'] > 0 or s['p'] is None:
                self.idle = 0
                return self.mash_a()
            if enemies(e):
                self.idle = 0
                if any(o[5] in self.BUU for o in enemies(e)):
                    return self.buu(e, s)
                self.in_nav = True
                self.fight_done = False
                k = self.fightp(e, s, ('fightp', 10 ** 9, 99999, 6, len(g) > 3 and g[3]))
                self.in_nav = False
                return k
            self.idle = 0
            return self.mash_a()
        if kind == 'fly':
            return self.fly(e, s, g)
        if kind == 'mark':
            if self.on_mark:
                self.on_mark(g[1], e)
            self.next_goal()
            return None
        return super().step(e)

    def fly(self, e, s, g):
        """('fly', x, y, w, h): world map 0-50 (as tools/story_playthrough/earth_driver.py): climb above the
        encounters (z ~110 px), steer (LEFT / RIGHT turn, UP thrust) to the rect's centre, then descend (A) inside it
        until the landing script warps; done once controllable elsewhere"""
        import math
        x0, y0, rw, rh = g[1], g[2], g[3], g[4]
        if s['area'][:2] != [0, 50]:
            if s['scene'] > 0 or s['p'] is None:
                self.idle = 0
                return self.mash_a()
            self.next_goal()
            return None
        p = e.r32(0x03001C34)
        if not p or s['scene'] > 0:
            return self.mash_a() if s['scene'] > 0 else 0
        sv = lambda v: v - (1 << 32) if v & 0x80000000 else v  # noqa: E731
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

    def hunt(self, e, s, g):
        """('hunt', equipment_id[, n]): world map encounters until the equipment list holds `n` (default 1) of the
        item: on 0-50 fly low into the nearest patrolling encounter (type 141), in the encounter arena 0-55 (Stolen
        Airship) fight everything, then A on its exit pad (345, 217) back to the map (on-defeat 0x08073EFC: 25% a
        Bandana once flag 76 is set)"""
        import math
        want = g[2] if len(g) > 2 else 1
        n = e.r16(0x03001068 + 0xE4)
        have = sum(1 for i in range(min(n, 250)) if e.r32(0x03001068 + 0xE8 + 4 * i) & 0xFFFF == g[1])
        if have >= want and s['area'][:2] == [0, 50]:
            self.next_goal()
            return None
        if s['scene'] > 0 or s['p'] is None:
            return self.mash_a()
        if s['area'][0] == 0 and 51 <= s['area'][1] <= 59:
            if enemies(e):
                self.in_nav = True
                self.fight_done = False
                k = self.fightp(e, s, ('fightp', 10 ** 9, 99999, 6))
                self.in_nav = False
                return k
            x, y = s['p'][0], s['p'][1]
            # the loot first (a dropped Bandana lies where its bandit fell)
            loot = [q for q in pickups(e) if q[0] == 0x08049C98]
            if loot and self.t % 2000 < 1500:
                q = min(loot, key=lambda q: abs(q[1] - x) + abs(q[2] - y))
                return self.walk_keys(e, s, (q[1] + q[3]) // 2 - 8, (q[2] + q[4]) // 2 - 4)
            r = self.map_pad(s)
            px, py = r['x'] + r['w'] // 2 - 8, r['y'] + r['h'] - 10
            if abs(x - px) > 8 or abs(y - py) > 8:
                return self.walk_keys(e, s, px, py)
            self.mash += 1
            return KEY.A if self.mash % 20 < 2 else 0
        if s['area'][:2] != [0, 50]:
            return self.mash_a()
        p = e.r32(0x03001C34)
        if not p:
            return 0
        sv = lambda v: v - (1 << 32) if v & 0x80000000 else v  # noqa: E731
        x, y = sv(e.r32(p + 0x1A0)) / 256, sv(e.r32(p + 0x1A4)) / 256
        z, h = sv(e.r32(p + 0x1A8)) / 1024, sv(e.r32(p + 0x1AC)) / 1024
        pats = [o for o in enemies(e) if o[5] == 141 and not self.over_pad(o[1], o[2])]
        if not pats:
            return KEY.UP
        o = min(pats, key=lambda o: math.hypot(o[1] - x, o[2] - y))
        dx, dy = o[1] - x, o[2] - y
        wantd = math.degrees(math.atan2(dx, -dy)) * 256 / 360
        diff = (wantd - h + 128) % 256 - 128
        k = 0
        if diff > 3:
            k |= KEY.RIGHT
        elif diff < -3:
            k |= KEY.LEFT
        if abs(diff) < 40:
            k |= KEY.UP
        # cruise high (no landing pad takes the player), drop onto the encounter
        dist = math.hypot(dx, dy)
        if dist > 90 and z < 100:
            k |= KEY.B
        elif dist <= 90 and z > 2 and not self.over_pad(x, y):
            k |= KEY.A
        return k

    PADS = None

    def over_pad(self, x, y):
        """near one of the world map's landing rects (0-50 script triggers warping into an area)"""
        import nav as navm
        import json
        if ShipDriver.PADS is None:
            o = json.loads((navm.CONTENT / 'maps' / 'area_00_050' / 'objects.json').read_text())
            ShipDriver.PADS = [t['rect'] for ar in o['areas'] for t in ar['objects'] if t.get('warps')]
        return any(r['x'] - 48 <= x < r['x'] + r['w'] + 48 and r['y'] - 48 <= y < r['y'] + r['h'] + 48
                   for r in ShipDriver.PADS)

    def map_pad(self, s):
        """the current area's pad back to the world map (a script trigger warping to 0-50)"""
        import nav as navm
        import json
        key = f"{s['area'][0]}-{s['area'][1]}"
        a = navm.areas()[key]
        m = a['maps'][0]
        o = json.loads((navm.CONTENT / 'maps' / m / 'objects.json').read_text())
        for ar in o['areas']:
            for t in ar['objects']:
                if any(w.get('to') == '0-50' for w in t.get('warps') or []):
                    return t['rect']
        raise RuntimeError(f'no map pad in {key}')

    def nav(self, e, s, g, inner=False):
        """('nav', w, a[, fight]): walk through the exits (nav.route) to area w-a; with fight, clear each room's
        enemies (fightp) before leaving it. Done 20 frames after arriving with the player present."""
        import nav as navm
        if s['scene'] > 0:
            return self.mash_a()
        if s['p'] is None:
            return 0
        cur = f"{s['area'][0]}-{s['area'][1]}"
        if cur != self.nav_area:
            self.nav_area = cur
            self.skip = set()
            self.tgt = {}
            self.fight_done = False
            self.path = None
            self.idle = 0
            self.nav_t = self.t
            self.push_t = None
        if cur == f'{g[1]}-{g[2]}':
            # arrived: clear this room too (a grind's target room), then done 20 frames later
            if len(g) > 3 and g[3] and not self.fight_done:
                self.in_nav = True
                k = self.fightp(e, s, ('fightp', 10 ** 9, 99999, 6))
                if not self.fight_done:
                    return k
            if self.t - self.nav_t >= 20:
                self.in_nav = False
                self.nav_area = None
                if inner:
                    return 0
                self.next_goal()
                return None
            return 0
        if self.maybe_heal(e, s):
            return self.heal_step(e) or 0
        if len(g) > 3 and g[3] and not self.fight_done:
            self.in_nav = True
            k = self.fightp(e, s, ('fightp', 10 ** 9, 99999, 6))
            if not self.fight_done:
                return k
        hops = navm.route(cur, f'{g[1]}-{g[2]}')
        if not hops:
            raise RuntimeError(f'nav: no route {cur} -> {g[1]}-{g[2]}')
        to, rect, push = hops[0][1]
        # several exits lead there (2-11's two doors down to 2-10): take one the player can walk to
        alts = [x for x in navm.exits(cur) if x[0] == to]
        if len(alts) > 1:
            self.reachable(e, s, 0, 0)
            for x in alts:
                ax, ay = navm.approach(x[1], x[2])
                if self.reachable(e, s, ax, ay):
                    to, rect, push = x
                    break
        tx, ty = navm.approach(rect, push)
        x, y = s['p'][0], s['p'][1]
        # pushing into an exit is sticky (an exit fires after the direction is held for a while)
        if self.push_t is None or self.t - self.push_t > 150:
            if abs(x - tx) > 8 or abs(y - ty) > 8:
                self.push_t = None
                return self.walk_keys(e, s, tx, ty)
            if self.push_t is None:
                self.push_t = self.t
        k = {'up': KEY.UP, 'down': KEY.DOWN, 'left': KEY.LEFT, 'right': KEY.RIGHT}[push]
        # keep lined up with the exit's centre while pushing
        if push in ('up', 'down') and abs(x - tx) > 2:
            k |= KEY.RIGHT if tx > x else KEY.LEFT
        if push in ('left', 'right') and abs(y - ty) > 2:
            k |= KEY.DOWN if ty > y else KEY.UP
        return k

    # --- live walk grid (doors / gates opened by scripts) ---------------------------------------------------------

    def live_walk(self, e):
        """the walk grid = the live grid OR the map's own (closed doors open when touched)"""
        raw = e.read(e.r32(MAPOBJ + 0xA8), 32 * self.grid.h)
        w = self.grid.w
        if getattr(self.grid, 'static', None) is None:
            self.grid.static = bytes(self.grid.walk)
        st = self.grid.static
        live = bytearray((raw[ty * 32 + (tx >> 3)] >> (tx & 7)) & 1 for ty in range(self.grid.h) for tx in range(w))
        self.grid.walk = live
        self.grid.union = bytearray(a | b for a, b in zip(live, st))
        if not getattr(self.grid, 'patched', False):
            g = self.grid
            plain = g.bfs

            def bfs(start, goal, fw=2, fh=1, blocked=()):
                r = plain(start, goal, fw, fh, blocked)
                if r is None:
                    # a closed door (opens when touched): retry with the map's own grid
                    keep = g.walk
                    g.walk = g.union
                    try:
                        r = plain(start, goal, fw, fh, blocked)
                    finally:
                        g.walk = keep
                if r is None:
                    # standing on a tile the grid calls blocked (under a canopy, pushed into a wall): start from the
                    # nearest free tile around
                    for rad in (1, 2, 3):
                        for dx in range(-rad, rad + 1):
                            for dy in range(-rad, rad + 1):
                                if max(abs(dx), abs(dy)) != rad:
                                    continue
                                st = (start[0] + dx, start[1] + dy)
                                if g.ok_box(st[0], st[1], fw, fh):
                                    r = plain(st, goal, fw, fh, blocked)
                                    if r is not None:
                                        return [start] + r
                return r
            g.bfs = bfs
            g.patched = True

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
            return self.heal_step(e) or 0
        x, y = s['p'][0], s['p'][1]
        ens = [o for o in enemies(e) if abs(o[1] - x) + abs(o[2] - y) <= radius and o[0] not in self.skip
               and o[5] not in self.AVOID]
        ens = [o for o in ens if self.reachable(e, s, o[1], o[2])]
        if not ens or self.t >= cap:
            if self.t >= cap and ens:
                raise RuntimeError(f'fight not over after {cap} frames: {ens}')
            self.idle += 1
            if self.idle < 30:
                return 0
            if self.in_nav:
                self.fight_done = True
                return 0
            self.next_goal()
            return None
        self.idle = 0
        en = min(ens, key=lambda o: abs(o[1] - x) + abs(o[2] - y))
        ex, ey = en[1], en[2]
        # an enemy that takes no damage over 400 frames of being the target (out of reach, invulnerable) is skipped
        tg = self.tgt.setdefault(en[0], [en[4], 0])
        if en[4] < tg[0]:
            tg[0], tg[1] = en[4], 0
        else:
            tg[1] += 1
            if tg[1] > 400 and not (len(g) > 4 and g[4]):
                self.skip.add(en[0])
        k = self.flame_guard(e, s)
        if k is None:
            k = self.block_guard(e, s)
        if k is not None:
            return k
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
            # never out through a door (fleeing a Destroyer at a doorway would bounce between two rooms)
            if any(r['x'] - 16 <= nx < r['x'] + r['w'] + 16 and r['y'] - 16 <= ny < r['y'] + r['h'] + 16
                   for r in self.exit_rects(s)):
                continue
            d = abs(nx - ex) + abs(ny - ey)
            if best is None or d > best:
                best, bk = d, k
        return bk

    BUU = (26, 27)                       # Majin Buu (7320 / 14640 HP)

    def buu(self, e, s):
        """Majin Buu with Babidi (8-71): Buu takes damage only while pacified (0x0300302C != 0: he wanders, 0xA2 /
        0xA3, and does not fight back). A punch on Babidi pacifies him (Babidi's 0xC9 -> enter 0x0802AEDA sets it, for
        ~770 frames, until Babidi's next gesture 0x0802AD88 clears it); Babidi can be hit only while he wanders on the
        ground (0xC8, >= 300 frames after each gesture), not while he hovers (0xC5). So: block (R) turned to Buu, then
        punch Babidi when he lands, then punch Buu (A every 6 frames from beside him); step across the line of Buu's
        candy beam (0x9B)"""
        if s['p'] is None:
            return 0
        if self.maybe_heal(e, s):
            return self.heal_step(e) or 0
        ens = enemies(e)
        b = next(o for o in ens if o[5] in self.BUU)
        bab = next((o for o in ens if o[5] in (4, 29)), None)
        st = (ai_state(e, b[0]) or [0])[-1]
        bst = (ai_state(e, bab[0]) or [0])[-1] if bab else 0
        x, y, pf = s['p']
        pacified = e.r32(0x0300302C) != 0
        if st == 0x9B and not pacified:
            self.mash = 0
            if b[3] >= 2:
                return KEY.DOWN if y >= b[2] else KEY.UP
            return KEY.RIGHT if x >= b[1] else KEY.LEFT
        tgt = b if pacified else bab if bab is not None and bst == 0xC8 else None
        if tgt is None:
            # Buu cannot be hurt: guard (R, a block from the front takes 1/16) turned to him
            self.mash = 0
            dx, dy = b[1] - x, b[2] - y
            face = (3 if dx > 0 else 2) if abs(dx) >= abs(dy) else (0 if dy > 0 else 1)
            if pf != face:
                if self.blocking(e):
                    return 0
                return (KEY.DOWN, KEY.UP, KEY.LEFT, KEY.RIGHT)[face]
            return KEY.R
        bx, by = tgt[1], tgt[2]
        dx, dy = bx - x, by - y
        if -20 <= dy <= 4 and 3 <= abs(dx) <= 21:
            face = 3 if dx > 0 else 2
            if pf != face:
                self.mash = 0
                return KEY.RIGHT if dx > 0 else KEY.LEFT
            self.mash += 1
            return KEY.A if self.mash % 6 < 2 else 0
        self.mash = 0
        sides = sorted((-1, 1), key=lambda sd: abs(bx + sd * 16 - x))
        tx, ty = bx + sides[0] * 16, by
        if self.grid is not None and not self.grid.ok_box(tx // 8, ty // 8):
            tx = bx + sides[1] * 16
        if abs(x - tx) + abs(y - ty) > 24:
            return self.walk_keys(e, s, tx, ty)
        k = 0
        if abs(x - tx) > 1:
            k |= KEY.RIGHT if tx > x else KEY.LEFT
        if abs(y - ty) > 1:
            k |= KEY.DOWN if ty > y else KEY.UP
        return k

    def exit_rects(self, s):
        import nav as navm
        key = f"{s['area'][0]}-{s['area'][1]}"
        if getattr(self, '_exits', (None,))[0] != key:
            self._exits = (key, [x[1] for x in navm.exits(key)])
        return self._exits[1]

    def yakon(self, e, s, g):
        """('yakon', cap): Yakon (re/buu/YAKON.md): flee his punch combo (0x321 / 0x322) and his mouth beam's column
        (0x326), Energy Blasts in the pause after the combo (0x323: full damage), the Mini-Yakons when summoned,
        punches otherwise (melee always hurts him). Done when no enemy is left."""
        if s['p'] is None:
            return 0
        if self.maybe_heal(e, s):
            return self.heal_step(e) or 0
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
            raw = e.read(e.r32(MAPOBJ + 0xA8), 32 * self.grid.h)
            live = lambda tx, ty: 0 <= tx < self.grid.w and 0 <= ty < self.grid.h and \
                (raw[ty * 32 + (tx >> 3)] >> (tx & 7)) & 1  # noqa: E731
            from collections import deque
            st = (s['p'][0] // 8, s['p'][1] // 8)
            seen = {st}
            q = deque([st])
            while q:
                cx, cy = q.popleft()
                for d in ((1, 0), (-1, 0), (0, 1), (0, -1)):
                    n = (cx + d[0], cy + d[1])
                    if n not in seen and live(*n):
                        seen.add(n)
                        q.append(n)
            self.reach = seen
            self.reach_at = self.t
        if len(self.reach) < 64:
            return True          # standing in a doorway / on a closed tile: unknown
        tx, ty = ex // 8, ey // 8
        return any((tx + a, ty + b) in self.reach for a in (-2, -1, 0, 1, 2) for b in (-1, 0, 1))

    def walk_keys(self, e, s, tx, ty):
        """walk()'s steering toward (tx, ty) without completing the goal (out of a Destroyer's flame first)"""
        k = self.flame_guard(e, s)
        if k is None:
            k = self.block_guard(e, s)
        if k is not None:
            return k
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
