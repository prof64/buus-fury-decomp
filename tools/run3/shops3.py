"""Run 3's shop / zero-zenie / item-use stretch (engine/docs/segments.md "Run 3", Earth): the goals of segment
`e3_shops` and the driver goals they need (ShopDriver = Run3Driver + the kinds below). Played at the Hercule City
Z-Mart (0-91; the clerk's s028), as run 2's stop after Videl.

  ('shopenter', choice)   from the field facing the clerk: A through the greeting, `choice` = 0 Buy / 1 Sell on the
                          first choice box, (Buy) 0 Items / 1 Equipment on the second, until that list screen is up
  ('buyall',)             on the Buy screen: every row once, top to bottom, while the zenie covers its price; then
                          the cheapest row until the zenie is under its price (exactly 0 when the prices allow)
  ('buyrefused',)         on the Buy screen: the count's LEFT at 1 and RIGHT past the money (sound 38 both), then A
                          on a row the zenie cannot pay (sound 38)
  ('sellall',)            on the Sell screen: A on every row once, top to bottom (rows with one copy go away)
  ('listout',)            B until the list screen is gone (back to the clerk's choice box)
All closed-loop on the emulator state: the mode object (0x03000F90) and its vtable, the selected row (+0x114), the
shop list (count 0x03003174, 8-byte entries from 0x03003178: u8 category, u8 stock, u16 item, u32 price), the zenie
(GameData +0x4F4) and the supply counts (GameData +2)."""
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
for d in ('tools/story_playthrough', 'tools/field', 'tools/run2', 'tools/run3'):
    if str(ROOT / d) not in sys.path:
        sys.path.insert(0, str(ROOT / d))
from run3_driver import Run3Driver  # noqa: E402
from route import KEY, status  # noqa: E402

U, D, L, R, A, B, S = KEY.UP, KEY.DOWN, KEY.LEFT, KEY.RIGHT, KEY.A, KEY.B, KEY.START
GD = 0x03001068
MODE = 0x03000F90
VT_BUY = 0x080499A8
VT_SELL = 0x08049B88
VT_FIELD = 0x080494AC
VT_STATUS = 0x0804AC88
VT_INVENTORY = 0x08049D48
# a choice box taking input (the text box mode with its cursor)
VT_CHOICE = 0x0804A920
SHOP_N = 0x03003174
OBJW = 0x03001C4C
VT_TEXTBOX = 0x08049F60
# frames from a choice box's start to its answer (text speed 0 types the clerk's greeting in ~380)
BOX_WAIT = 450


def mode_vt(e):
    m = e.r32(MODE)
    return e.r32(m) if 0x02000000 <= m < 0x04000000 else 0


def supplies(e):
    return [e.r8(GD + 2 + k) for k in range(83)]


def zenie(e):
    return e.r32(GD + 0x4F4)


def shop_list(e):
    n = min(e.r32(SHOP_N), 32)
    out = []
    for k in range(n):
        a = SHOP_N + 4 + 8 * k
        out.append({'cat': e.r8(a), 'stock': e.r8(a + 1), 'item': e.r16(a + 2), 'price': e.r32(a + 4)})
    return out


def choice_cursor(e):
    """the cursor object of a text box waiting on a choice (0: none): the box (vt 0x08049F60) in the world list,
    its +0x48"""
    n = min(e.r32(OBJW + 4), 256)
    for k in range(n):
        o = e.r32(OBJW + 8 + 4 * k)
        if 0x02000000 <= o < 0x04000000 and e.r32(o) == VT_TEXTBOX:
            c = e.r32(o + 0x48)
            return c if 0x02000000 <= c < 0x04000000 else 0
    return 0


def shop_sel(e):
    return e.r32(e.r32(MODE) + 0x114)


