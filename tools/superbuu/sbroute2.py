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

    def step(self, e):
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
        ('walkbad', 94, 60, 240),
        *save_at(124, 186), ('mark', 'a01_saved'),
        *pre,
        ('luck',),
        ('boss', 20000), ('events',), ('wait', 30), ('mark', 'a03_after_gotenks'),
        ('retry_off',),
    ]
