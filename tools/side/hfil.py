"""The HFIL corridor (18-1 .. 18-11) and the way to Janemba, played by the real game with the ship chapter's
closed-loop driver (re/buu/ship/ship_driver.py: nav with room fights, healing from the inventory, live walk grid).

usage: hfil.py START.ss "GOALS" [--save OUT.ss] [--cap N] [--keys out.json] [--quiet]
       hfil.py --route [--from MARK] [--to MARK] [--keys out.json]      (the ROUTE below; marks -> work/side/h_<name>.ss)
"""
import ast
import json
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parent.parent
OUT = ROOT / 'work' / 'side'
sys.path.insert(0, str(ROOT / 're' / 'buu' / 'ship'))
sys.path.insert(0, str(ROOT / 'tools' / 'story_playthrough'))
sys.path.insert(0, str(ROOT / 'tools' / 'field'))
sys.path.insert(0, str(HERE))
from shipdrv import ShipDriver, enemies, ai_state, status, KEY  # noqa: E402,F401
from route import start_emu  # noqa: E402


class SideDriver(ShipDriver):
    """ShipDriver + side-content goals:
      ('talkto', x, y, dir)   walk to (x, y), face dir, A, play the event through
      ('exp', level)          fight on in the current area until the active character has the level (a 'nav' with
                              fights re-enters rooms; this one only waits for the room's enemies)
    """
    # Janemba's hands (0x39) come up under the player wherever he stands: not a room target (50 HP, they sink again)
    AVOID = ShipDriver.AVOID + (0x39,)
    # enemies that come for the player during walks: fought when they get close (Poltergeist, Living Dead, Lich)
    CHASERS = (0x5d, 0x42, 0x41)

    def nav(self, e, s, g, inner=False):
        """ShipDriver.nav with the exit's approach point taken where the player can walk to (HFIL's edge exits span
        two levels of a cliff: the rect's centre can be on the other one)"""
        import sidenav as navm
        plain = navm.approach

        def approach(rect, push):
            x0, y0 = plain(rect, push)
            if s['p'] is None:
                return x0, y0
            self.reachable(e, s, 0, 0)
            if self.reachable(e, s, x0, y0):
                return x0, y0
            dx, dy = navm.DIRS.get(push, (0, 0))
            best = None
            if dx:      # a vertical rect: slide along y
                cands = [(x0, y) for y in range(rect['y'] + 4, rect['y'] + rect['h'] - 8, 8)]
            else:
                cands = [(x, y0) for x in range(rect['x'] + 4, rect['x'] + rect['w'] - 12, 8)]
            px, py = s['p'][0], s['p'][1]
            for c in cands:
                if self.reachable(e, s, c[0], c[1]):
                    d = abs(c[0] - px) + abs(c[1] - py)
                    if best is None or d < best[0]:
                        best = (d, c)
            return best[1] if best else (x0, y0)
        navm.approach = approach
        try:
            return super().nav(e, s, g, inner)
        finally:
            navm.approach = plain

    def goto(self, e, s, g):
        """('goto', 'W-A', x, y[, fight]): walk through the maze of areas (maze.plan: walk components + exits) to the
        footprint position (x, y) of area W-A; with fight (default) every area entered is cleared first"""
        import maze
        if s['scene'] > 0:
            return self.mash_a()
        if s['p'] is None:
            return 0
        cur = f"{s['area'][0]}-{s['area'][1]}"
        fight = g[4] if len(g) > 4 else True
        if cur != getattr(self, '_gt_area', None):
            self._gt_area = cur
            self.skip = set()
            self.tgt = {}
            self.fight_done = False
            self.path = None
            self._gt_push = None
        if self.maybe_heal(e, s):
            return self.heal_step(e) or 0
        if fight and not self.fight_done:
            self.in_nav = True
            k = self.fightp(e, s, ('fightp', 10 ** 9, 99999, 6))
            if not self.fight_done:
                return k
        x, y = s['p'][0], s['p'][1]
        hops = maze.plan(cur, (x, y), g[1], (g[2], g[3]))
        if hops is None:
            raise RuntimeError(f'goto: no route from {cur} {(x, y)} to {g[1:4]}')
        if not hops:
            if abs(x - g[2]) <= 3 and abs(y - g[3]) <= 3:
                self.in_nav = False
                self._gt_area = None
                self.next_goal()
                return None
            return self.walk_keys(e, s, g[2], g[3])
        tiles = hops[0][2]
        # the standing tile nearest to the player; pushing is sticky once started
        if getattr(self, '_gt_push', None) is None:
            t, push, rect = min(tiles, key=lambda q: abs(q[0][0] * 8 - x) + abs(q[0][1] * 8 - y))
            tx, ty = t[0] * 8, t[1] * 8
            if abs(x - tx) > 4 or abs(y - ty) > 4:
                return self.walk_keys(e, s, tx, ty)
            self._gt_push = (self.t, push)
        t0, push = self._gt_push
        if self.t - t0 > 90:
            self._gt_push = None
        return {'up': KEY.UP, 'down': KEY.DOWN, 'left': KEY.LEFT, 'right': KEY.RIGHT}[push]

    # spike strips (script 507: 20 HP and a knock-back when the player's position is inside): walks keep out
    SPIKES = {(18, 10): [(65, 25, 175, 39), (433, 25, 111, 39), (569, 121, 47, 391), (113, 585, 399, 39),
                         (17, 409, 44, 135), (17, 89, 44, 155)],
              (18, 5): [(281, 9, 39, 279)], (18, 7): [(281, -7, 39, 299)]}

    def live_walk(self, e):
        super().live_walk(e)
        g = self.grid
        rects = self.SPIKES.get(tuple(self.grid_key[:2])) if self.grid_key else None
        if rects and not getattr(g, 'spiked', False):
            import maze10
            bad = maze10.forbidden(g, rects)
            inner = g.bfs

            def bfs(start, goal, fw=2, fh=1, blocked=()):
                return inner(start, goal, fw, fh, (set(blocked) | bad) - {start})
            g.bfs = bfs
            g.spiked = True

    def hand_guard(self, e, s):
        """a Janemba Hand (0x39, 50 HP) up beside the player: punch it (it strikes again and again otherwise)"""
        if s['p'] is None or s['scene'] > 0:
            return None
        x, y = s['p'][0], s['p'][1]
        near = [o for o in enemies(e) if o[5] == 0x39 and abs(o[1] - x) <= 26 and abs(o[2] - y) <= 22
                or o[5] in self.CHASERS and abs(o[1] - x) + abs(o[2] - y) <= 64 and o[0] not in self.skip]
        if not near:
            return None
        keep = (self.AVOID, self.in_nav, self.fight_done, self.idle)
        self.AVOID = tuple(t for t in keep[0] if t != 0x39)
        self.in_nav = True
        try:
            k = self.fightp(e, s, ('fightp', 10 ** 9, 70, 6))
        finally:
            self.AVOID, self.in_nav, self.fight_done, self.idle = keep
        return k

    def step(self, e):
        g = self.goals[self.k]
        if g[0] in ('walk', 'goto', 'exitat', 'talkto') or g[0] == 'it' and getattr(self, '_it', None) is None:
            k = self.hand_guard(e, status(e))
            if k:
                return k
        if g[0] == 'goto':
            return self.goto(e, status(e), g)
        if g[0] == 'it':
            # ('it', rx, ry, rw, rh[, wait]): Instant Transmission (the selected skill) onto the switch trigger with
            # that rect: hold B (the cursor comes up at the player's body centre, (x + 8, y - 12)), steer it - the
            # game's cursor: v = v * 3 / 4 + 1 px per frame held, per axis - into the pick box (x0 + 15, y0 + 7, x1,
            # y1), x first, then y; it picks when the point is inside; release. Goku jumps to the switch, its script
            # runs, he jumps back: done 6 frames after he is back.
            s = status(e)
            st = getattr(self, '_it', None)
            if s['scene'] > 0 and not st:   # a level-up's popups (an Instant Transmission kill)
                return self.mash_a()
            if st is None:
                if s['p'] is None:
                    return 0
                x, y = s['p'][0], s['p'][1]
                bx0, by0, bx1, by1 = g[1] + 15, g[2] + 7, g[1] + g[3], g[2] + g[4]
                cx, cy = (x + 8) * 256, (y - 12) * 256

                def hold(c, lo, hi):
                    """frames to hold toward [lo, hi) so that the cursor comes to rest inside (v decays by 3/4)"""
                    if lo * 256 <= c < hi * 256:
                        return 0, 0
                    d = 1 if c < lo * 256 else -1
                    mid = (lo + hi) * 128
                    best = None
                    for n in range(1, 80):
                        v, pos = 0, c
                        for _ in range(n):
                            v = int(v * 3 / 4) + 256 * d
                            pos += v
                        for _ in range(40):
                            v = int(v * 3 / 4)
                            pos += v
                        if best is None or abs(pos - mid) < best[0]:
                            best = (abs(pos - mid), n)
                    return best[1], d
                nx, dx = hold(cx, bx0, bx1)
                ny, dy = hold(cy, by0, by1)
                prog = [KEY.B] * 20
                prog += [KEY.B | (KEY.RIGHT if dx > 0 else KEY.LEFT)] * nx
                prog += [KEY.B] * (10 if nx else 0)
                prog += [KEY.B | (KEY.DOWN if dy > 0 else KEY.UP)] * ny
                prog += [KEY.B] * 12
                st = self._it = prog
                self._it_from = (x, y)
                self._it_away = False
                self._it_t = 0
            if st:
                return st.pop(0)
            # released: gone to the switch (the position changes), back at the start -> done 6 frames later
            self._it_t += 1
            if s['p'] is not None:
                at = abs(s['p'][0] - self._it_from[0]) <= 12 and abs(s['p'][1] - self._it_from[1]) <= 12
                if not at:
                    self._it_away = True
                elif self._it_away and s['scene'] == 0:
                    self._it_back = getattr(self, '_it_back', 0) + 1
                    if self._it_back >= 6:
                        self._it = None
                        self._it_back = 0
                        self.next_goal()
                        return None
            if self._it_t > (g[5] if len(g) > 5 else 420):
                self._it = None
                self._it_back = 0
                self.next_goal()
                return None
            return 0
        if g[0] == 'maze10':
            # ('maze10', x, y): HFIL 18-10's gate maze to (x, y): maze10.plan from where the player is (the wall state
            # read from the live grid) every step: walk to the plan's next spot; there, a switch toggle (Instant
            # Transmission) runs as its own goal and this one resumes after it
            import maze10
            s = status(e)
            if s['scene'] > 0:          # a level-up's popups
                return self.mash_a()
            if s['p'] is None:
                return 0
            x, y = s['p'][0], s['p'][1]
            raw = e.read(e.r32(0x030019A4 + 0xA8), 32 * 80)
            state = (raw[35 * 32 + (41 >> 3)] >> (41 & 7)) & 1
            pl = maze10.plan((x, y), (g[1], g[2]), state)
            if pl is None:
                raise RuntimeError(f'maze10: no route from {(x, y)} state {state}')
            tx, ty = pl[0][1], pl[0][2]
            if abs(x - tx) > 3 or abs(y - ty) > 3:
                if self.stuck > 60:
                    self.stuck = 0
                    self.path = None
                return self.walk_keys(e, s, tx, ty)
            if len(pl) > 1:
                self.goals[self.k:self.k + 1] = [('it', 301, 309, 31, 23), g]
                self.t = 0
                return 0
            self.next_goal()
            return None
        if g[0] == 'openwall':
            # ('openwall', 'W-A', [(tx, ty), ...]): a switch opened these tiles (the maze planner's static grid)
            import maze
            maze.open_wall(g[1], g[2])
            self.next_goal()
            return None
        if g[0] == 'exitat':
            # ('exitat', x, y, push): walk to (x, y) and hold `push` until the area changes and the player is back
            s = status(e)
            if getattr(self, '_ex', None) is None:
                self._ex = s['area'][:2]
            if s['area'][:2] != self._ex:
                if s['p'] is not None and s['scene'] == 0:
                    self._ex = None
                    self._ex_push = False
                    self.next_goal()
                    return None
                return 0
            if s['scene'] > 0:
                return self.mash_a()
            if s['p'] is None:
                return 0
            if self.maybe_heal(e, s):
                return self.heal_step(e) or 0
            x, y = s['p'][0], s['p'][1]
            k = {'U': KEY.UP, 'D': KEY.DOWN, 'L': KEY.LEFT, 'R': KEY.RIGHT}[g[3]]
            # (pushing is sticky: the exit can lie many px beyond the walk's target)
            if not getattr(self, '_ex_push', False):
                if abs(x - g[1]) > 6 or abs(y - g[2]) > 6:
                    return self.walk_keys(e, s, g[1], g[2])
                self._ex_push = True
            return k
        if g[0] == 'talkto':
            s = status(e)
            if s['scene'] > 0:
                self.idle = 0
                self._talked = True
                return self.mash_a()
            if getattr(self, '_talked', False):
                self.idle += 1
                if self.idle >= 20:
                    self._talked = False
                    self.next_goal()
                    return None
                return 0
            if s['p'] is None:
                return 0
            x, y, pf = s['p']
            if abs(x - g[1]) > 3 or abs(y - g[2]) > 3:
                return self.walk_keys(e, s, g[1], g[2])
            want = {'D': 0, 'U': 1, 'L': 2, 'R': 3}[g[3]]
            if pf != want:
                return (KEY.DOWN, KEY.UP, KEY.LEFT, KEY.RIGHT)[want]
            self.mash += 1
            return KEY.A if self.mash % 20 < 2 else 0
        return super().step(e)


