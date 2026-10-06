"""field coverage tours of worlds 11-14 (Diablo Desert, Diablo Desert Express, Airship, Fukurou Forest, Ninja
Stronghold, Kyodai Pyramid): goal lists for run_tours.py (states work/states/tour_desert_*.ss, made with
tools/run2/state_at.py from run 1's master rows at settled moments -- a state mid area load is an unsupported cut
for the segment import -- or saved from these tours at an area's entry, see CONT).

Adds two driver goals (patched into tours.TourDriver.step; other tours' goals are untouched):
  ('bfight', cap)          punch / ki blast the nearest enemy along the BFS path, never chasing it through an exit
  ('exit', x, y, w, a)     walk toward (x, y) (inside an exit rect) until the area is W-A
Breakables here (rocks, pots) take ki blasts (B), not punches; tanks (11-3, 11-9, 11-10, 11-12) can't be beaten by the
driver, so those areas are crossed or only briefly fought."""
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import tours  # noqa: E402
from tours import area_goals  # noqa: E402

A, B = 1, 2
L_, R_ = 32, 16


_EXITS = {}


def _exits(area):
    """the area's exit rects (x0, y0, x1, y1) from its objects.json (edge exits, doors)"""
    key = (area[0], area[1])
    if key not in _EXITS:
        import json
        p = tours.CONTENT / 'maps' / f'area_{area[0]:02d}_{area[1]:03d}' / 'objects.json'
        d = json.loads(p.read_text()) if p.exists() else {}
        rs = [t['rect'] for t in d.get('map_triggers', []) if 'to' in t]
        rs += [o['rect'] for ar in d.get('areas', []) for o in ar['objects'] if 'to' in o and o.get('rect')]
        _EXITS[key] = [(r['x'], r['y'], r['x'] + r['w'], r['y'] + r['h']) for r in rs]
    return _EXITS[key]


def _near_exit(area, x, y):
    """footprint (x, y, 16 x 8) within ~12 px of an exit rect"""
    return any(x0 - 28 <= x <= x1 + 12 and y0 - 20 <= y <= y1 + 12 for (x0, y0, x1, y1) in _exits(area))


def _bfight(self, e, s, cap):
    """('bfight', cap): like the driver's 'fight' (punch the nearest enemy until the area has none) but the approach
    walks the BFS path (around rocks), and a side that stays blocked is swapped for the other one"""
    ens = [o for o in tours.objs(e) if o[0] == 'enemy' and o[2] is not None]
    if not ens or self.t >= cap or s['p'] is None:
        if not ens and s['p'] is not None:
            self.idle += 1
            if self.idle < 40:
                return 0
        if s['p'] is None and self.t < cap:
            return 0
        self.next_goal()
        return None
    self.idle = 0
    x, y = s['p'][0], s['p'][1]
    en = min(ens, key=lambda o: abs(o[2] - x) + abs(o[3] - y))
    ex, ey = en[2], en[3]
    # blocked on the way (walk's stuck count): approach from the other side for a while
    if self.stuck > 45:
        self.bf_flip = self.t + 180
        self.stuck = 0
    side = -1 if x < ex else 1
    if getattr(self, 'bf_flip', 0) > self.t:
        side = -side
    tx, ty = ex + side * 18, ey
    if _near_exit(s['area'], tx, ty) or _near_exit(s['area'], x, y):
        # never chase an enemy through an exit: step back from the edge and let it come
        self.mash = 0
        k = 0
        for (x0, y0, x1, y1) in _exits(s['area']):
            if x0 - 28 <= x <= x1 + 12 and y0 - 20 <= y <= y1 + 12:
                k |= (R_ if x0 < 40 else L_ if x1 > 200 and x0 > 200 else 0)
                k |= (128 if y0 < 40 and y1 - y0 < 40 else 64 if y0 > 200 and y1 - y0 < 40 else 0)
        return k
    if abs(x - tx) > 2 or abs(y - ty) > 2:
        self.mash = 0
        return self.walk_point(e, s, tx, ty)
    want = R_ if side < 0 else L_
    if s['p'][2] != (3 if side < 0 else 2):
        return want
    self.mash += 1
    # punches, and a ki blast every third swing (the tanks only take ki blasts)
    c = self.mash % 30
    return A if c < 2 or 10 <= c < 12 else (B if 20 <= c < 22 else 0)


