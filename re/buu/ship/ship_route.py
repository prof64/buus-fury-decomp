"""The chapter route (Southwest Forest -> Babidi's ship -> Majin Vegeta -> Majin Buu): one list of ShipDriver goals.
('mark', name) goals are checkpoints (run_route.py saves a state there and can resume from it).

`goals(e)` builds the list from the start state (the stat allocation reads the unspent points)."""
from ship_driver import alloc_keys, KEY

U, D, L, R, A, B = KEY.UP, KEY.DOWN, KEY.LEFT, KEY.RIGHT, KEY.A, KEY.B


def nav(w, a, fight=True):
    return ('nav', w, a, fight)


def switch(x=104, y=161):
    """a ship switch pedestal (rect (97,137) 31x23, A facing up); its script shows the door (8-26 / 8-38)"""
    return [('walk', x, y), ('face', 'U'), ('press', A), ('wait', 500)]


def charsel(moves):
    """save point menu -> Switch Characters -> LEFT/RIGHT `moves` -> A (standing at a save point, facing it)"""
    d = R if moves > 0 else L
    k = [(A, 3, 40), (D, 3, 12), (A, 3, 80)] + [(d, 3, 20)] * abs(moves) + [(A, 3, 60)]
    return [('face', 'U'), ('keys', k), ('events',)]


def shop(x, y, row, n):
    """a food stall (\"Want something to eat?\" -Sure): row `row` of the buy list, A `n` times (one item each while
    the zenie lasts), B out, the goodbye text"""
    k = [(A, 3, 60), (A, 3, 100)] + [(D, 3, 15)] * row + [(A, 3, 40)] * n + [(B, 3, 100), (A, 3, 60)]
    return [('walk', x, y), ('face', 'U'), ('keys', k), ('events',), ('wait', 30)]


