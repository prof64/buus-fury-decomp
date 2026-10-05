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

    _menu_t = 0

    def keys(self, e):
        self._calls = 0
        # a pause menu left open in a fight / grind (a heal program that went wrong: the frozen ShipDriver only backs
        # out of it in nav / fightp goals): B out after 300 frames
        if not self.prog and self.heal is None and not self.done() and \
                self.goals[self.k][0] in ('grind', 'boss', 'beam', 'itbuu', 'evade', 'nav', 'fightp'):
            st = status(e)
            if st['p'] is None and st['scene'] == 0 and st['area'][0] != 0 and e.r32(0x03001C34) == 0:
                self._menu_t += 1
                if self._menu_t > 300:
                    return B if self._menu_t % 20 < 3 else 0
            else:
                self._menu_t = 0
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
            n = self.tries + 10 * getattr(self, 'cycles', 0)
            self.next_goal()
            if n:
                # (off the save point first: an A on it opens its menu, whatever the facing)
                self.prog += [D] * 40 + [0] * 12
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
        if kind == 'deathcont':
            # the game over -> title -> Single Player -> GAME 1 (Continue), closed-loop: A through the death text
            # until the title has stood still 120 frames, then START / A alternately every 40 frames until a leader
            # exists, then A until it is controllable
            s = status(e)
            ph = getattr(self, 'dc_phase', 0)
            if ph == 0:
                if s['p'] is None and s['scene'] == 0 and s['area'][0] == 0:
                    self.idle += 1
                    if self.idle > 120:
                        self.dc_phase, self.idle, self.dc_t = 1, 0, 0
                    return 0
                self.idle = 0
                return A if self.t % 40 < 3 else 0
            if ph == 1:
                if s['p'] is not None:
                    self.dc_phase = 2
                    return 0
                self.dc_t += 1
                if self.dc_t % 40 < 4:
                    return S if (self.dc_t // 40) % 2 == 0 else A
                return 0
            if s['scene'] == 0 and s['p'] is not None:
                self.idle += 1
                if self.idle >= 30:
                    self.dc_phase = 0
                    self.next_goal()
                    return None
                return 0
            self.idle = 0
            return A if self.t % 20 < 3 else 0
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
        if kind == 'retry_here' and self.retry_k != self.k - 4:
            self.cycles = 0
        if kind == 'hpok':
            # ('hpok', frac): the active character below frac of max HP -> soft reset (A+B+START+SELECT) and the
            # retry stretch again from its save (as after a game over: the title, Continue, one more luck punch)
            s = status(e)
            if s['p'] is None or s['scene'] > 0:
                return self.mash_a() if s['scene'] else 0
            rec = 0x03001560 + 0x68 * e.r8(0x03001068)
            hp, mx = e.r16(rec + 0x16), e.r16(rec + 0x18)
            if hp * 100 >= mx * int(g[1] * 100) or self.retry_k is None:
                self.next_goal()
                return None
            self.tries += 1
            if self.tries >= 10:
                self.cycles = getattr(self, 'cycles', 0) + 1
                self.tries = 0
                if self.cycles > 3:
                    raise RuntimeError('hpok: forty resets')
            self.prog += [A | B | S | E] * 10 + [0] * 5
            self.k = self.retry_k - 1
            self.next_goal()
            return self.prog.pop(0)
        if kind == 'allocfor':
            # ('allocfor', c, tab_moves, fn): character c's unspent points n -> fn(n), spent on the Status page
            # opened on the active character's tab and moved `tab_moves` tabs right
            n = e.r16(0x03001560 + 0x68 * g[1] + 0x2C)
            self.next_goal()
            if n:
                for m, on, off in alloc_keys([(g[2], g[3](n))])[1]:
                    self.prog += [m] * on + [0] * off
                return self.prog.pop(0)
            return None
        if kind == 'autoheal':
            self.autoheal = g[1]
            self.next_goal()
            return None
        s = status(e)
        if self.retry_k is not None and s['area'][:2] == [0, 2] and s['hp'] == 0 and self.tries >= 9 and \
                self.k not in (self.retry_k, self.retry_k + 1):
            # (ShipDriver2 gives up after ten game overs in a stretch: up to 40 here, the luck punches go on growing)
            self.cycles = getattr(self, 'cycles', 0) + 1
            if self.cycles > 3:
                raise RuntimeError('retry stretch: forty game overs')
            self.tries = -1
        if self.autoheal and kind in ('boss', 'itbuu', 'beam', 'evade', 'fightp'):
            self.maybe_heal(e, s)
        if self.retry_k is not None and s['area'][:2] == [0, 2] and s['hp'] > 0:
            # area 0-2 with HP left is not a game over (the Spirit Bomb's scene after Kid Buu): no retry
            rk = self.retry_k
            self.retry_k = None
            try:
                return super().step(e)
            finally:
                if self.retry_k is None:
                    self.retry_k = rk
        return super().step(e)


def swap_rest(x, y, c):
    """Goku <-> Vegeta at a save point (Switch Characters, LEFT once either way: the other three are out in this
    part): the other stands in until character c (in reserve) is full, then back"""
    return [('unstick',), ('walk', x, y), *charsel(-1), ('rest', c), ('unstick',), ('walk', x, y), *charsel(-1)]


def save_at(x, y):
    """walk to a save point's spot (x, y), face it, Save Game (GAME 1)"""
    return [('unstick',), ('walk', x, y), ('face', 'U'), SAVE, ('bout',)]


def checkpoint(name, x, y, w, a):
    """Save at the save point (spot x, y in area w-a); a game over after it (until the next checkpoint / retry_off)
    takes the death -> title (Options: music volume) -> Continue program and plays on from here (ShipDriver2's
    retry), punching the air once more per try first ('luck')"""
    return [('unstick',), ('walk', x, y), ('face', 'U'), SAVE, ('bout',),
            ('jumpto', 4), ('deathcont',), ('until', w, a), ('wait', 30),
            ('mark', name), ('retry_here',), ('luck',)]


# the pause menu tour of this part: Status -> the Skills page (DOWN) and back, Inventory -> Story Items, Journal ->
# Completed Goals; out
TOUR_SB = keys((S, 2, 60), (D, 3, 50), (U, 3, 40),
               (RS, 3, 40), (R, 3, 30),
               (LS, 3, 40), (LS, 3, 40), (R, 3, 30),
               (B, 3, 60))


#   pause menu -> Journal (L shoulder) -> Options (L) -> Sleep (DOWN x3, RIGHT, A) -> s007's box -> the sleep mode ->
#   L + R + SELECT held 20 frames wakes it -> "Sleep Mode ended."
SLEEP = keys((S, 2, 60), (LS, 3, 40), (LS, 3, 40), (D, 3, 20), (D, 3, 20), (D, 3, 20), (R, 3, 20), (A, 3, 120),
             (A, 3, 200), (0, 0, 300), (LS | RS | E, 20, 200), (A, 3, 120))
#   West City 4-1 -> its Z-Mart 0-92 -> Buy -> Items: two Curry Plates (the list's first row: 400 HP, 2000 zenie) ->
#   out -> 4-1
ZMART_CURRY = [('nav', 0, 92, False), ('wait', 30), ('walk', 100, 110), ('hold', L, 20),
               keys((A, 3, 120), (A, 3, 90), (A, 3, 90), (A, 3, 60), (A, 3, 60)),
               ('bout',), ('wait', 30), ('walk', 116, 200), ('hold', D, 30), ('area', 4, 1), ('wait', 30)]


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

    def navs(goals):
        return [('nav', int(x[1].split('-')[0]), int(x[1].split('-')[1]), True) if x[0] == 'exitto' else x for x in goals]

    inside = navs(seg('inside_buu', 'brain'))
    # 21-14: Vegeta stays (run 1's Switch Characters program to Goku out), a checkpoint there instead
    k = next(i for i, x in enumerate(inside) if x[0] == 'prog' and 'L:2' in x[1])
    inside[k - 1:k + 1] = checkpoint('d01_21-14', 166, 236, 21, 14)
    # 21-25's save point before the brain (21-26)
    # (Goku, rested since 21-1, takes over there: Vegeta lost the brain fight ten times in a row)
    k = inside.index(('nav', 21, 26, True))
    inside[k:k] = [('unstick',), ('walk', 56, 150), *charsel(-1), *checkpoint('d02_21-25', 56, 150, 21, 25)]
    inside += [('boss', 30000), ('eventsw', 240), ('wait', 20), ('walke', 160, 40), ('eventsw', 240), ('wait', 30),
               ('retry_off',), ('autoheal', False), ('mark', 'd03_kid_buu_saga')]
    # a start in the arena 18-14 (run 2's chain: right after Janemba) walks down to 18-13 first (run 1's sb_start
    # stretch did the same)
    first = []
    if [e.r8(0x0300197C), e.r8(0x0300197D)] == [18, 14]:
        first = [('exitto', '18-13'), ('areaw', 18, 13), ('wait', 30)]
    return [
        ('mark', 'a00_start'), *first,
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
        # (STR / END 2/5 each, POW 1/5: run 1 put half into STR, half into END - a POW-heavy Gohan died in the Pyramid
        # every few thousand frames)
        ('allocnow', lambda n: [n * 2 // 5, n // 5, n * 2 // 5, n - 2 * (n * 2 // 5) - n // 5]),
        # the Equip page: Gohan's body slot -> the first item in the list (run 1 wore nothing in this part)
        ('wait', 30), *equip2(0), ('wait', 30),
        # the pad -> West City 4-1 -> its Z-Mart 0-92 (run 1 never shopped here): Buy -> Items: two Curry Plates
        # (the list's first row now: 400 HP each, 2000 zenie); back, Sell -> the fourth row (the Cotton Gi); out
        ('walk', 280, 256), ('talk', 'U'), ('fly', 1250, 1200), ('until', 4, 1), ('areaw', 4, 1), ('wait', 30),
        ('nav', 0, 92, False), ('wait', 30), ('walk', 100, 110), ('hold', L, 20),
        keys((A, 3, 120), (A, 3, 90), (A, 3, 90), (A, 3, 60), (A, 3, 60), (B, 3, 60), (B, 3, 60), (D, 3, 30),
             (A, 3, 90), (D, 3, 50), (D, 3, 50), (D, 3, 50), (A, 3, 60)),
        ('bout',), ('wait', 30), ('walk', 116, 200), ('hold', D, 30), ('area', 4, 1), ('wait', 30),
        # 4-1's save point: Save, soft reset, Continue (the title's slot load) - then the checkpoint for the crater
        *save_at(148, 594), RESET_CONTINUE, ('until', 4, 1), ('wait', 30), ('mark', 'b01_continued'),
        # the pad (410, 356) -> Kyodai Pyramid 14-1: the second gate (Gohan, level 120: s517, flag 176); a save there;
        # Gohan to level 141 in 14-1 <-> 14-2 with Energy Blasts first (run 1: punches, a STR build) and food from
        # the Z-Mart when low (run 1 never healed); a game over goes back to this save
        ('unstick',), ('walk', 410, 356), ('talk', 'U'), ('fly', 2177, 1201, 31, 23), ('until', 14, 1), ('wait', 30),
        ('walk', 224, 92), ('hold', U, 20), ('press', A), ('events',), ('wait', 30), ('mark', 'b02_gate'),
        *checkpoint('b03_14-1', 90, 240, 14, 1),
        ('autoheal', True), ('skill', 0), ('mode', 'blasty', True),
        ('grind', 131, [(14, 1), (14, 2)]),
        ('mode', 'blasty', False), ('retry_off',),
        # (in steps, each saved at 14-1: a game over costs only the step)
        # (nobody can stand in for Gohan here: Goku, Vegeta dead, Goten / Trunks greyed out)
        *[x for lv in (134, 137, 139, 141) for x in [('nav', 14, 1, False),
                                                       *checkpoint(f'b04_gohan{lv - 3}', 90, 240, 14, 1),
                                                       ('grind', lv, [(14, 1), (14, 2)]), ('retry_off',)]],
        # the grind's zenie -> two more Curry Plates at West City's Z-Mart; the checkpoint at 4-1
        ('nav', 14, 1, False), ('unstick',), ('walk', 146, 214), ('talk', 'U'), ('fly', 1250, 1200), ('until', 4, 1), ('areaw', 4, 1),
        ('wait', 30), *ZMART_CURRY,
        *checkpoint('b04_gohan141', 148, 594, 4, 1), ('autoheal', False),   # (the curries are for 20-1)
        # the pad -> the Giant Crater: Ultimate Gohan vs Super Buu by punches as run 1 (the route's Electric
        # Kamehameha plan lost ten times in a row here too), but eating Curry Plates when low (run 1 never)
        ('unstick',), ('walk', 410, 356), ('talk', 'U'), ('fly', 648, 1076), ('events',), ('wait', 30),
        ('boss', 40000), ('events',), ('wait', 30), ('mark', 'b05_wasteland'),
        # --- Barren Wasteland (Gohan) 20-1: the curries; a Gohan left with too little HP for 20-2 .. 20-4 (the Death
        # Machines killed him forty times in a row from 871) soft-resets and Continues at 4-1 for another crater
        # fight; then a save at 20-1 (run 1 passed it), the story to 20-5 and Gohan's second fight with Super Buu ---
        *[x for _ in range(2) for x in (use_item(0), ('closemenu',), ('wait', 20))],   # up to two Curry Plates
        ('hpok', 0.8),
        ('retry_off',), ('autoheal', False),
        *checkpoint('c00_20-1', 231, 234, 20, 1), ('autoheal', True),
        *seg('wasteland', 'gohan_vs_buu2'),
        *seg('gohan_vs_buu2', 'fusion_saga'),
        ('retry_off',), ('autoheal', False), ('mark', 'c01_fusion_saga'),
        # --- 20-5 as Goku: his points POW / SPD (run 1: STR / END); the 20-6 save point; Goku to level 140 for the
        # gate with Energy Blasts first, in steps saved at 20-6 (run 1: 141 by punches, no save) ---------------------
        ('allocnow', lambda n: [0, n // 2, 0, n - n // 2]),
        # (level-up points: STR / END a third each, POW / SPD a sixth - run 1: half STR, half END; punches: the
        # Energy Blast Goku died to the Power Mechandoid every time)
        ('mode', 'split', lambda n: [n // 3, n // 6, n // 3, n - 2 * (n // 3) - n // 6]),
        # (the first levels as run 1, no save to fall back on: the 20-5 / 20-6 robots by punches)
        # (each step saved at 20-6, from the first one on; the retry stretch runs on until the next save)
        *[x for lv in (127, 130, 133, 136, 140) for x in [('nav', 20, 6, True), *checkpoint(f'c02_goku{lv - 3}', 56, 124, 20, 6),
                                                  ('autoheal', True), ('grind', lv, [(20, 6), (20, 5)])]],
        ('nav', 20, 6, True), *checkpoint('c03_goku140', 56, 124, 20, 6), ('autoheal', True),
        # the gate (Goku 140: s473, "confront Majin Buu? Yes") -> 20-7: Goku and Vegeta vs Super Buu (the three
        # Instant Transmission rounds), Vegito, the absorption -> inside Buu (Vegeta)
        ('walk', 96, 44), ('talk', 'U'), ('eventsw', 240), ('wait', 20),
        *seg('goku_vs_buu', 'inside_buu'),
        ('retry_off',), ('autoheal', False), ('mark', 'c04_inside_buu'),
        # --- inside Buu (Vegeta): a save at 21-1 (run 1 passed it); the pods; at 21-14 Vegeta saves and goes on
        # himself (run 1 switched to Goku there) - the three fakes and Super Buu in the brain are his; 21-25's save
        # before the brain. (ShipDriver's nav for the exits: the route's exitto stood still in 21-2 in run 1) ------
        *checkpoint('d00_21-1', 424, 290, 21, 1), ('autoheal', True),
        *inside,
        # --- Kid Buu saga (Goku, world map): West City 4-1, Goku's points (POW / SPD: run 1 STR / END), two Curry
        # Plates, Vegeta stands in at the save point until Goku is full, then Goku back and rested Vegeta too;
        # no grind (run 1: Goku to 158 and Vegeta to 150 in the Pyramid, 188k frames) - a lower party XP, another
        # Dragon Ranking; the fight with food and retries from 4-1's save ---------------------------------------
        ('fly', 1250, 1200), ('until', 4, 1), ('areaw', 4, 1), ('wait', 30),
        *ZMART_CURRY,
        ('events',), ('allocnow', lambda n: [n // 4, n // 4, n // 4, n - 3 * (n // 4)]), ('wait', 30),
        # Vegeta's 87 points from Goku's Status page (his tab: RIGHT four times; he cannot be switched to here),
        # POW / SPD heavier than run 1's half STR / half END; then both rest full (in reserve: Vegeta)
        ('allocfor', 4, 4, lambda n: [n // 4, n // 3, n // 4, n - n // 4 - n // 3 - n // 4]), ('closemenu',),
        ('wait', 30), ('rest', 4),
        # Options -> Sleep (s007: the console's sleep mode, woken by L + R + SELECT; run 1 never slept)
        SLEEP, ('events',), ('wait', 30),
        *checkpoint('e00_4-1', 148, 594, 4, 1), ('autoheal', True),
        ('unstick',), ('walk', 410, 356), ('talk', 'U'), ('fly', 648, 1076), ('eventsw', 240), ('wait', 30),
        ('mark', 'e01_kid_buu'), ('luck',),
        ('boss', 60000, 'timer'), ('eventsw', 240), ('evade', 60000), ('evade', 60000), ('eventsw', 240), ('wait', 30),
        ('retry_off',), ('autoheal', False), ('mark', 'e02_after_kid_buu'),
        # --- the ending: West City -> Capsule Corp 4-20 -> 4-21 (s190) -> the party, the Tournament, Uub, the
        # credits, the ranking, the title --------------------------------------------------------------------------
        ('fly', 1137, 1137, 71, 119), ('until', 4, 1), ('areaw', 4, 1), ('wait', 30), ('exitto', '4-20'), ('wait', 20),
        ('exitto', '4-21'), ('wait', 20), ('mark', 'e03_final'),
        ('walke', 100, 76), ('eventsw', 240), ('wait', 30), ('mark', 'e04_the_end'),
    ]
