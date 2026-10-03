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
        nav(8, 66), ('mark', 'c11_8-66'),
        ('walk', 104, 340), ('hold', D, 60), ('expect', 200), ('wait', 30),   # s336 -> Dabura
    ]
