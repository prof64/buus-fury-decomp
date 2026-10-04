"""Run 2's ship chapter (7-2 after the World Tournament -> Southwest Forest -> Babidi's ship -> Majin Vegeta -> Majin
Buu): the same story as ship_route.real_prologue + goals_real (run 1), played differently to reach code paths run 1
never did. `goals_run2(e)` builds the goal list from the start state; `ShipDriver2` adds the goals it needs:
  ('die',)              walk up to the nearest enemy and stand there (no attack, no healing) until the game over
                        screen (area 0-2)
  ('walkbad', x, y, n)  bump around (L / U / R / D, 40 frames each) for n frames, then walk to (x, y)
  ('talkto', x, y[, side])  EarthDriver's: walk beside the live NPC nearest (x, y), face it, A, play the talk through
  ('mode', name, value) driver switches: 'blasty' (fire the selected skill at any enemy in line while EP lasts),
                        'split' (the grind's stat split per point count: a function n -> [STR, POW, END, SPD])
  ('grind', ...)        ShipDriver's, but with the run-2 split
Everything else is ShipDriver's (re/buu/ship/ship_driver.py). Input only - no RAM writes.

NOTE for the chain: the in-game Save writes the cartridge's flash; emulator save states (.ss) do NOT carry it. The
Save -> soft reset / death -> Continue stretches must be replayed in one emulator session from before the save (the
chain's replay from power-on does that); a .ss taken between a save and its Continue resumes with an empty slot."""
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[3] / 'tools' / 'story_playthrough'))
from ship_driver import ShipDriver, alloc_keys, enemies, level_of, KEY, status  # noqa: E402
from ship_route import nav, charsel, switch, button, item, panel, rest_at  # noqa: E402,F401
from earth_driver import EarthDriver  # noqa: E402

U, D, L, R, A, B = KEY.UP, KEY.DOWN, KEY.LEFT, KEY.RIGHT, KEY.A, KEY.B
S, E, LS, RS = KEY.START, KEY.SELECT, KEY.L, KEY.R


