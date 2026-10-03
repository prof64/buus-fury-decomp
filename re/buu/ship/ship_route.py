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
    S.append(('8-3', [('fightp', 8000, 99999, 6), ('wait', 30), *exit_(80, 24, U, 8, 6)]))
    S.append(('8-6', [('fightp', 8000, 99999, 6), ('wait', 30), *exit_(96, 24, U, 8, 7)]))
    S.append(('8-7', [('fightp', 8000, 99999, 6), ('wait', 30), *exit_(100, 24, U, 8, 11)]))
    S.append(('8-11', [*exit_(16, 104, L, 8, 12)]))
    S.append(('8-12', [('wait', 10), *exit_(184, 24, U, 8, 13)]))
    S.append(('8-13', [*exit_(104, 24, U, 8, 14)]))
    S.append(('8-14', [*exit_(360, 96, R, 8, 15)]))
    S.append(('8-15', [
        # the save point: back to Gohan (the ship's hatch wants Gohan at level 85)
        ('walk', 132, 90), *charsel(1),
        ('walk', 104, 30), ('hold', U, 40), ('area', 8, 16), ('wait', 30),
    ]))
    S.append(('8-16', [
        ('wait', 10), ('walk', 256, 150), ('events',), ('wait', 30),          # s319: Babidi's ship
        ('walk', 48, 112), ('face', 'U'), ('press', A), ('until', 8, 17), ('wait', 30),
    ]))
    S.append(('8-17', [('walk', 216, 236), ('hold', U, 20), ('until', 8, 18), ('wait', 30)]))
    # --- Stage 1: three switches, Pui Pui -----------------------------------------------------------------------
    F = ('fightp', 8000, 99999, 6)
    S.append(('8-18', [*exit_(104, 360, D, 8, 19)]))
    S.append(('8-19', [F, ('wait', 20), *exit_(340, 196, R, 8, 20)]))
    S.append(('8-20', [*switch(), *exit_(8, 132, L, 8, 19)]))
    S.append(('8-19b', [F, ('wait', 20), *exit_(184, 330, D, 8, 21)]))
    S.append(('8-21', [F, ('wait', 20), *exit_(104, 360, D, 8, 22)]))
    S.append(('8-22', [F, ('wait', 20), *exit_(24, 200, L, 8, 23)]))
    S.append(('8-23', [*switch(), *exit_(200, 132, R, 8, 22)]))
    S.append(('8-22b', [F, ('wait', 20), *exit_(344, 200, R, 8, 24)]))
    S.append(('8-24', [F, ('wait', 20), *exit_(360, 132, R, 8, 25)]))
    S.append(('8-25', [*switch(), *exit_(8, 132, L, 8, 24)]))
    S.append(('8-24b', [F, ('wait', 20), *exit_(16, 132, L, 8, 22)]))
    S.append(('8-22c', [F, ('wait', 20), *exit_(184, 330, D, 8, 26)]))
    S.append(('8-26', [('walk', 104, 340), ('hold', D, 60), ('expect', 200), ('wait', 30)]))        # s327
    S.append(('8-27', [('fightp', 8000, 99999, 6), ('wait', 200), ('events',), ('until', 8, 28), ('wait', 600)]))  # Pui Pui
    # --- Stage 2: three switches, Yakon -------------------------------------------------------------------------
    S.append(('8-28', [*exit_(104, 360, D, 8, 29)]))
    S.append(('8-29', [F, ('wait', 20), *exit_(24, 200, L, 8, 30)]))
    S.append(('8-30', [*switch(), *exit_(200, 132, R, 8, 29)]))
    S.append(('8-29b', [F, ('wait', 20), *exit_(184, 330, D, 8, 31)]))
    S.append(('8-31', [F, ('wait', 20), *exit_(104, 360, D, 8, 32)]))
    S.append(('8-32', [F, ('wait', 20), *exit_(24, 200, L, 8, 33)]))
    S.append(('8-33', [F, ('wait', 20), *exit_(16, 132, L, 8, 34)]))
    S.append(('8-34', [F, ('wait', 20), *exit_(104, 216, D, 8, 35)]))
    S.append(('8-35', [*switch(), ('walk', 80, 120), ('walk', 104, 86), ('wait', 20), *exit_(104, 48, U, 8, 34)]))
    S.append(('8-34b', [F, ('wait', 20), *exit_(200, 132, R, 8, 33)]))
    S.append(('8-33b', [F, ('wait', 20), *exit_(360, 132, R, 8, 32)]))
    S.append(('8-32b', [F, ('wait', 20), *exit_(344, 200, R, 8, 36)]))
    S.append(('8-36', [F, ('wait', 20), *exit_(360, 132, R, 8, 37)]))
    S.append(('8-37', [*switch(), *exit_(8, 132, L, 8, 36)]))
    S.append(('8-36b', [F, ('wait', 20), *exit_(16, 132, L, 8, 32)]))
    S.append(('8-32c', [F, ('wait', 20), *exit_(184, 330, D, 8, 38)]))
    S.append(('8-38', [('walk', 104, 340), ('hold', D, 60), ('expect', 200), ('until', 8, 40), ('wait', 30)]))  # s328
    S.append(('8-40', [('skill', 0),                                                         # Energy Blast
                       ('yakon', 30000), ('until', 8, 41), ('wait', 60)]))
    # --- Stage 3: the three Switch items (doors picked with the 8-63 buttons), three panels, Dabura -------------
    def room(seq):
        """[(label, [goals...])] -> segments; each label is a room entry"""
        for lab, gl in seq:
            S.append((lab, gl))
    # room exits by layout: 024 (wide corridor), 051-type (cross room), 021 (tall), 022 (hub)
    LW = lambda w, a: exit_(8, 132, L, w, a)          # noqa: E731  left door of a small / wide room
    RW = lambda w, a: exit_(360, 132, R, w, a)        # noqa: E731  right door of a wide (024) room
    RS = lambda w, a: exit_(200, 132, R, w, a)        # noqa: E731  right door of a small room
    US = lambda w, a: [('walk', 80, 120), ('walk', 104, 86), ('wait', 20), *exit_(104, 48, U, w, a)]  # noqa: E731
    DS = lambda w, a: exit_(104, 216, D, w, a)        # noqa: E731  bottom door of a small room
    DT = lambda w, a: exit_(104, 360, D, w, a)        # noqa: E731  bottom door of a tall (021) room
    UT = lambda w, a: [('walk', 104, 86), ('wait', 20), *exit_(104, 48, U, w, a)]  # noqa: E731
    HL = lambda w, a: exit_(24, 200, L, w, a)         # noqa: E731  hub (022) left
    HR = lambda w, a: exit_(344, 200, R, w, a)        # noqa: E731  hub right
    HD = lambda w, a: exit_(184, 330, D, w, a)        # noqa: E731  hub down
    HU = lambda w, a: exit_(184, 72, U, w, a)         # noqa: E731  hub up

    def button(x):
        return [('walk', x, 96), ('face', 'U'), ('press', A), ('wait', 700)]

    def item():
        return [('walk', 104, 136), ('face', 'U'), ('press', A), ('events',), ('wait', 30)]

    def panel():
        return [('walk', 104, 161), ('face', 'U'), ('press', A), ('events',), ('wait', 60)]
    room([
        ('8-41', [*DT(8, 42)]),
        ('8-42', [F, ('wait', 20), *HD(8, 48)]),
        ('8-48', [F, ('wait', 20), *DT(8, 49)]),
        ('8-49', [F, ('wait', 20), *HL(8, 50)]),
        ('8-50', [F, ('wait', 20), *LW(8, 51)]),
        ('8-51', [F, ('wait', 20), *LW(8, 58)]),
        ('8-58', [F, ('wait', 20), *LW(8, 59)]),
        ('8-59', [F, ('wait', 20), *LW(8, 62)]),
        ('8-62', [F, ('wait', 20), *LW(8, 63)]),
        ('8-63', [*button(104), *RS(8, 62)]),                       # middle: flag 65, 8-51's door
        ('8-62b', [F, ('wait', 20), *RW(8, 59)]),
        ('8-59b', [F, ('wait', 20), *RS(8, 58)]),
        ('8-58b', [F, ('wait', 20), *RW(8, 51)]),
        ('8-51b', [F, ('wait', 20), *US(8, 52)]),
        ('8-52', [*item(), *DS(8, 51)]),                            # Switch 1 (flag 68)
        ('8-51c', [F, ('wait', 20), *LW(8, 58)]),
        ('8-58c', [F, ('wait', 20), *LW(8, 59)]),
        ('8-59c', [F, ('wait', 20), *LW(8, 62)]),
        ('8-62c', [F, ('wait', 20), *LW(8, 63)]),
        ('8-63b', [*button(80), *RS(8, 62)]),                       # left: flag 64, 8-60's door
        ('8-62d', [F, ('wait', 20), *RW(8, 59)]),
        ('8-59d', [F, ('wait', 20), *US(8, 60)]),
        ('8-60', [F, ('wait', 20), *UT(8, 61)]),
        ('8-61', [*item(), *DS(8, 60)]),                            # Switch 2 (flag 67)
        ('8-60b', [F, ('wait', 20), *DT(8, 59)]),
        ('8-59e', [F, ('wait', 20), *LW(8, 62)]),
        ('8-62e', [F, ('wait', 20), *LW(8, 63)]),
        ('8-63c', [*button(136), *RS(8, 62)]),                      # right: flag 66, 8-56's door
        ('8-62f', [F, ('wait', 20), *RW(8, 59)]),
        ('8-59f', [F, ('wait', 20), *RS(8, 58)]),
        ('8-58d', [F, ('wait', 20), *RW(8, 51)]),
        ('8-51d', [F, ('wait', 20), *DS(8, 53)]),
        ('8-53', [F, ('wait', 20), *DT(8, 54)]),
        ('8-54', [F, ('wait', 20), *LW(8, 55)]),
        ('8-55', [F, ('wait', 20), *LW(8, 56)]),
        ('8-56', [F, ('wait', 20), *US(8, 57)]),
        ('8-57', [*item(), *DS(8, 56)]),                            # Switch 3 (flag 69)
        ('8-56b', [F, ('wait', 20), *RS(8, 55)]),
        ('8-55b', [F, ('wait', 20), *RW(8, 54)]),
        ('8-54b', [F, ('wait', 20), *US(8, 53)]),
        ('8-53b', [F, ('wait', 20), *UT(8, 51)]),
        ('8-51e', [F, ('wait', 20), *RS(8, 50)]),
        ('8-50b', [F, ('wait', 20), *RW(8, 49)]),
        ('8-49b', [F, ('wait', 20), *HR(8, 64)]),
        ('8-64', [F, ('wait', 20), *RW(8, 65)]),
        ('8-65', [*panel(), *LW(8, 64)]),                           # s335 (flag 72)
        ('8-64b', [F, ('wait', 20), *LW(8, 49)]),
        ('8-49c', [F, ('wait', 20), *HU(8, 48)]),
        ('8-48b', [F, ('wait', 20), *UT(8, 42)]),
        ('8-42b', [F, ('wait', 20), *HL(8, 43)]),
        ('8-43', [F, ('wait', 20), *LW(8, 44)]),
        ('8-44', [*panel(), *RS(8, 43)]),                           # s330 (flag 70)
        ('8-43b', [F, ('wait', 20), *RW(8, 42)]),
        ('8-42c', [F, ('wait', 20), *HR(8, 45)]),
        ('8-45', [F, ('wait', 20), *RW(8, 46)]),
        ('8-46', [F, ('wait', 20), *DS(8, 47)]),
        ('8-47', [*panel(), *US(8, 46)]),                           # s331 (flag 71)
        ('8-46b', [F, ('wait', 20), *LW(8, 45)]),
        ('8-45b', [F, ('wait', 20), *LW(8, 42)]),
        ('8-42d', [F, ('wait', 20), *HD(8, 48)]),
        ('8-48c', [F, ('wait', 20), *DT(8, 49)]),
        ('8-49d', [F, ('wait', 20), *HD(8, 66)]),
        ('8-66', [('walk', 104, 340), ('hold', D, 60), ('expect', 200), ('wait', 30)]),   # s336 -> Dabura
    ])
    return S