W4 = [(x, 32) for x in range(11, 19)]                      # 18-4's wall (x 88..151, y 256): hidden by 18-2's switch
ROCK = [(x, y) for x in range(50, 54) for y in range(7, 10)]  # 18-10's boulder (400..432, 56..80): punched from the east
A_, B_, L_, R_, U_, D_ = 1, 2, 32, 16, 64, 128
# on a save point's pad (the player's position inside its trigger rect), A: menu -> Switch Characters -> LEFT / RIGHT -> A (re/buu/ship/ship_route.py charsel)
def charsel(moves):
    d = R_ if moves > 0 else L_
    k = [(A_, 3, 40), (D_, 3, 12), (A_, 3, 80)] + [(d, 3, 20)] * abs(moves) + [(A_, 3, 60)]
    return [('face', 'U'), ('keys', k), ('events',)]


ROUTE = [
    ('mark', 'hfil'),
    # 18-2: the Living Dead, then the switch on the ledge: Instant Transmission (skill slot 2) from below; its script
    # shows 18-4 and hides the wall there (flag 181)
    ('goto', '18-2', 272, 100), ('face', 'U'), ('skill', 2), ('it', 257, 25, 31, 23), ('wait', 30),
    ('openwall', '18-4', W4), ('mark', 'switch'),
    # 18-3 -> 18-4's pocket -> 18-5 -> 18-4 -> (the opened wall) -> 18-3's upper floor -> 18-4 -> 18-6 .. 18-9 -> 18-10
    ('goto', '18-10', 336, 40), ('mark', 'maze'),
    # 18-10: the gate maze (eight switch toggles by Instant Transmission), the ring's lanes beside the spikes; the
    # boulder between the ring and the entry breaks from its east side (punches; flag 284: the shortcut back); the
    # exit to 18-11
    ('maze10', 440, 66), ('walk', 433, 66), ('face', 'L'), ('keys', [(A_, 2, 10)] * 4), ('wait', 20),
    ('openwall', '18-10', ROCK), ('walk', 592, 72), ('exitat', 600, 72, 'R'), ('wait', 30), ('mark', 'statues'),
    # 18-11: four Vegetas; the real one stands at (208, 128) (s446: he joins, flag 123)
    ('talkto', 208, 142, 'U'), ('wait', 30), ('mark', 'vegeta'),
    # back to 18-1 (the boulder is open now), its save point: Vegeta for the level-100 gate of 18-12 (s517, flag 183),
    # then Goku again
    # (the boulder is back after the reload: punch it again from the east)
    ('exitat', 24, 72, 'L'), ('walk', 433, 66), ('face', 'L'), ('keys', [(A_, 2, 10)] * 4), ('wait', 20),
    # (no room fights on the way back: a fight beside an exit pushes through it; the enemies near the path are
    # still fought, hand_guard)
    # (the menu's row: Goku, Gohan, Goten, Trunks, Vegeta - four steps)
    ('goto', '18-1', 104, 272, False), ('walk', 107, 240), *charsel(4), ('wait', 20), ('mark', 'veg1'),
    # (A with the position inside the gate's rect (73, 33, 47, 15): up against it)
    ('goto', '18-12', 88, 52, False), ('keys', [(U_, 12, 5), (A_, 3, 60)]), ('events',), ('wait', 30),
    ('mark', 'gate'),
    # 18-13: s448 (Pikkon) on the way in, then its save point's pad. (No switch back: in the constructed start
    # state the menu has Goku greyed out - to be checked against the real-route state.)
    ('exitat', 96, 28, 'U'), ('wait', 20), ('events',), ('wait', 30), ('walk', 120, 170), ('wait', 20),
    ('mark', 'h13'),
]