def split2(n):
    """run 2's grind split (run 1: half STR, half END): a third STR, a sixth POW, a third END, the rest SPD (END
    stays: the ship's stage 3 killed a Vegeta grown without it)"""
    return [n // 3, n // 6, n // 3, n - n // 3 - n // 6 - n // 3]


class ShipDriver2(ShipDriver):
    talkto = EarthDriver.talkto
    blasty = False
    split = staticmethod(split2)

    def in_flame(self, e, px, py, margin=44):
        """ShipDriver's, but never at the room's left edge: walk_keys waited there for a Destroyer's flames to stop
        (a step left into the exit stays in their line) while it walked up and burned Gohan (8-59's left exit)"""
        if px < 40:
            return False
        return super().in_flame(e, px, py, margin)

    def flame_guard(self, e, s):
        """ShipDriver's, but not at the room's left edge (as in_flame): with the wall behind, the guard turned Gohan
        to block the flames instead of stepping out through the exit"""
        if s['p'] is not None and s['p'][0] < 40:
            return None
        return super().flame_guard(e, s)

    def heal_step(self, e):
        """ShipDriver's healing menu with the DOWN presses 50 frames apart (20 apart some are dropped: with more than
        one kind of food in the Inventory the wrong row - Miso Soup, 20 HP - was eaten)"""
        h = self.heal
        if h is None or h['phase'] != 'open':
            return super().heal_step(e)
        h['t'] += 1
        if h['t'] <= 2:
            return KEY.START
        if status(e)['p'] is None:
            prog = [(0, 0, 70), (RS, 3, 60)] + [(D, 3, 50)] * h['row'] + [(A, 3, 40), (A, 3, 80), (B, 3, 60)]
            self.prog = []
            for m, on, off in prog:
                self.prog += [m] * on + [0] * off
            self.heal = None
            return self.prog.pop(0)
        if h['t'] > 40:
            self.heal = None
            self.heal_cool = 20
        return 0

    def fightp(self, e, s, g):
        # 'blasty': the selected skill first, at any enemy in line, while the EP lasts (run 1 punched)
        if self.blasty and s['p'] is not None and s['scene'] == 0:
            x, y = s['p'][0], s['p'][1]
            ens = [o for o in enemies(e) if o[4] > 0 and o[5] not in self.AVOID]
            if ens:
                en = min(ens, key=lambda o: abs(o[1] - x) + abs(o[2] - y))
                if abs(en[1] - x) + abs(en[2] - y) > 36:
                    k = self.blast(e, s, [en])
                    if k is not None:
                        return k
        return super().fightp(e, s, g)

    def step(self, e):
        g = self.goals[self.k]
        kind = g[0]
        s = status(e)
        if kind == 'die':
            if s['area'][:2] == [0, 2]:
                self.next_goal()
                return None
            if s['scene'] > 0 or s['p'] is None:
                return 0
            ens = [o for o in enemies(e) if o[4] > 0]
            if not ens:
                return 0
            x, y = s['p'][0], s['p'][1]
            o = min(ens, key=lambda o: abs(o[1] - x) + abs(o[2] - y))
            if abs(o[1] - x) <= 20 and abs(o[2] - y) <= 12:
                return 0
            return self.walk_keys(e, s, o[1] - 18 if o[1] > x else o[1] + 18, o[2])
        if kind == 'walkbad':
            if self.t < g[3]:
                return (L, U, R, D)[(self.t // 40) % 4]
            if s['p'] is None or s['scene'] > 0:
                return self.mash_a() if s['scene'] > 0 else 0
            if (abs(s['p'][0] - g[1]) <= 1 and abs(s['p'][1] - g[2]) <= 1) or self.t > g[3] + 3000:
                self.next_goal()
                return None
            return self.walk_keys(e, s, g[1], g[2])
        if kind == 'talkto':
            return self.talkto(e, s, g)
        if kind == 'mode':
            setattr(self, g[1], g[2])
            self.next_goal()
            return None
        if kind == 'grind':
            if s['p'] is not None and s['scene'] == 0 and level_of(e) >= g[1]:
                self.next_goal()
                return None
            route = g[2]
            if self.grind_k is None:
                self.grind_k = 0
            w, a = route[self.grind_k % len(route)]
            if s['area'][:2] == [w, a] and s['p'] is not None and self.nav_area is None:
                c = e.r8(0x03001068)
                pts = e.r16(0x03001560 + 0x68 * c + 0x2C)
                if pts >= 6 and not enemies(e) and getattr(self, 'alloc_tried', None) != pts:
                    self.alloc_tried = pts
                    for m, on, off in alloc_keys([(0, self.split(pts))])[1]:
                        self.prog += [m] * on + [0] * off
                    return self.prog.pop(0)
                self.grind_k += 1
                w, a = route[self.grind_k % len(route)]
            return self.nav(e, s, ('nav', w, a, True), inner=True)
        return super().step(e)


def keys(*prog):
    return ('keys', list(prog))


# --- menu programs (pause menu: START; shoulder R / L page right / left: Status, Inventory, Equip, Options, Journal) --
# the save point menu (standing on it, facing it): Save (the first entry) -> the Save Game screen -> GAME 1 ->
# "Saving..." -> "Game Saved" closed
SAVE = keys((A, 3, 80), (A, 3, 80), (A, 3, 160), (0, 0, 60), (A, 3, 80))
# soft reset (A+B+START+SELECT) -> the logos -> title: START, Single Player (A) -> Select Game: GAME 1 (A) -> the field
RESET_CONTINUE = keys((A | B | S | E, 10, 800), (S, 4, 200), (A, 4, 120), (A, 4, 300))
# the death message -> the title (Start / Options): Options (the title's own screen: SFX / music / text speed,
# Credits), the music volume down two steps, back; Start -> Single Player -> GAME 1 (the last save)
DEATH_CONTINUE = keys((A, 3, 300), (D, 3, 30), (A, 3, 120), (D, 3, 20), (L, 3, 15), (L, 3, 15), (B, 3, 100),
                      (U, 3, 30), (A, 3, 200), (A, 4, 120), (A, 4, 300))
# the pause menu tour run 1 never made: Status (Gohan's tab), Journal -> Completed Goals, Inventory -> Story Items,
# Equip, Options -> Text Speed up three steps; B leaves the option list, B again closes the menu. (The Status page's
# "Skills" icon did not open with SELECT: not in the tour.)
TOUR = keys((S, 2, 60), (R, 3, 20), (L, 3, 20),                   # Status: Gohan's tab, back
            (LS, 3, 40), (R, 3, 30), (RS, 3, 40),                    # Journal: Completed Goals; Status
            (RS, 3, 40), (R, 3, 30),                                 # Inventory: Story Items
            (RS, 3, 40),                                             # Equip
            (RS, 3, 40), (A, 3, 30), (D, 3, 15), (D, 3, 15),         # Options: Text Speed up three steps
            (R, 3, 15), (R, 3, 15), (R, 3, 15), (B, 3, 40),
            (B, 3, 60))                                              # closed


def stall(x, y, picks):
    """a 7-2 stall (s257 drinks / s258 food / s259 equipment: "Want something?" -Sure): picks = [(rows down from the
    previous pick, buys)], B out, the goodbye text"""
    k = [(A, 3, 60), (A, 3, 100)]
    for down, n in picks:
        k += [(D, 3, 50)] * down + [(A, 3, 40)] * n      # (a D soon after the last is not always taken)
    k += [(B, 3, 100), (B, 3, 100)]      # B leaves the list, B closes the goodbye (an A here talks again)
    return [('unstick',), ('walk', x, y), ('face', 'U'), ('keys', k), ('events',), ('wait', 30)]


def equip2(slot):
    """pause menu -> Equip (shoulder R twice) -> slot row (body / hands / feet / accessory) -> the first item ->
    Equip to the active character"""
    # (A on the item: "Equip to <char>" / "Drop"; A equips and returns to the slot list; B closes the menu)
    k = [(S, 2, 60), (RS, 3, 40), (RS, 3, 40)] + [(D, 3, 50)] * slot + [(A, 3, 40), (A, 3, 40), (A, 3, 60),
                                                                       (B, 3, 80)]
    return ('keys', k)


def use_item(row):
    """pause menu -> Inventory (shoulder R) -> supply row `row` -> A, Use -> back"""
    return ('keys', [(S, 2, 60), (RS, 3, 60)] + [(D, 3, 20)] * row + [(A, 3, 40), (A, 3, 80), (B, 3, 60), (S, 2, 60)])


def goals_run2(e):
    pts = [e.r16(0x03001560 + 0x68 * c + 0x2C) for c in range(5)]
    return [
        ('mark', 'r00_7-2'),
        # --- Papaya Island 7-2: the pause menu tour, everyone's points POW / SPD heavy, two tournament talkers, the
        # drinks stall (EP supplies: run 1 bought food only) ------------------------------------------------------
        TOUR,
        alloc_keys([(0, [0, pts[0] // 2, 0, pts[0] - pts[0] // 2]), (2, [0, pts[3], 0, 0])]),
        ('talkto', 264, 88, 'D'), ('wait', 20),
        ('talkto', 232, 80, 'D'), ('wait', 20),
        *stall(188, 712, [(0, 2), (2, 1)]),          # drinks: item 10 x2, item 12
        ('walkbad', 232, 168, 320),                   # bumping about (wasted walking)
        nav(7, 1, False), ('walk', 224, 340), ('face', 'U'), ('press', A), ('wait', 60),   # the capsule
        ('fly', 633, 1065, 31, 23), ('wait', 30),                                            # Kami's Lookout 6-1
        ('talkto', 312, 328, 'D'), ('wait', 20),                                             # s208
        ('walk', 48, 320), ('face', 'L'), ('press', A), ('until', 6, 13), ('wait', 30),
        ('walk', 184, 116), ('talk', 'U'), ('wait', 30),                                     # Yajirobe: s230
        *[('talk', 'U'), ('wait', 30)] * 3,
        ('talkto', 136, 152, 'U'), ('wait', 20),                                             # Korin: s229
        ('walk', 72, 150), ('face', 'U'), ('press', A), ('until', 6, 1), ('wait', 30),
        ('walk', 304, 570), ('face', 'U'), ('press', A), ('wait', 60),
        ('fly', 1953, 2041, 143, 159), ('wait', 30),                                         # Papaya Island 7-1
        nav(7, 2, False), ('walk', 224, 88), ('hold', U, 20),                                # s261
        ('story', 8, 1, True), ('wait', 30),                                                 # .. s314 -> 8-1
        ('mark', 'r01_8-1'),
        # --- 8-1: Save (GAME 1), Goku's points, Goku, East District: die to the fighters -> the title's Options ->
        # Continue from the save (Gohan again, at the save point) -----------------------------------------------
        ('walk', 148, 186), ('face', 'U'), SAVE,
        ('mark', 'r01b_saved'),
        ('walk', 148, 186), *charsel(-1),                                                    # Goku
        ('walk', 280, 266), ('face', 'U'), ('press', A), ('wait', 60),
        ('fly', 2632, 1465, 80, 71), ('wait', 30),                                           # East District 2-8
        nav(2, 9, False), ('die',), DEATH_CONTINUE, ('until', 8, 1), ('wait', 30),
        ('mark', 'r01c_continued'),
        # Goku again (his points went at 7-2), East District, the grind with skills first
        ('unstick',), ('walk', 148, 186), *charsel(-1),
        ('walk', 280, 266), ('face', 'U'), ('press', A), ('wait', 60),
        ('fly', 2632, 1465, 80, 71), ('wait', 30),
        ('mark', 'r02_grind_goku'),
        ('skill', 0), ('mode', 'blasty', True),                       # Energy Blasts first (run 1 punched)
        ('grind', 70, [(2, 9), (2, 10)]),
        ('mode', 'blasty', False), ('skill', 0),
        use_item(0),                                                                         # a drink (EP)
        nav(2, 8, False), ('walk', 176, 380), ('face', 'U'), ('press', A), ('wait', 60),
        ('fly', 849, 1785, 31, 23), ('wait', 30),                                            # Southwest Forest 8-1
        nav(8, 2, False), ('walk', 104, 56), ('face', 'U'), ('press', A), ('events',),      # s517: the gate
        nav(8, 1, False), ('walk', 148, 186), *charsel(1),                                   # Gohan
        ('mark', 'r03_grind_gohan'),
        # Gohan carries the ship: his points two thirds END (run 1: half STR, half END; a POW / SPD Gohan ate all
        # his food by stage 3 and died there)
        ('mode', 'split', lambda n: [n - 2 * n // 3, 0, 2 * n // 3, 0]),
        ('grind', 103, [(8, 3), (8, 6)]),
        ('mode', 'split', split2),
        nav(8, 1, False), ('walk', 148, 186), *charsel(1),                                   # Vegeta
        ('mark', 'r04_grind_vegeta'),
        ('mode', 'blasty', True),
        ('grind', 92, [(8, 3), (8, 6)]),
        ('mode', 'blasty', False),
        nav(8, 1, False), ('walk', 148, 186), *charsel(-1),                                  # Gohan
        # --- Papaya Island: the equipment stall (run 1 never bought or wore any here): Leather Gloves, Leather
        # Moccasins, worn by Gohan; then food: four Onigiri, a Miso Soup ---------------------------------
        nav(8, 1, False), ('walk', 280, 266), ('face', 'U'), ('press', A), ('wait', 60),
        ('fly', 1953, 2041, 143, 159), ('wait', 30),                                         # Papaya Island 7-1
        nav(7, 2, False), ('mark', 'r05_shop'),
        *stall(188, 472, [(0, 1), (2, 1)]),
        equip2(1), equip2(2),
        *stall(276, 592, [(4, 4)]), *stall(276, 592, [(0, 1)]),     # (a DOWN right after a purchase is dropped)
        nav(7, 1, False), ('walk', 224, 340), ('face', 'U'), ('press', A), ('wait', 60),
        ('fly', 849, 1785, 31, 23), ('wait', 30),                                            # Southwest Forest
        nav(8, 16, False), ('wait', 30),
        # --- the ship (run 1's goals from 8-16 on), with a Save + soft reset + Continue at 8-28's save point -------
        *ship_main(e),
    ]


def ship_main(e):
    import ship_route
    g = ship_route.goals(e)
    g = g[g.index(('mark', 'c04_8-16')):]
    k = g.index(('mark', 'c07_8-28'))
    # 8-28's save point: Save, soft reset, Continue (the field from the save), then Gohan again (run 1's switch)
    g[k + 1:k + 1] = [('walk', 60, 306), ('face', 'U'), SAVE, ('mark', 'c07b_saved'), RESET_CONTINUE,
                      ('until', 8, 28), ('wait', 30), ('mark', 'c07c_continued'),
                      ('unstick',), ('walk', 60, 306), *charsel(-2)]
    k = g.index(('mark', 'c09_8-41'))
    # Gohan, rested full at the save point first (Goten stands in)
    g[k + 1:k + 1] = [('walk', 60, 314), *charsel(1), ('mark', 'c09b_rest'), *rest_at(60, 314, 1, 1)]
    # stage 3 walked past the room fights (run 1 fought them): the fight guards kept Gohan blocking a Destroyer's
    # flames beside 8-59's exit until he died
    k1 = g.index(('mark', 'c11_8-66'))
    g[k:k1] = [('nav', x[1], x[2], False) if x[0] == 'nav' else x for x in g[k:k1]]
    return g