class ShopDriver(Run3Driver):
    def tap(self, key, every=24):
        """`key` 3 frames, then nothing until `every` frames passed (the list's buttons fire on release)"""
        return key if self.t % every < 3 else 0

    def step(self, e):
        g = self.goals[self.k]
        kind = g[0]
        if kind == 'shopenter':
            # each choice box is answered BOX_WAIT frames after it opens, long enough for the slowest text speed to
            # type it out (the game's text box object is not in the world list; no A while it types: the input ring
            # keeps a recent press and the choice takes it): talk (A in the field), then per box BOX_WAIT quiet
            # frames, DOWN for its second row, 20 frames, A; then the next box or the list screen
            want = VT_SELL if g[1] == 1 else VT_BUY
            picks = [g[1] == 1] + ([] if g[1] == 1 else [len(g) > 2 and g[2] == 1])
            st = getattr(self, 'enter', None)
            if st is None:
                st = self.enter = {'box': 0, 't': 0, 'talk': 0}
            m = mode_vt(e)
            if self.t > 3000:
                raise RuntimeError(f'shopenter: no list screen ({st}) {status(e)}')
            if m == want:
                self.idle += 1
                if self.idle >= 20:
                    self.enter = None
                    self.next_goal()
                    return None
                return 0
            self.idle = 0
            if status(e)['scene'] == 0:
                st['talk'] += 1
                return A if st['talk'] % 60 < 3 else 0
            if st['box'] >= len(picks):
                return 0
            st['t'] += 1
            t = st['t'] - BOX_WAIT
            down = picks[st['box']]
            if t < 0:
                return 0
            if down and t < 3:
                return D
            if t < 23:
                return 0
            if t < 26:
                return A
            if t >= 26:
                st['box'] += 1
                st['t'] = 0
            return 0
        if kind == 'buyall':
            lst = shop_list(e)
            z = zenie(e)
            if mode_vt(e) != VT_BUY:
                raise RuntimeError(f'buyall: not on the Buy screen {status(e)}')
            st = getattr(self, 'buy', None)
            if st is None:
                st = self.buy = {'i': 0, 'z': z, 'w': 0, 'pass': 0}
            # a purchase happened (the zenie fell): the next row (first pass) / the cheapest again (second)
            if z < st['z']:
                st['z'] = z
                st['i'] += 1 if st['pass'] == 0 else 0
                st['mv'] = None
                self.t = 0
                return 0
            cheapest = min(range(len(lst)), key=lambda k: lst[k]['price'])
            if st['pass'] == 0 and st['i'] >= len(lst):
                st['pass'] = 1
            target = st['i'] if st['pass'] == 0 else cheapest
            if st['pass'] == 1 and z < lst[cheapest]['price']:
                self.buy = None
                self.next_goal()
                return None
            if st['pass'] == 0 and lst[target]['price'] > z:
                st['i'] += 1
                return 0
            sel = shop_sel(e)
            mv = st.get('mv')
            if mv is not None:
                # a tap in flight: its 3 frames, then until the row moved (or 40 frames), then 10 quiet frames
                mv['t'] += 1
                if mv['t'] <= 3:
                    return mv['k']
                if sel == mv['from'] and mv['t'] < 40:
                    return 0
                if mv.get('q', 0) < 10:
                    mv['q'] = mv.get('q', 0) + 1
                    return 0
                st['mv'] = None
                self.t = 0
                return 0
            if sel != target:
                st['mv'] = {'k': D if sel < target else U, 't': 0, 'from': sel}
                return 0
            # on the row: A, then wait until the zenie fell
            if self.t > 2000:
                raise RuntimeError(f'buyall: no purchase {target} {lst[target]} zenie {z} {status(e)}')
            return self.tap(A, 40)
        if kind == 'buyrefused':
            prog = [(L, 3), (0, 30), (R, 3), (0, 30), (R, 3), (0, 30)]
            # the most expensive row (never affordable by now), then A
            lst = shop_list(e)
            dear = max(range(len(lst)), key=lambda k: lst[k]['price'])
            t = self.t
            for k, n in prog:
                if t < n:
                    return k
                t -= n
            sel = shop_sel(e)
            if sel != dear:
                return self.tap(D if sel < dear else U, 20)
            if getattr(self, 'refused', 0) >= 40:
                self.refused = 0
                self.next_goal()
                return None
            self.refused = getattr(self, 'refused', 0) + 1
            return A if self.refused <= 3 else 0
        if kind == 'sellall':
            if mode_vt(e) != VT_SELL:
                raise RuntimeError(f'sellall: not on the Sell screen {status(e)}')
            st = getattr(self, 'sell', None)
            n = e.r32(e.r32(MODE) + 0x11C) if False else None
            sup = supplies(e)
            z = zenie(e)
            if st is None:
                st = self.sell = {'z': z, 'done': 0, 'rows': None}
            # the list's length: the sell screen keeps its own (owned supplies and spare equipment); sell the selected
            # row, then step down; stop when a step down no longer moves (the last row)
            sel = shop_sel(e)
            if z > st['z']:
                st['z'] = z
                st['done'] += 1
                st['moved'] = sel
                st['phase'] = 'down'
                self.t = 0
                return 0
            ph = st.get('phase', 'sell')
            if ph == 'down':
                if self.t < 30:
                    return D if self.t < 3 else 0
                if shop_sel(e) == st['moved'] and st['done'] > 0:
                    # (a row that went away moved the next one up: sell again where we are, unless nothing is left)
                    pass
                st['phase'] = 'sell'
                st['tries'] = 0
                self.t = 0
                return 0
            st['tries'] = st.get('tries', 0) + 1
            if st['tries'] > 200:
                # A did not sell (the list is empty or the row is not for sale): finished
                self.sell = None
                self.next_goal()
                return None
            if st['done'] >= g[1] if len(g) > 1 else False:
                self.sell = None
                self.next_goal()
                return None
            return self.tap(A, 60)
        if kind == 'invuse':
            # ('invuse', row, action): pause -> Inventory -> supply row `row` -> its menu -> Use (action 0) / Drop
            # (1), then out. Closed-loop on the mode: START (every 60 frames) until the Status page, R (every 40)
            # until the Inventory page, DOWN x row, A (the item's menu), DOWN x action, A, until the supply counts
            # change (or 180 frames: a Use the game refuses), then B until the field runs
            st = getattr(self, 'inv', None)
            if st is None:
                st = self.inv = {'ph': 0, 't': 0, 'sup': supplies(e)}
            st['t'] += 1
            m = mode_vt(e)
            if self.t > 4000:
                raise RuntimeError(f'invuse: stuck ({st["ph"]}, mode {m:#x}) {status(e)}')
            if st['ph'] == 0:
                if m == VT_STATUS:
                    st['ph'], st['t'] = 1, 0
                    return 0
                sx = status(e)
                if m != VT_FIELD or sx['p'] is None or sx['scene'] > 0:
                    st['t'] = 0
                    return 0
                return S if st['t'] % 60 == 20 else 0
            if st['ph'] == 1:
                if m == VT_INVENTORY:
                    st['ph'], st['t'] = 2, 0
                    return 0
                return KEY.R if st['t'] % 40 == 20 else 0
            if st['ph'] == 2:
                prog = []
                for _ in range(g[1]):
                    prog += [(D, 3), (0, 20)]
                prog += [(0, 20), (A, 3), (0, 30)]
                for _ in range(g[2]):
                    prog += [(D, 3), (0, 20)]
                prog += [(A, 3), (0, 10)]
                t = st['t']
                for k, n in prog:
                    if t < n:
                        return k
                    t -= n
                st['ph'], st['t'] = 3, 0
                return 0
            if st['ph'] == 3:
                if supplies(e) != st['sup'] or st['t'] > 180:
                    self.inv = None
                    self.goals[self.k] = ('bout',)
                    self.t = 0
                    return 0
                # (a confirmation box: A again)
                return A if st['t'] % 40 == 20 else 0
        if kind == 'listout':
            if mode_vt(e) not in (VT_BUY, VT_SELL):
                self.idle += 1
                if self.idle >= 30:
                    self.next_goal()
                    return None
                return 0
            self.idle = 0
            return self.tap(B, 40)
        return super().step(e)


