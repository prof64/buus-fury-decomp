"""Route for Dragon Ball dungeon 6: Fukurou Forest / Ninja Stronghold (world 13, 6-Star ball, flag 117)."""
from dbdriver import KEY, DbDriver, status

ITEM_VT = (0x0804C508, 0x0804A314)     # map item taken with A (the balls) / by touch (the Ninja Key)


def item_box(e):
    n = e.r32(0x03001C4C + 4)
    for k in range(min(n, 255)):
        o = e.r32(0x03001C4C + 8 + 4 * k)
        if e.r32(o) in ITEM_VT:
            return [e.r32(o + i) for i in (0x18, 0x1C, 0x20, 0x24)]
    return None


class Db6Driver(DbDriver):
    def step(self, e):
        g = self.goals[self.k]
        if g[0] == 'pick':
            # ('pick',): walk onto the area's map item (either kind); beside it: DOWN + A; done when it is gone
            s = status(e)
            if s['scene'] > 0:
                self.goals[self.k] = ('events',)
                return self.mash_a()
            if s['p'] is None:
                return 0
            box = item_box(e)
            if box is None:
                if self.t > 60:
                    self.next_goal()
                    return None
                return 0
            if self.t > 2500:
                raise RuntimeError(f'item not reached {g} {box}')
            x, y = s['p'][0], s['p'][1]
            # walk at the box until the walk stops making progress next to it, then face it and press A
            near = abs(x - box[0]) <= 20 and -12 <= y - box[1] <= 20
            if (x, y) == getattr(self, 'pick_last', None):
                self.pick_still += 1
            else:
                self.pick_still = 0
            self.pick_last = (x, y)
            if not near or self.pick_still < 20 and self.t % 200 < 150:
                return self.walk_keys(e, s, box[0], box[1] + 4)
            d = KEY.DOWN if y < box[1] else KEY.UP if y >= box[3] - 4 else KEY.RIGHT if x < box[0] else KEY.LEFT
            ph = self.t % 24
            return d if ph < 6 else KEY.A if 8 <= ph < 11 else 0
        return super().step(e)


DRIVER = Db6Driver

A, B, U, D, L, R = KEY.A, KEY.B, KEY.UP, KEY.DOWN, KEY.LEFT, KEY.RIGHT


def db6():
    """From the 13-1 save point (Goten): the level gate (s518: Goten L80, flag 187) -> 13-2 (s390 walks into 13-3) ->
    the stronghold 13-32 .. 13-47 -> 13-48 .. 13-52 (the Ninja Key, s396, flag 98) -> back to 13-47, the locked door
    (s395) -> 13-53: the Ninja Boss (s397 / s398, flag 99) -> the 6-Star Dragon Ball (s399, flag 117) -> back out to
    13-1's pad -> the world map."""
    return [
        ('mark', 'n00_start'),
        ('walk', 112, 92), ('talk', 'U'),                 # the gate
        ('go', 13, 47, 'near'),
        ('mark', 'n01_13-47'),
        ('go', 13, 52, 'near'),
        ('mark', 'n02_13-52'),
        ('clear', 20000),
        ('pick',), ('events',),                           # the Ninja Key
        ('mark', 'n03_key'),
        ('go', 13, 47, 'near'),
        ('walk', 240, 196), ('talk', 'U'),                # the locked door
        ('mark', 'n04_door'),
        ('go', 13, 53, False),
        ('events',),
        ('mark', 'n05_boss'),
        ('clear', 30000),
        ('mark', 'n06_ball'),
        ('pick',), ('events',),
        ('mark', 'n07_out'),
        ('go', 13, 1, 'near'),
        ('mark', 'n08_13-1'),
        ('walk', 112, 56), ('talk', 'D'), ('unstick',),    # the gate closes on every entry (on-enter block_area)
        ('trig', 80, 246, A),
        ('wait', 120),
        ('mark', 'n09_map'),
    ]


ROUTES = {'db6': db6}
