"""Run 2's Super Buu + Kid Buu chapters (after Janemba, HFIL 18-13 -> the credits -> the title): the same story as
run 1 (tools/superbuu/sbroute.py ROUTE + the grinds of work/ship/sb), played differently to reach code paths run 1
never did. `goals_run2(e)` builds the goal list from the start state; `SbDriver2` drives it.

SbDriver2 = re/buu/ship/ship_route2.ShipDriver2 (the ship's run-2 driver on run 1's frozen ShipDriver: nav, grind,
auto-heal from the Inventory, the save -> death -> title -> Continue retry) + tools/superbuu/sbdriver's goals (boss,
eventsw, itbuu, evade, exitto, node, prog, walke, fastmash: monkeypatched into the base Driver) plus:
  ('fly', x, y)          sbdriver's world-map flight to a point (ShipDriver's own 'fly' takes a rect: 5 values)
  ('luck',)              punch the air `tries` times (the retry stretch's game overs so far): each punch draws the
                         RNG, so a fight lost after a Continue is not replayed identically
  ('autoheal', on)       boss / itbuu / beam fights eat the best healing supply below 35 % HP (run 1 never healed)
Input only - no RAM writes. The in-game Save writes the cartridge's EEPROM, which .ss files do not carry: replay the
whole part from its start in one emulator session (marks between a save and a Continue are not resume points)."""
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'tools' / 'superbuu'))
sys.path.insert(0, str(ROOT / 're' / 'buu' / 'ship'))
import sbdriver  # noqa: E402,F401  (patches the base Driver's step with the Super Buu goals)
import driver  # noqa: E402
from ship_route2 import (ShipDriver2, keys, SAVE, DEATH_CONTINUE, RESET_CONTINUE, use_item, equip2,  # noqa: E402,F401
                         split2, KEY, status, level_of, alloc_keys)
from ship_route import charsel, rest_at  # noqa: E402,F401

U, D, L, R, A, B = KEY.UP, KEY.DOWN, KEY.LEFT, KEY.RIGHT, KEY.A, KEY.B
S, E, LS, RS = KEY.START, KEY.SELECT, KEY.L, KEY.R


class SbDriver2(ShipDriver2):
    autoheal = False
    _calls = 0

    def keys(self, e):
        self._calls = 0
        return super().keys(e)

    def step(self, e):
        # a goal that keeps returning None without moving on would spin the frame forever
        self._calls += 1
        if self._calls > 2000:
            raise RuntimeError(f'goal {self.k} {self.goals[self.k]} spins without a key: {status(e)}')
        g = self.goals[self.k]
        kind = g[0]
        if kind == 'fly' and len(g) == 3:
            return driver.Driver.step(self, e)
        if kind == 'luck':
            n = self.tries
            self.next_goal()
            if n:
                for _ in range(n):
                    self.prog += [A] * 2 + [0] * 34
                return self.prog.pop(0)
            return None
        if kind == 'allocnow':
            # ('allocnow', fn): the active character's unspent points n -> fn(n) = [STR, POW, END, SPD], spent now
            c = e.r8(0x03001068)
            n = e.r16(0x03001560 + 0x68 * c + 0x2C)
            self.next_goal()
            if n:
                for m, on, off in alloc_keys([(0, g[1](n))])[1]:
                    self.prog += [m] * on + [0] * off
                return self.prog.pop(0)
            return None
        if kind == 'bout':
            # B every 30 frames until the field runs again (no event, a leader) for 30 frames (shop lists: an A there
            # would buy)
            s = status(e)
            if s['scene'] == 0 and s['p'] is not None:
                self.idle += 1
                if self.idle >= 30:
                    self.next_goal()
                    return None
                return 0
            self.idle = 0
            if self.t > 3000:
                raise RuntimeError(f'bout: still in a menu after 3000 frames: {s}')
            return B if self.t % 30 < 3 else 0
        if kind == 'autoheal':
            self.autoheal = g[1]
            self.next_goal()
            return None
        if self.autoheal and kind in ('boss', 'itbuu', 'beam', 'evade', 'fightp'):
            s = status(e)
            self.maybe_heal(e, s)
        return super().step(e)


def save_at(x, y):
    """walk to a save point's spot (x, y), face it, Save Game (GAME 1)"""
    return [('unstick',), ('walk', x, y), ('face', 'U'), SAVE]