# pause -> Inventory -> the first supply -> Use (earth_route.USE_FIRST_SUPPLY)
USE_FIRST = 'S:3,0:60,RS:5,0:40,A:6,0:40,A:6,0:40,B:6,0:30,S:6,0:60'
# pause -> Inventory -> the second supply -> its menu -> the second entry (Drop)
DROP_SECOND = ('keys', [(S, 2), (0, 60), (KEY.R, 3), (0, 40), (D, 3), (0, 30), (A, 3), (0, 40), (D, 3), (0, 30), (A, 3),
                        (0, 60), (B, 3), (0, 40), (S, 2), (0, 100)])

# the goals, from 3-1 (Hercule City) after Videl: into the Z-Mart, the clerk; Buy -> Items: every row once while the
# money lasts, then the cheapest down to what no row costs (0 when the prices allow), the refusals; Buy -> Equipment:
# the refusal; Sell: two rows; out to 3-1; the Inventory: a supply used, one dropped
SHOPS3_GOALS = [
    ('walk', 136, 82), ('hold', U, 30), ('area', 0, 91), ('wait', 30), ('walk', 100, 110), ('hold', L, 20),
    ('shopenter', 0, 0), ('buyall',), ('buyrefused',), ('listout',),
    ('shopenter', 0, 1), ('buyrefused',), ('listout',),
    ('bout',),
    ('hold', L, 10), ('shopenter', 1), ('sellall', 2), ('listout',),
    ('bout',),
    ('walk', 116, 200), ('hold', D, 30), ('area', 3, 1), ('wait', 30),
    ('invuse', 0, 0), ('wait', 20), ('invuse', 0, 1), ('wait', 30),
]