def split(n):
    """unspent stat points -> [STR, POW, END, SPD]: half STR, half END"""
    return [n - n // 2, 0, n // 2, 0]


def button(x):
    """an 8-63 door button (A facing up); its script shows the door it opens"""
    return [('walk', x, 88), ('face', 'U'), ('press', A), ('wait', 700)]


def item():
    """the Switch item on its pedestal (8-52 / 8-57 / 8-61)"""
    return [('walk', 104, 136), ('face', 'U'), ('press', A), ('events',), ('wait', 30)]


def panel():
    """a switch panel (s330 / s331 / s335: uses a Switch)"""
    return [('walk', 104, 161), ('face', 'U'), ('press', A), ('events',), ('wait', 60)]


def real_prologue(e):
    """from the Earth agent's real end state (work/earth/earth_end.ss: Goku in 7-2 after Spopovich / Yamu): the Senzu
    Beans from Yajirobe (s230, flag 51), the rest of the tournament (s261 .. s314) to Southwest Forest, Goku to level 70
    in East District 439 (the 8-2 gate), the gate, Gohan to level 85 in Southwest Forest (the ship's hatch)"""
    pts = [e.r16(0x03001560 + 0x68 * c + 0x2C) for c in range(5)]
    return [
        ('mark', 'r00_7-2'),
        nav(7, 1, False), ('walk', 224, 340), ('face', 'U'), ('press', A), ('wait', 60),   # the capsule
        ('fly', 633, 1065, 31, 23), ('wait', 30),                                            # Kami's Lookout 6-1
        ('walk', 48, 320), ('face', 'L'), ('press', A), ('until', 6, 13), ('wait', 30),
        ('walk', 184, 116), ('talk', 'U'), ('wait', 30),                                     # Yajirobe: s230
        # talking again: one more Senzu per story milestone flag (0, 13, 43, 54, ... : three here, s230 node 7)
        *[('talk', 'U'), ('wait', 30)] * 3,
        ('walk', 72, 150), ('face', 'U'), ('press', A), ('until', 6, 1), ('wait', 30),
        ('walk', 304, 570), ('face', 'U'), ('press', A), ('wait', 60),
        ('fly', 1953, 2041, 143, 159), ('wait', 30),                                         # Papaya Island 7-1
        nav(7, 2, False), ('walk', 224, 88), ('hold', U, 20),                                # s261
        ('story', 8, 1, True), ('wait', 30),                                                 # .. s314 -> 8-1
        ('mark', 'r01_8-1'),
        alloc_keys([(-1, split(pts[0]))]),                                                   # Goku's points
        ('walk', 148, 186), *charsel(-1),                                                    # Goku
        ('walk', 280, 266), ('face', 'U'), ('press', A), ('wait', 60),
        ('fly', 2632, 1465, 80, 71), ('wait', 30),                                           # East District 2-8
        ('mark', 'r02_grind_goku'),
        ('grind', 70, [(2, 9), (2, 10)]),
        nav(2, 8, False), ('walk', 176, 380), ('face', 'U'), ('press', A), ('wait', 60),
        ('fly', 849, 1785, 31, 23), ('wait', 30),                                            # Southwest Forest 8-1
        nav(8, 2, False), ('walk', 104, 56), ('face', 'U'), ('press', A), ('events',),      # s517: the gate
        nav(8, 1, False), ('walk', 148, 186), *charsel(1),                                   # Gohan
        ('mark', 'r03_grind_gohan'),
        # 85 opens the hatch; the ship's corridors (Destroyers, soldiers' waves, no Senzu Beans to spare) want 100
        ('grind', 100, [(8, 3), (8, 6)]),
        # Vegeta too: he fights Buu alone in the Giant Crater (8-71, Buu 14640 HP); Gohan rests in reserve meanwhile
        nav(8, 1, False), ('walk', 148, 186), *charsel(1),                                   # Vegeta
        ('mark', 'r04_grind_vegeta'),
        ('grind', 92, [(8, 3), (8, 6)]),
        nav(8, 1, False), ('walk', 148, 186), *charsel(-1),                                  # Gohan
        # the zenie from the grind buys food at Papaya Island's stall (s258: Onigiri, 200 HP for 1000 Z)
        nav(8, 1, False), ('walk', 280, 266), ('face', 'U'), ('press', A), ('wait', 60),
        ('fly', 1953, 2041, 143, 159), ('wait', 30),                                         # Papaya Island 7-1
        nav(7, 2, False), ('mark', 'r05_shop'), *shop(276, 590, 4, 12),
        nav(7, 1, False), ('walk', 224, 340), ('face', 'U'), ('press', A), ('wait', 60),
        ('fly', 849, 1785, 31, 23), ('wait', 30),                                            # Southwest Forest
        nav(8, 16, False), ('wait', 30),
    ]


def goals_real(e):
    """the chapter route from the real Earth end state"""
    g = goals(e)
    k = g.index(('mark', 'c04_8-16'))
    g = g[k:]
    # the ship's save points: Gohan again for the corridors (Vegeta after Pui Pui, Goku after Yakon are weaker)
    k = g.index(('mark', 'c07_8-28'))
    g[k + 1:k + 1] = [('walk', 60, 306), *charsel(-2)]
    k = g.index(('mark', 'c09_8-41'))
    # stage 3 (the hub rooms come back on every pass, Destroyers) wants more than 92: the six fighters next door in
    # 8-42 are easy experience (a level-up refills HP)
    g[k + 1:k + 1] = [('walk', 60, 314), *charsel(1)]
    import os
    if os.environ.get('S3') == 'walk':
        k1 = g.index(('mark', 'c11_8-66'))
        g[k:k1] = [('nav', x[1], x[2], False) if x[0] == 'nav' else x for x in g[k:k1]]
    return real_prologue(e) + g


def goals(e):
    pts = [e.r16(0x03001560 + 0x68 * c + 0x2C) for c in range(5)]
    return [
        # --- Southwest Forest (after s314; Gohan active) ------------------------------------------------------------
        ('mark', 'c01_8-1'),
        # spend everyone's stat points (tabs: Goku Gohan Goten Trunks Vegeta; the page opens on Gohan)
        alloc_keys([(0, split(pts[1])), (-1, split(pts[0])), (2, split(pts[2])), (1, split(pts[3])),
                    (1, split(pts[4]))]),
        # the save point: switch to Goku (the 8-2 gate wants Goku at level 70)
        ('walk', 148, 186), *charsel(-1),
        nav(8, 2),
        ('walk', 104, 56), ('face', 'U'), ('press', A), ('events',),       # s517: Goku opens the gate
        nav(8, 3), ('mark', 'c02_8-3'),
        nav(8, 7), nav(8, 11), ('mark', 'c03_8-11'),
        nav(8, 15, False),
        # the save point: back to Gohan (the ship's hatch wants Gohan at level 85)
        ('walk', 132, 90), *charsel(1),
        ('walk', 104, 30), ('hold', U, 40), ('area', 8, 16), ('wait', 30),
        ('mark', 'c04_8-16'),
        ('wait', 10), ('walk', 256, 150), ('events',), ('wait', 30),          # s319: Babidi's ship
        ('walk', 48, 112), ('face', 'U'), ('press', A), ('until', 8, 17), ('wait', 30),
        ('walk', 216, 236), ('hold', U, 20), ('until', 8, 18), ('wait', 30),  # Gohan opens the hatch (level 85)
        # --- Stage 1: three switches, Pui Pui --------------------------------------------------------------------
        ('mark', 'c05_8-18'),
        nav(8, 20), *switch(),
        nav(8, 23), *switch(),
        nav(8, 25), *switch(),
        nav(8, 26), ('mark', 'c06_8-26'),
        ('walk', 104, 340), ('hold', D, 60), ('expect', 200), ('wait', 30),   # s327 -> 8-27, Vegeta vs Pui Pui
        ('fightp', 8000, 99999, 6), ('wait', 200), ('events',), ('until', 8, 28), ('wait', 600),
        # --- Stage 2: three switches, Yakon -----------------------------------------------------------------------
        ('mark', 'c07_8-28'),
        nav(8, 30), *switch(),
        nav(8, 35), *switch(),
        nav(8, 37), *switch(),
        nav(8, 38), ('mark', 'c08_8-38'),
        ('walk', 104, 340), ('hold', D, 60), ('expect', 200), ('until', 8, 40), ('wait', 30),  # s328 -> Yakon
        ('skill', 0), ('yakon', 30000), ('until', 8, 41), ('wait', 60),
        # --- Stage 3: three Switch items (doors picked with the 8-63 buttons), three panels, Dabura -------------
        ('mark', 'c09_8-41'),
        nav(8, 63), *button(104),                 # middle: flag 65 opens 8-51's door
        nav(8, 52), *item(),                      # Switch (flag 68)
        nav(8, 63), *button(80),                  # left: flag 64, 8-60's door
        nav(8, 61), *item(),                      # Switch (flag 67)
        nav(8, 63), *button(136),                 # right: flag 66, 8-56's door
        nav(8, 57), *item(),                      # Switch (flag 69)
        ('mark', 'c10_switches'),
        nav(8, 65), *panel(),                     # s335 (flag 72)
        nav(8, 44), *panel(),                     # s330 (flag 70)
        nav(8, 47), *panel(),                     # s331 (flag 71)
        nav(8, 66, False),                        # straight on (the rooms behind came back)
        # the save point before Dabura: Goku stands in while Gohan rests in reserve (+5% HP every ~10 s)
        ('walk', 60, 306), *charsel(-1), ('rest', 1), ('walk', 60, 306), *charsel(1),
        ('mark', 'c11_8-66'),
        ('walk', 104, 340), ('hold', D, 60), ('expect', 200), ('wait', 30),   # s336 -> Dabura
        # --- Dabura (8-68, scripted), Babidi's spell on Vegeta, Goku vs Majin Vegeta (8-70: four 8500-HP rounds,
        # s325 / s338 / s340 / s341), Vegeta left standing in Rocky Canyon ---------------------------------------------
        ('mark', 'c12_8-68'),
        ('story', 8, 70, True),
        # --- Vegeta: the capsule (flag 73) back to Southwest Forest, the ship's door (s342) -> Giant Crater 8-71 -----
        ('mark', 'c13_8-70'),
        ('walk', 272, 280), ('face', 'U'), ('press', A), ('wait', 60),
        ('fly', 849, 1785, 31, 23), ('wait', 30),
        nav(8, 15), nav(8, 16, False),
        ('walk', 48, 112), ('face', 'U'), ('press', A),
        ('mark', 'c14_s342'),
        ('story', 8, 70, True),                   # Vegeta vs Buu (8-71, s344) -> Goku
        ('mark', 'c15_after_buu'),
    ]


def goals_fatbuu(e):
    """The Majin Buu arc from `c15_after_buu` (Goku in Rocky Canyon 8-70; work/ship/real4b/c15_after_buu.ss), work in
    progress: Kami's Lookout (s214, flag 76: Goten + Trunks) -> Bulma's Dragon Radar (s193, flag 77) -> West City
    prep. Next: the seven Dragon Balls (flags 112 .. 118: Thieves Den 9, Diablo Desert 11, Pilaf's Castle 10, the
    Airship 12, Fukurou Forest 13, Kyodai Pyramid 14, Nataday Village 15), s017 (119), s027 / s218 (120).

    Thieves Den needs a Bandana (equipment 126) worn: only the Stolen Airship encounter's (0-55) first four enemy
    slots drop it (on-defeat 0x08073EFC, 25%), and s508 spawns those slots only for a current character of level
    <= 70 - Trunks (65), not Goten (75)."""
    return [
        ('mark', 'd00_8-70'),
        ('walk', 272, 280), ('face', 'U'), ('press', A), ('wait', 60),
        ('fly', 633, 1065, 31, 23), ('wait', 30),                              # Kami's Lookout 6-1
        ('walk', 310, 560), ('story', 6, 1, True),                             # s214: now Goten
        ('mark', 'd01_goten'),
        ('walk', 310, 560), ('face', 'D'), ('press', A), ('wait', 60),         # the edge: the world map
        ('fly', 1137, 1137, 71, 119), ('wait', 30),                            # West City 4-1
        nav(4, 27, False), ('walk', 144, 194), ('talk', 'U'), ('events',),     # Bulma: the Dragon Radar
        ('mark', 'd02_radar'),
        # West City: Trunks (save point), his 15 stat points, five Hamburgers at the Z-Mart (s030), the capsule
        nav(4, 1, False), ('walk', 148, 594), *charsel(1),
        alloc_keys([(0, [8, 0, 7, 0])]),
        ('walk', 264, 310), ('hold', U, 40), ('wait', 30),
        ('unstick',), ('walk', 102, 124), ('hold', L, 10),
        ('keys', [(A, 3, 60), (A, 3, 60), (A, 3, 100), (D, 3, 15), (D, 3, 15)] + [(A, 3, 40)] * 5 +
         [(B, 3, 80), (B, 3, 60), (D, 3, 15), (D, 3, 15), (A, 3, 60), (A, 3, 60)]), ('events',), ('wait', 30),
        ('unstick',), ('walk', 112, 212), ('hold', D, 40), ('wait', 30),
        ('unstick',), ('walk', 416, 360), ('face', 'U'), ('press', A), ('wait', 60),
        ('mark', 'd03_hunt'),
        ('hunt', 126),                                                         # world map encounters: the Bandana
    ]