def checkpoint(name, x, y, w, a):
    """Save at the save point (spot x, y in area w-a); a game over after it (until the next checkpoint / retry_off)
    takes the death -> title (Options: music volume) -> Continue program and plays on from here (ShipDriver2's
    retry), punching the air once more per try first ('luck')"""
    return [('unstick',), ('walk', x, y), ('face', 'U'), SAVE,
            ('jumpto', 4), DEATH_CONTINUE, ('until', w, a), ('wait', 30),
            ('mark', name), ('retry_here',), ('luck',)]


# the pause menu tour of this part: Status -> the Skills page (DOWN) and back, Inventory -> Story Items, Journal ->
# Completed Goals; out
TOUR_SB = keys((S, 2, 60), (D, 3, 50), (U, 3, 40),
               (RS, 3, 40), (R, 3, 30),
               (LS, 3, 40), (LS, 3, 40), (R, 3, 30),
               (B, 3, 60))


def goals_run2(e):
    import sbroute
    r1 = list(sbroute.ROUTE)

    def seg(a, b):
        """run 1's ROUTE between its marks a and b (exclusive of mark a, inclusive of b)"""
        i = r1.index(('mark', a)) + 1
        j = r1.index(('mark', b)) + 1
        return r1[i:j]

    pre = seg('start', 'gotenks')
    # 6-7 -> 6-6: the save point there before going down to the fusion (6-6: rect 50,155 40x38)
    k = pre.index(('area', 6, 6)) + 2
    pre[k:k] = checkpoint('a02_6-6', 66, 185, 6, 6)
    return [
        ('mark', 'a00_start'),
        # --- HFIL 18-13 (Goku): the menu tour, bumping about, the save point ----------------------------------------
        TOUR_SB, ('closemenu',),
        # bumping into the path's walls (LEFT / RIGHT against them), then the save point (rect 105,145 47x55)
        ('walk', 80, 250), keys(*[(L, 40, 0), (R, 40, 0)] * 4), ('unstick',),
        *save_at(124, 186), ('mark', 'a01_saved'),
        *pre,
        ('luck',),
        ('boss', 20000), ('events',), ('wait', 30), ('mark', 'a03_after_gotenks'),
        # the crater (6-20 -> 6-16): Gotenks vs Super Buu again (a loss: back to 6-6's save, both fights again)
        *seg('after_gotenks', 'ult_gohan'),
        ('retry_off',), ('mark', 'b00_ult_gohan'),
        # --- Southwest Forest 8-1, Ultimate Gohan: his points POW-heavy (run 1: STR / END, a melee build) -----------
        ('allocnow', lambda n: [n // 6, n // 2, n // 6, n - n // 6 - n // 2 - n // 6]),
        # the capsule -> West City 4-1 -> its Z-Mart 0-92 (run 1 never shopped here): Buy -> Items: Rice Balls?
        # rows 2 and 3 (supplies 3 / 4) twice each; back, Sell -> the first row once; out
        ('walk', 280, 256), ('talk', 'U'), ('fly', 1250, 1200), ('until', 4, 1), ('wait', 30),
        ('walk', 262, 302), ('hold', U, 40), ('area', 0, 92), ('wait', 30), ('walk', 100, 110), ('hold', L, 20),
        keys((A, 3, 120), (A, 3, 90), (A, 3, 90), (D, 3, 50), (D, 3, 50), (A, 3, 60), (A, 3, 60), (D, 3, 50),
             (A, 3, 60), (A, 3, 60), (B, 3, 60), (B, 3, 60), (D, 3, 30), (A, 3, 90), (A, 3, 60)),
        ('bout',), ('wait', 30), ('walk', 116, 200), ('hold', D, 30), ('area', 4, 1), ('wait', 30),
        # 4-1's save point: Save, soft reset, Continue (the title's slot load) - then the checkpoint for the crater
        *save_at(148, 594), RESET_CONTINUE, ('until', 4, 1), ('wait', 30), ('mark', 'b01_continued'),
        *checkpoint('b02_4-1', 148, 594, 4, 1),
        ('autoheal', True),
        # the pad (410, 356) -> the Giant Crater: Ultimate Gohan vs Super Buu with the Electric Kamehameha (skill
        # slot 2, held 120 frames, from 70 px: the route's original plan - run 1's melee Gohan punched)
        ('unstick',), ('walk', 410, 356), ('talk', 'U'), ('fly', 648, 1076), ('events',), ('wait', 30),
        ('beam', 40000, 2, 120, 70), ('events',), ('wait', 30),
        ('retry_off',), ('autoheal', False), ('mark', 'b03_wasteland'),
    ]
