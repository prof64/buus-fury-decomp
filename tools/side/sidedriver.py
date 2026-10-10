"""Side-content driver goals (on top of tools/superbuu/sbdriver.py):
  ('bossx', cap)   'boss' with every exit of the current area blocked for the walk (fights never leave the area)
  ('bosst', [types], cap[, [dodge types]])  punch only the enemies of these types (enemy table index, e.g. 0x33 Janemba); done when an
                   event starts (the defeat scene) or none of them is left
"""
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent.parent
sys.path.insert(0, str(ROOT / 'tools' / 'superbuu'))
sys.path.insert(0, str(ROOT / 'tools' / 'story_playthrough'))
sys.path.insert(0, str(ROOT / "tools" / "field"))
import sbdriver  # noqa: E402
import driver  # noqa: E402
from route import status  # noqa: E402

_step = driver.Driver.step


def bosst(self, e, g):
    from common_field import enemy_detail
    s = status(e)
    if s['scene'] > 0:
        self.next_goal()
        return None
    if self.t >= g[2]:
        raise RuntimeError(f'bosst not over after {g[2]} frames')
    # the types in priority order: the first type that has a living one is fought (Janemba: the minis first)
    alive = [d for d in enemy_detail(e) if d['type'] in g[1] and d['hp1024'] > 0]
    ens = []
    for t in g[1]:
        ens = [d for d in alive if d['type'] == t]
        if ens:
            break
    if not ens:
        self.idle += 1
        if self.idle >= 30:
            self.next_goal()
            return None
        return 0
    self.idle = 0
    if s['p'] is None:
        return 0
    x, y = s['p'][0], s['p'][1]
    # dodge: an enemy of the dodge types (g[3], e.g. Janemba's hand strike 0x36 created under the player) close by:
    # step away from it for as long as it is there
    dodge = g[3] if len(g) > 3 else ()
    if dodge:
        hz = [d for d in enemy_detail(e) if d['type'] in dodge]
        hz = [d for d in hz if abs((d['x'] >> 8) - x) < 36 and abs((d['y'] >> 8) - y) < 30]
        if hz:
            h = hz[0]
            hx, hy = h['x'] >> 8, h['y'] >> 8
            k = driver.KEY.UP if y <= hy else driver.KEY.DOWN
            if y < 40:
                k = driver.KEY.DOWN
            k |= driver.KEY.LEFT if x <= hx else driver.KEY.RIGHT
            self.mash = 0
            return k
    # progress = the fought type's total HP going down; none for 150 frames: next approach side, after the four
    # sides the next-nearest target (an enemy inside another's body cannot be reached from every side)
    hp = sum(d['hp1024'] for d in ens)
    st = getattr(self, '_bt', None)
    if st is None or st[0] != self.k:
        st = [self.k, 0, hp, 0, 0]
    if hp < st[2]:
        st[2], st[3] = hp, 0
    else:
        st[2] = hp
        st[3] += 1
    if st[3] > 150:
        st[3] = 0
        st[1] += 1
        if st[1] % 4 == 0:
            st[4] += 1
    self._bt = st
    order = sorted(ens, key=lambda d: abs((d['x'] >> 8) - x) + abs((d['y'] >> 8) - y))
    en = order[st[4] % len(order)]
    ex, ey = en['x'] >> 8, en['y'] >> 8
    sides = [(-1 if x < ex else 1, 0), (1 if x < ex else -1, 0), (0, -1), (0, 1)]
    st[1] %= 4
    sx, sy = sides[st[1]]
    tx, ty = ex + sx * 18, ey + sy * 14
    k = 0
    if abs(x - tx) > 2 or abs(y - ty) > 2:
        if tx > x + 2:
            k |= driver.KEY.RIGHT
        elif tx < x - 2:
            k |= driver.KEY.LEFT
        if ty > y + 2:
            k |= driver.KEY.DOWN
        elif ty < y - 2:
            k |= driver.KEY.UP
        self.mash = 0
        return k
    # facing: 0 down, 1 up, 2 left, 3 right
    if sx:
        want, fdir = (driver.KEY.RIGHT, 3) if sx < 0 else (driver.KEY.LEFT, 2)
    else:
        want, fdir = (driver.KEY.DOWN, 0) if sy < 0 else (driver.KEY.UP, 1)
    if s['p'][2] != fdir:
        return want
    self.mash += 1
    if self.mash % 10 < 2:
        return driver.KEY.A
    return 0


def step(self, e):
    g = self.goals[self.k]
    if g[0] == 'bosst':
        return bosst(self, e, g)
    if g[0] == 'bossx':
        s = status(e)
        here = f"{s['area'][0]}-{s['area'][1]}"
        if getattr(self, '_bx', None) != (self.k, here):
            sbdriver.avoid_exits(here, None)
            self._bx = (self.k, here)
        idx = self.k
        self.goals[idx] = ('boss', g[1])
        try:
            r = _step(self, e)
        finally:
            self.goals[idx] = g
        if self.k != idx:
            sbdriver.AVOID.clear()
            self._bx = None
        elif r and s['p'] is not None:
            # standing in an exit's rect (an arrival spot, a knock-back): never press its push direction
            import mapgraph
            G, _ = mapgraph.load_cached()
            px, py = s['p'][0], s['p'][1]
            for x in G.get(here, []):
                q = x[1]
                if q['x'] - 4 <= px < q['x'] + q['w'] + 4 and q['y'] - 4 <= py < q['y'] + q['h'] + 4:
                    r &= ~{'up': 64, 'down': 128, 'left': 32, 'right': 16}[x[6]]
        return r
    return _step(self, e)


driver.Driver.step = step