def run(st, goals, save=None, cap=10 ** 7, keys_out=None, quiet=False, marks=True):
    e = start_emu(str(st))
    d = SideDriver(goals)

    log = []

    def on_mark(name, ee):
        if marks:
            ee.save_file(str(OUT / f'h_{name}.ss'))
            # the keys from this run's start state up to the mark (chain the legs: hfilkeys.py)
            (OUT / f'h_{name}.keys.json').write_text(json.dumps({'from': Path(str(st)).name, 'keys': list(log)}))
        print('mark', name, status(ee), flush=True)
    d.on_mark = on_mark
    prev = None
    f = 0
    err = None
    try:
        while not d.done() and f < cap:
            k = d.keys(e)
            e.set_keys(k)
            e.run(1)
            log.append(k)
            s = status(e)
            key = (tuple(s['area']), s['scene'] > 0, d.k)
            if key != prev and not quiet:
                print(f, 'goal', d.k, d.goals[d.k] if not d.done() else None, s, flush=True)
            prev = key
            f += 1
            if s['area'][:2] == [0, 2]:
                raise RuntimeError('game over')
    except Exception as x:  # report and still save
        err = x
    print('end', f, 'done' if d.done() else f'NOT DONE ({err!r})', status(e), 'enemies', [(hex(o[5]), o[1], o[2], o[4] >> 10) for o in enemies(e)])
    e.set_keys(0)
    if save:
        e.save_file(str(save))
    if keys_out:
        Path(keys_out).write_text(json.dumps(log))
    return e


if __name__ == '__main__':
    a = sys.argv
    opt = lambda n: a[a.index(n) + 1] if n in a else None  # noqa: E731
    if '--route' in a:
        goals = list(ROUTE)
        st = OUT / 'j_hfil.ss'
        if opt('--from'):
            i = goals.index(('mark', opt('--from')))
            import maze
            for g in goals[:i]:         # the walls opened before the mark
                if g[0] == 'openwall':
                    maze.open_wall(g[1], g[2])
            goals = goals[i + 1:]
            st = OUT / f"h_{opt('--from')}.ss"
        if opt('--to'):
            goals = goals[:goals.index(('mark', opt('--to'))) + 1]
        run(st, goals, save=OUT / 'h_end.ss', cap=int(opt('--cap') or 10 ** 7), keys_out=opt('--keys'), quiet='--quiet' in a)
    else:
        run(a[1], eval(a[2]), opt('--save'), int(opt('--cap') or 20000), opt('--keys'), '--quiet' in a, marks=False)
