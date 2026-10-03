"""Dungeon 7: Kyodai Pyramid (world 14), the 7-Star Dragon Ball (s406, flag 118).

Main line (what the scripts require): Trunks L80 opens the 14-1 gate (flag 175) -> Dr. Challenger s400 (flag 100) ->
his four men: 14-6 (s412, 101), 14-10 (s414, 102), 14-18 (s417, 103), 14-25 (s422, 104, behind the boss) -> the four
switches s415 (273 @ 14-16, 274 @ 14-17, 276 @ 14-22, 275 @ 14-19) lift 14-16's north door -> 14-23 -> 14-24
Totenhotep (s419 / s420, flag 105), his Headdress (item 39, s421, flag 106) -> back to 14-2: s406 (flag 118) -> 14-1's
pad -> the world map. 14-7 .. 14-9 and 14-13 .. 14-15 are not needed (14-3's east door leads to 14-11)."""
import json

from dbdriver import DbDriver, KEY, map_item, navm, status
from driver import DIRS

A, B, U, D, L, R = KEY.A, KEY.B, KEY.UP, KEY.DOWN, KEY.LEFT, KEY.RIGHT

_traps = {}


def traps(key):
    """the rects of the area's touch event triggers that hurt / warp (14-4's darts s407..s411, 14-17's pits s416)"""
    if key not in _traps:
        out = []
        a = navm.areas().get(key)
        if a is not None:
            o = json.loads((navm.CONTENT / 'maps' / a['maps'][0] / 'objects.json').read_text())
            for ar in o.get('areas', []):
                if ar['key'] != key:
                    continue
                for t in ar['objects']:
                    if t['type'] == 'event_trigger' and t.get('event') in ('s416', 's407', 's408', 's409', 's410', 's411'):
                        out.append(t['rect'])
        _traps[key] = out
    return _traps[key]


class PyramidDriver(DbDriver):
    TRAP_AREAS = ('14-17',)

    def step(self, e):
        g = self.goals[self.k]
        if g[0] == 'path':
            # ('path', [(x, y), ...]): exact axis-aligned moves through the points (x first, then y), within 2 px;
            # the 14-17 pit maze leaves 15 px lanes for the player's position point
            s = status(e)
            if s['scene'] > 0:
                return self.mash_a()
            if s['p'] is None:
                return 0
            if self.t == 0:
                self.pi = 0
                self.parea = s['area'][:2]
            if s['area'][:2] != self.parea:
                raise RuntimeError(f'path left the area at point {self.pi} {s}')
            x, y = s['p'][0], s['p'][1]
            while self.pi < len(g[1]):
                tx, ty = g[1][self.pi]
                if abs(x - tx) > 2:
                    return KEY.RIGHT if tx > x else KEY.LEFT
                if abs(y - ty) > 2:
                    return KEY.DOWN if ty > y else KEY.UP
                self.pi += 1
            self.next_goal()
            return None
        if g[0] == 'item':
            # the map item is taken with A next to it (the boss can die against a wall: the item then lies half
            # inside it and its top edge cannot be reached)
            s = status(e)
            if s['scene'] > 0:
                self.goals[self.k] = ('events',)
                return self.mash_a()
            if s['p'] is None:
                return 0
            if self.t > 1500:
                raise RuntimeError(f'item not reached {g}')
            box = map_item(e)
            if box is None:
                return 0
            x, y = s['p'][0], s['p'][1]
            if abs(x - box[0]) <= 12 and abs(y - (box[1] + 4)) <= 12 and self.t % 24 >= 12:
                return KEY.A if self.t % 24 < 15 else 0
            return self.walk_keys(e, s, box[0], box[1] + 4)
        if g[0] == 'shoot':
            # ('shoot', x, y, dir): from (x, y) turn to dir and fire Energy Blasts (B) until an event starts
            s = status(e)
            if s['scene'] > 0:
                self.goals[self.k] = ('events',)
                return self.mash_a()
            if s['p'] is None:
                return 0
            if self.t > 3000:
                raise RuntimeError(f'shoot: no event {g} {s}')
            x, y = s['p'][0], s['p'][1]
            if getattr(self, 'shoot_t', None) is None:
                if abs(x - g[1]) > 1 or abs(y - g[2]) > 1:
                    return self.walk_keys(e, s, g[1], g[2])
                self.shoot_t = 0
            ph = self.shoot_t % 50
            self.shoot_t += 1
            if abs(x - g[1]) > 6 or abs(y - g[2]) > 6:
                self.shoot_t = None
                return 0
            return DIRS[g[3]] if ph < 2 else KEY.B if 6 <= ph < 9 else 0
        return super().step(e)

    def next_goal(self):
        super().next_goal()
        self.shoot_t = None