_step0 = tours.TourDriver.step


def _step(self, e):
    g = self.goals[self.k]
    if g[0] == 'exit':
        # ('exit', tx, ty, w, a): walk toward (tx, ty) (inside an exit rect) until the area is W-A
        s = tours.status(e)
        if s['area'][:2] == [g[3], g[4]] or self.t > 1500:
            self.next_goal()
            return None
        if s['scene'] > 0:
            return self.mash_a()
        if s['p'] is None:
            return 0
        k = self.walk_point(e, s, g[1], g[2])
        if not k:
            x, y = s['p'][0], s['p'][1]
            k = (R_ if g[1] > x + 1 else L_ if g[1] < x - 1 else 0) | (128 if g[2] > y + 1 else 64 if g[2] < y - 1 else 0)
        if self.stuck > 80:
            self.stuck = 0
        return k
    if g[0] == 'bfight':
        s = tours.status(e)
        if s['scene'] > 0:
            return self.mash_a()
        if self.t == 0:
            self.stuck = 0
            self.bf_flip = 0
        return _bfight(self, e, s, g[1])
    return _step0(self, e)


tours.TourDriver.step = _step


def punch(hb, side='below'):
    """break a breakable prop (its hit box [x0, y0, x1, y1]): stand next to it, face it, ki blast (B) three times (the
    desert's rocks take B, not A), then play its break event through"""
    x0, y0, x1, y1 = hb
    if side == 'below':
        g = ('a_at', (x0 + x1) // 2 - 8, y1 + 20, 'U')
    elif side == 'above':
        g = ('a_at', (x0 + x1) // 2 - 8, y0 - 28, 'D')
    elif side == 'left':
        g = ('a_at', x0 - 40, (y0 + y1) // 2 - 4, 'R')
    else:
        g = ('a_at', x1 + 24, (y0 + y1) // 2 - 4, 'L')
    return [g, ('wait', 10), ('hold', B, 3), ('wait', 30), ('hold', B, 3), ('wait', 30), ('hold', B, 3), ('wait', 40),
            ('events',)]


def go(w, a, x, y, tx, ty, cap=3500):
    """walk to (x, y) then on to (tx, ty) (inside an exit rect) until the area is W-A, then let it load"""
    return [('walk', x, y), ('exit', tx, ty, w, a), ('wait', 60), ('bfight', cap)]


# Diablo Desert, from 11-1 (run 1 row 779,730, after the arrival scene): 11-1's old man and his rock (break event
# s369, then s368), the A-up scripts at the rock spot; up into the never-entered 11-7 (fight, 4 rocks), right into
# the never-entered 11-8 (fight, 5 rocks)
DESERT_A = ([('wait', 60), ('a_at', 244, 84, 'U')]
            + punch([209, 64, 239, 80]) + [('a_at', 244, 84, 'U')]
            + [('a_at', 224, 192, 'U'), ('a_at', 224, 192, 'U'), ('a_at', 224, 192, 'U')]
            + go(11, 7, 120, 40, 120, 4)
            + punch([177, 248, 207, 264]) + punch([305, 224, 335, 240]) + punch([81, 120, 111, 136])
            + punch([297, 56, 327, 72])
            + go(11, 8, 440, 160, 462, 160)
            + punch([113, 136, 143, 152]) + punch([65, 208, 95, 224]) + punch([337, 256, 367, 272])
            + punch([417, 80, 447, 96]) + punch([385, 56, 415, 72]))

# from 11-13's upper ledge (run 1 row 788,300): its save point; right through 11-14 and 11-15 (fights) into the
# never-entered 11-16's upper ledge (the A script at the post; the ledge doesn't reach the lower ground)
DESERT_B = ([('wait', 30)] + area_goals('11-13', 1)[:1]
            + go(11, 14, 440, 150, 462, 150) + go(11, 15, 440, 150, 462, 150) + go(11, 16, 440, 150, 462, 150)
            + area_goals('11-16', 1) + [('wait', 60)])

# from 11-11 (run 1 row 786,330, entering from 11-10): its rocks, right into the never-entered 11-12 (its tank
# can't be beaten by the driver and kills it at its rocks: straight through, up the left side), onto 11-16's lower
# ground, right onto 11-13's lower ground, down into the never-entered 11-9 (the scripted rock). (11-10's rocks: its
# tank kills the driver)
DESERT_C = ([('wait', 60), ('bfight', 3500)]
            + punch([217, 176, 247, 192]) + punch([369, 232, 399, 248]) + punch([97, 80, 127, 96])
            + go(11, 12, 440, 160, 462, 160, 0)
            + [('walk', 40, 100)]
            + go(11, 16, 40, 30, 40, 4)
            + go(11, 13, 440, 260, 462, 260)
            + go(11, 9, 220, 290, 220, 310, 600)  # its tank too: a short go at it
            + punch([129, 232, 159, 248]))

# from 11-2 (run 1 row 780,020): its rocks, right into the never-entered 11-3 (fight, 3 rocks), right to 11-4 (rocks)
DESERT_D = ([('wait', 30), ('bfight', 3500)]
            + punch([297, 216, 327, 232]) + punch([393, 248, 423, 264]) + punch([417, 288, 447, 304], 'left')
            + punch([369, 48, 399, 64])
            + go(11, 3, 440, 160, 462, 160, 900)  # its tank takes no damage from the driver
            + punch([97, 224, 127, 240]) + punch([121, 56, 151, 72]) + punch([329, 104, 359, 120])
            + go(11, 4, 440, 160, 462, 160)
            + punch([161, 200, 191, 216]) + punch([201, 240, 231, 256]) + punch([257, 216, 287, 232])
            + punch([241, 168, 271, 184]) + punch([193, 160, 223, 176]))

# from 11-6 (run 1 row 782,975): its rocks, left to 11-5 (the A event s370 at the bottom left, rocks)
DESERT_E = ([('wait', 60), ('bfight', 3500)]
            + punch([169, 272, 199, 288], 'left') + punch([321, 240, 351, 256]) + punch([313, 128, 343, 144])
            + punch([97, 96, 127, 112])
            + go(11, 5, 24, 160, 2, 160)
            + [('a_at', 76, 234, 'D'), ('a_at', 76, 262, 'U')]  # the chest (s370: Spiked Breastplate)
            + punch([369, 216, 399, 232]) + punch([89, 96, 119, 112]) + punch([65, 80, 95, 96], 'left')
            + punch([97, 64, 127, 80], 'above'))

# Diablo Desert Express: 11-17's A script (run 1 row 789,410, after the boarding scene: it takes the player off the
# train, back to 11-13); 11-24's passengers (row 795,480, after the bandit fight and its scene)
EXPRESS_17 = [('wait', 30)] + area_goals('11-17', 1) + [('a_at', 384, 112, 'R'), ('wait', 30)]
EXPRESS_24 = [('wait', 30), ('a_at', 398, 200, 'R'), ('a_at', 434, 168, 'L'),
              ('a_at', 434, 184, 'L'), ('wait', 30)]

# Airship: 12-1's save point on the way in (row 799,800) and its "no time to save" event on the way out (row
# 828,920); 12-27's "no time to save" event (row 817,000)
AIR_1 = [('wait', 60), ('a_at', 252, 116, 'U'), ('wait', 60)]
AIR_1B = [('wait', 30), ('a_at', 252, 116, 'U'), ('wait', 30)]
AIR_27 = [('wait', 60), ('a_at', 179, 320, 'U'), ('wait', 30)]

# Ninja Stronghold: from 13-36 (row 832,940) west through the never-entered 13-37, 13-38, 13-39, 13-40 to 13-41
NINJA_A = ([('wait', 60), ('bfight', 2500)]
           + go(13, 37, 40, 138, 2, 138) + go(13, 38, 40, 106, 2, 106) + go(13, 39, 80, 30, 80, 2)
           + go(13, 40, 250, 138, 278, 138) + go(13, 41, 250, 106, 300, 106))
# the pots: 13-33 (row 832,420), 13-42 (row 833,300), 13-45 (row 834,470), 13-53 (row 841,960); 13-47's save
# point (row 835,640)
NINJA_33 = [('wait', 30)] + punch([184, 48, 200, 56]) + [('wait', 30)]
NINJA_42 = [('wait', 60), ('bfight', 2500)] + punch([128, 48, 144, 56]) + punch([144, 48, 160, 56]) + [('wait', 30)]
NINJA_45 = [('wait', 60), ('bfight', 2500)] + punch([192, 48, 208, 56]) + [('wait', 30)]
NINJA_47 = [('wait', 60), ('a_at', 299, 200, 'U'), ('wait', 30)]
NINJA_53 = [('wait', 30), ('bfight', 2500)] + punch([8, 80, 27, 96], 'right') + [('wait', 30)]

# Kyodai Pyramid: 14-1 (row 849,870) the three A-up scripts at the door
PYRAMID_1 = [('wait', 60), ('a_at', 224, 80, 'U'), ('a_at', 224, 80, 'U'), ('a_at', 224, 80, 'U'), ('wait', 30)]
# 14-5 (row 854,250): the right door into the never-entered 14-7; only its first pot is on the entry ledge (the rest
# of the room, and 14-8 / 14-9 behind it, isn't walkable from there)
PYRAMID_5 = ([('wait', 60), ('walk', 264, 60)] + go(14, 7, 264, 40, 264, 20, 300)
             + [('a_at', 40, 264, 'R'), ('wait', 10), ('hold', B, 3), ('wait', 30), ('hold', B, 3), ('wait', 30),
                ('hold', B, 3), ('wait', 40), ('events',), ('wait', 30)])
# 14-16 (row 860,060): the save point
PYRAMID_16 = [('wait', 60), ('a_at', 144, 192, 'U'), ('wait', 30)]
# Totenhotep's Chamber 14-25 (row 873,990): the four chests (s423-s426)
PYRAMID_25 = [('wait', 60), ('a_at', 72, 90, 'U'), ('a_at', 152, 90, 'U'), ('a_at', 232, 90, 'U'),
              ('a_at', 312, 90, 'U'), ('wait', 30)]

# continuations (to compare past an earlier tour's first divergence): states saved from the tours above at the
# entry of an area (work/shipfix/desert_mkstate.py TOUR GOAL NAME), the same goals from there on
CONT = {'desert_a7': ('tour_desert_d7.ss', DESERT_A[18:59] + [('wait', 30)]),
        'desert_a8': ('tour_desert_d8.ss', DESERT_A[58:]),
        'desert_e5': ('tour_desert_d5.ss', DESERT_E[41:]),
        'desert_nj37': ('tour_desert_nj37.ss', NINJA_A[5:9] + [('wait', 30)]),
        'desert_nj38': ('tour_desert_nj38.ss', NINJA_A[9:13] + [('wait', 30)]),
        'desert_nj39': ('tour_desert_nj39.ss', NINJA_A[13:17] + [('wait', 30)]),
        'desert_nj40': ('tour_desert_nj40.ss', NINJA_A[17:21] + [('wait', 30)])}

TOURS = {'desert_a': ('tour_desert_d1.ss', DESERT_A),
         'desert_b': ('tour_desert_d13.ss', DESERT_B),
         'desert_c': ('tour_desert_d11.ss', DESERT_C),
         'desert_d': ('tour_desert_d2.ss', DESERT_D),
         'desert_e': ('tour_desert_d6.ss', DESERT_E),
         'desert_exp17': ('tour_desert_exp17.ss', EXPRESS_17),
         'desert_exp24': ('tour_desert_exp24b.ss', EXPRESS_24),
         'desert_air1': ('tour_desert_air1.ss', AIR_1),
         'desert_air1b': ('tour_desert_air1b.ss', AIR_1B),
         'desert_air27': ('tour_desert_air27.ss', AIR_27),
         'desert_nj36': ('tour_desert_nj36.ss', NINJA_A),
         'desert_nj33': ('tour_desert_nj33.ss', NINJA_33),
         'desert_nj42': ('tour_desert_nj42.ss', NINJA_42),
         'desert_nj45': ('tour_desert_nj45.ss', NINJA_45),
         'desert_nj47': ('tour_desert_nj47.ss', NINJA_47),
         'desert_nj53': ('tour_desert_nj53.ss', NINJA_53),
         'desert_py1': ('tour_desert_py1.ss', PYRAMID_1),
         'desert_py5': ('tour_desert_py5.ss', PYRAMID_5),
         'desert_py16': ('tour_desert_py16.ss', PYRAMID_16),
         'desert_py25': ('tour_desert_py25.ss', PYRAMID_25)}

TOURS.update(CONT)
