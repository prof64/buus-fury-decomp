"""The chapter route (Southwest Forest -> Babidi's ship -> Majin Vegeta -> Majin Buu): ShipDriver goals in segments.
A segment starts where the previous ended (the recorder saves an emulator state at each segment start).

`segments(e)` builds the list from the start state (stat allocation reads the unspent points)."""
from ship_driver import alloc_keys, KEY

U, D, L, R, A, B = KEY.UP, KEY.DOWN, KEY.LEFT, KEY.RIGHT, KEY.A, KEY.B


def exit_(x, y, key, w, a):
    return [('walk', x, y), ('hold', key, 30), ('area', w, a), ('wait', 30)]


def switch(x=104, y=161):
    """a ship switch pedestal (rect (97,137) 31x23, A facing up); its script shows the door in 8-26 / 8-38 / 8-66"""
    return [('walk', x, y), ('face', 'U'), ('press', A), ('wait', 500)]


def charsel(moves):
    """save point menu -> Switch Characters -> LEFT/RIGHT `moves` -> A (standing at a save point, facing it)"""
    d = R if moves > 0 else L
    k = [(A, 3, 40), (D, 3, 12), (A, 3, 80)] + [(d, 3, 20)] * abs(moves) + [(A, 3, 60)]
    return [('face', 'U'), ('keys', k), ('events',)]


def split(n):
    """unspent stat points -> [STR, POW, END, SPD]: half STR, half END"""
    return [n - n // 2, 0, n // 2, 0]


def segments(e):
    pts = [e.r16(0x03001560 + 0x68 * c + 0x2C) for c in range(5)]
    S = []
    # --- Southwest Forest 8-1 (after s314; Gohan active) -----------------------------------------------------------
    S.append(('8-1', [
        # spend everyone's stat points (tabs: Goku Gohan Goten Trunks Vegeta; the page opens on Gohan)
        alloc_keys([(0, split(pts[1])), (-1, split(pts[0])), (2, split(pts[2])), (1, split(pts[3])),
                    (1, split(pts[4]))]),
        # the save point: switch to Goku (the 8-2 gate wants Goku at level 70)
        ('walk', 148, 186), *charsel(-1),
        *exit_(270, 24, U, 8, 2),
    ]))
    S.append(('8-2', [
        ('walk', 104, 56), ('face', 'U'), ('press', A), ('events',),       # s517: Goku opens the gate
        *exit_(104, 24, U, 8, 3),
    ]))
    S.append(('8-3', [('fightp', 8000), ('wait', 30), *exit_(80, 24, U, 8, 6)]))
    S.append(('8-6', [('fightp', 8000), ('wait', 30), *exit_(96, 24, U, 8, 7)]))
    S.append(('8-7', [('fightp', 8000), ('wait', 30), *exit_(100, 24, U, 8, 11)]))
    S.append(('8-11', [*exit_(16, 104, L, 8, 12)]))
    S.append(('8-12', [('wait', 10), *exit_(184, 24, U, 8, 13)]))
    S.append(('8-13', [*exit_(104, 24, U, 8, 14)]))
    S.append(('8-14', [*exit_(360, 96, R, 8, 15)]))
    S.append(('8-15', [
        # the save point: back to Gohan (the ship's hatch wants Gohan at level 85)
        ('walk', 132, 100), *charsel(1),
        ('walk', 104, 30), ('hold', U, 40), ('area', 8, 16), ('wait', 30),
    ]))
    S.append(('8-16', [
        ('wait', 10), ('walk', 256, 150), ('events',), ('wait', 30),          # s319: Babidi's ship
        ('walk', 48, 112), ('face', 'U'), ('press', A), ('until', 8, 17), ('wait', 30),
    ]))
    S.append(('8-17', [('walk', 216, 236), ('hold', U, 20), ('until', 8, 18), ('wait', 30)]))
    # --- Stage 1: three switches, Pui Pui -----------------------------------------------------------------------
    S.append(('8-18', [*exit_(104, 360, D, 8, 19), *exit_(340, 196, R, 8, 20)]))
    S.append(('8-20', [*switch(), *exit_(8, 132, L, 8, 19), *exit_(184, 330, D, 8, 21), *exit_(104, 360, D, 8, 22),
                       *exit_(24, 200, L, 8, 23)]))
    S.append(('8-23', [*switch(), *exit_(200, 132, R, 8, 22), *exit_(344, 200, R, 8, 24), *exit_(360, 132, R, 8, 25)]))
    S.append(('8-25', [*switch(), *exit_(8, 132, L, 8, 24), *exit_(16, 132, L, 8, 22), *exit_(184, 330, D, 8, 26)]))
    S.append(('8-26', [('walk', 104, 340), ('hold', D, 60), ('expect', 200), ('wait', 30)]))        # s327
    S.append(('8-27', [('fightp', 8000), ('wait', 200), ('events',), ('until', 8, 28), ('wait', 600)]))  # Pui Pui
    return S