# 14-12 is split by a wall: its west half (14-11 / 14-13 doors) and east half (14-15 / 14-16 doors) connect only
# through the loop 14-13 -> 14-14 -> 14-15
IN = [('go', 14, 12, 'near'), ('go', 14, 13, 'near'), ('go', 14, 14, 'near'), ('go', 14, 15, 'near'),
      ('go', 14, 12, 'near'), ('go', 14, 16, 'near')]
OUT = [('go', 14, 12, 'near'), ('go', 14, 15, 'near'), ('go', 14, 14, 'near'), ('go', 14, 13, 'near'),
       ('go', 14, 12, 'near'), ('go', 14, 11, 'near')]


# 14-21, the bridge room (s418; the on-enter script sets flag 271): four bridge blocks, two switches (breakables:
# prop 4 at (272, 112) between the north corridor and the middle strip, prop 5 at (72, 336) between the south-west
# column and the south room). State 272 opens (24,160) (104,160) (264,288) (184,288); state 271 opens (184,160)
# (264,160) (104,288) (24,288). From the north door: switch A from above (-> 272), west column -> (24,160) ->
# (104,160) -> the strip, switch A from below (-> 271), (264,160) -> the east column, an Energy Blast up at switch A
# (-> 272), (264,288) -> the south room, switch B from its east side (-> 271), (104,288) -> (24,288) -> the exit.
BRIDGES_21 = [
    ('go', 14, 21, False),
    ('shoot', 272, 96, 'D'), ('shoot', 272, 136, 'U'), ('shoot', 272, 200, 'U'), ('shoot', 104, 352, 'L'),
    ('mark', 'p08b_bridges'),
]


def db7():
    return [
        ('mark', 'p00_start'),
        ('trig', 224, 84, A),                      # the gate: s517 (Trunks L80), flag 175
        ('go', 14, 2, 'near'), ('events',),        # s400 Dr. Challenger
        ('mark', 'p01_challenger'),
        ('go', 14, 6, 'near'),
        ('mark', 'p02_14-6'),
        ('trig', 40, 130, A),                      # man 1 (s412)
        ('mark', 'p03_man1'),
        ('go', 14, 10, 'near'),
        ('walk', 200, 104), ('talk', 'U'),         # man 2 (s414)
        ('mark', 'p04_man2'),
        *IN,
        ('walk', 144, 136), ('events',),           # switch 1 (s415, flag 273)
        ('mark', 'p05_sw1'),
        # 14-17, the pit maze (s416 pits drop into 14-12 .. 14-15): lanes x 24..63 / 89..143 (89..103 at the
        # cross pits) / 161..215 / 241..280 for the position point
        ('go', 14, 17, False),
        ('path', [(40, 636), (40, 816), (96, 816), (96, 992), (40, 992), (40, 916)]),
        ('go', 14, 18, False),
        ('walk', 112, 144), ('talk', 'U'),         # man 3 (s417)
        ('mark', 'p06_man3'),
        ('go', 14, 17, False),
        ('path', [(40, 916), (40, 992), (96, 992), (96, 680), (168, 680), (168, 160), (260, 160), (260, 84)]),
        ('events',),                               # switch 2 (flag 274)
        ('mark', 'p07_sw2'),
        ('path', [(260, 160), (168, 160), (168, 90), (96, 90), (96, 456), (40, 456), (40, 116)]),
        ('go', 14, 22, False),
        ('walk', 144, 120), ('events',),           # switch 4 (flag 276)
        ('mark', 'p08_sw4'),
        *BRIDGES_21,
        ('go', 14, 19, 'near'),
        ('walk', 336, 144), ('events',),           # switch 3 (flag 275)
        ('mark', 'p09_sw3'),
        ('go', 14, 16, 'near'),
        ('go', 14, 24, 'near'),
        ('mark', 'p10_boss'),
        ('walk', 192, 200), ('events',),           # s419
        ('clear', 30000),
        ('item',), ('events',),                    # the Headdress (s421)
        ('mark', 'p11_headdress'),
        ('go', 14, 25, 'near'),
        ('walk', 192, 176), ('talk', 'U'),         # man 4 (s422)
        ('mark', 'p12_man4'),
        *OUT,
        ('go', 14, 2, 'near'),
        ('walk', 264, 72), ('events',),            # s406: the 7-Star Dragon Ball
        ('mark', 'p13_ball'),
        ('go', 14, 1, 'near'),
        ('trig', 144, 216, A),                     # the pad: the world map
        ('wait', 120),
        ('mark', 'p14_map'),
    ]


ROUTES = {'db7': db7}
DRIVER = PyramidDriver
