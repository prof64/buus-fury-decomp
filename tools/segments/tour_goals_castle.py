"""field coverage tours of world 8 (Southwest Forest, Babidi's Ship), world 9 (Thieves Den, Lighthouse) and world 10
(Pilaf's Castle); imported by run_tours.py. States: work/states/tour_castle_*.ss (tools/run2/state_at.py rows)."""
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from tours import area_goals  # noqa: E402

A = 1
B = 2
UP, DOWN, LEFT, RIGHT = 64, 128, 32, 16


def brk(x0, y0, x1, y1, side='D'):
    """punch a breakable prop (hit box x0, y0, x1, y1) three times from below it facing up (side 'D'), or from its
    left facing right (side 'L'), or from its right facing left (side 'R')"""
    at = {'D': ((x0 + x1) // 2 - 8, y1 + 1, 'U'), 'L': (x0 - 17, (y0 + y1) // 2 - 4, 'R'),
          'R': (x1 + 1, (y0 + y1) // 2 - 4, 'L')}[side]
    if side != 'D':
        # a punch from the side knocks the player back: walk back in for each of the three
        return [('walk',) + at[:2], ('a_at',) + at] * 3 + [('wait', 20)]
    g = [('walk',) + at[:2], ('a_at',) + at, ('wait', 20)]
    for _ in range(3):
        g += [('hold', A, 3), ('wait', 30)]
    return g


def go(path, w, a):
    """walk the points of path (the last one inside an exit rect), then hold on until the area is W-A"""
    return [('walk', x, y) for x, y in path] + [('area', w, a), ('wait', 40)]


# Southwest Forest west side (state tour_castle_f3b.ss: run 2 row 632,219, 8-3): 8-4 (never entered), 8-5 (never
# entered: its A trigger s315 and two breakables), back through 8-4 to 8-3
FOREST_W = (go([(30, 270), (8, 270)], 8, 4)
            + [('fight', 5000)]
            + go([(30, 120), (8, 120)], 8, 5)
            + [('fight', 5000)]
            # the chest (prop 6, 112..128 x 384..392) from below, facing up
            + [('a_at', 112, 394, 'U')]
            + brk(57, 360, 87, 376) + brk(153, 344, 183, 360)
            + go([(190, 130), (208, 130)], 8, 4)
            + go([(350, 110), (368, 110)], 8, 3))

# Southwest Forest north side: 8-7's level gate (A trigger, s518), 8-8, 8-9, 8-10 (never entered:
# breakables, 8-10's chest s316)
NORTH = ([('fight', 5000), ('a_at', 447, 72, 'U')]
         + go([(447, 40), (447, 8)], 8, 8)
         + [('fight', 5000)] + brk(145, 112, 175, 128)
         + go([(110, 30), (110, 8)], 8, 9)
         + brk(225, 48, 255, 64)
         + go([(370, 30), (370, 8)], 8, 10)
         + [('fight', 5000)]
         # the two rocks wall off the chest (prop 3, 184..200 x 56..64; A trigger s316)
         + brk(97, 64, 127, 80) + brk(145, 80, 175, 96, 'L')
         + [('a_at', 184, 66, 'U')])
# (state tour_castle_g87.ss = work/side/pg_g87.ss: post-game, Vegeta L120 just through 8-7's gate, the per-visit
# Vegeta L120 gate (tools/side/pgroute.py leg g87); no run reaches 8-8 .. 8-10 in the story)
FOREST_N = NORTH
# late game (state tour_castle_f1late.ss: run 1 row 1,282,460, 8-1, Gohan L141): 8-1's two breakables, 8-2's tier-2
# gate (A up: "Gohan cannot open this gate!"; it wants Goku L100, dead at this point of the story - post-game only)
FOREST_LATE = ([('wait', 60)] + brk(233, 88, 263, 104) + brk(273, 112, 303, 128, 'L')
               + go([(280, 30), (280, 8)], 8, 2)
               + [('walk', 104, 58), ('hold', UP, 20), ('wait', 5), ('hold', A, 3), ('wait', 60), ('events',)])


def save_point(x, y):
    """stand at (x, y) in a save point's rect, face up, A: the menu; B closes it"""
    return [('walk', x, y), ('hold', UP, 3), ('wait', 4), ('hold', A, 3), ('wait', 60), ('hold', B, 3), ('wait', 60),
            ('events',)]


# Southwest Forest 8-12 .. 8-16 before Babidi's Ship (state tour_castle_f12b.ss: run 3 row 896,741, 8-12): 8-12's
# NPC (s317: its runtime object is the plain-NPC kind, so A from below) and item 79, 8-13, 8-14's three
# breakables, 8-15's save point (item 79 does not spawn: it wants flag 361)
FOREST_MID = ([('a_at', 280, 354, 'U'), ('walk', 304, 348)]
              + go([(184, 30), (184, 8)], 8, 13)
              + go([(104, 30), (104, 8)], 8, 14)
              + brk(145, 72, 175, 88) + brk(97, 64, 127, 80) + brk(177, 40, 207, 56)
              + go([(360, 92), (368, 92)], 8, 15)
              + save_point(132, 76))
# 8-16 (state tour_castle_f15.ss: castle_forest_mid's frame 2,860 - past the save point, walking up to 8-16's exit)
FOREST_16 = (go([(104, 8)], 8, 16)
             # the arrival cutscene (s319, ~5,300 frames: the ship and back)
             + [('walk', 230, 150), ('events',), ('events',)]
             # the Z fighters (s321 .. s323, s320; some are plain-NPC objects at runtime): A from below each
             + [('a_at', 192, 130, 'U'), ('a_at', 240, 122, 'U'), ('a_at', 200, 154, 'U'), ('a_at', 168, 138, 'U')])
# the talks alone (state tour_castle_f16.ss: castle_forest_16's frame 6,440, the cutscene over)
FOREST_16B = [('a_at', 192, 130, 'U'), ('a_at', 240, 122, 'U'), ('a_at', 200, 154, 'U'), ('a_at', 168, 138, 'U')]


def talk_at(nx, ny):
    """A from below an NPC standing at (nx, ny) (works for the NPCs whose runtime object is the plain kind too)"""
    return [('a_at', nx, ny + 10, 'U')]


# Thieves Den east and the Lighthouse (state tour_castle_den6.ss: run 1 row 728,242, 9-6): 9-7, 9-8 (never entered;
# its touch event s346), the Lighthouse 9-9 (never entered: its keeper, s347) and its top 9-17 (never entered)
DEN_9 = (talk_at(112, 80)
         + [('walk', 160, 32), ('hold', UP, 30), ('area', 9, 17), ('wait', 40)]
         + [('walk', 120, 140), ('wait', 30)])
DEN_8 = ([('walk', 64, 352), ('events',)]
         + go([(240, 264), (260, 256)], 9, 9)
         + DEN_9)
DEN_EAST = (go([(440, 150), (450, 150)], 9, 7)
            + go([(160, 200), (160, 214)], 9, 8)
            + DEN_8)
# (the same from later starts, to look past early divergences: tour_castle_den8.ss / den9.ss = castle_den_east's
# frames 495 (9-8 just entered) / 987 (9-9 just entered))

# Thieves Den town (state tour_castle_den10b.ss: run 1 row 729,260, 9-10): 9-10's second NPC (s349), 9-11 (its save
# point, four NPCs; its A trigger s356 sits in a walled-off pocket at the top, out of reach), 9-16 (never entered),
# back to 9-11, 9-12 .. 9-15 (the three NPCs there)
DEN_TOWN = (talk_at(136, 80) + talk_at(112, 64)
            + [('walk', 108, 40), ('hold', UP, 30), ('area', 9, 11), ('wait', 40)]
            + save_point(172, 256))
# the rest (state tour_castle_den11.ss: castle_den_town's frame 798, the save point's menu closed)
DEN_TOWN_B = (talk_at(240, 240) + talk_at(80, 240) + [('walk', 120, 232)] + talk_at(88, 144) + talk_at(216, 80)
            + [('walk', 64, 136), ('walk', 58, 116), ('hold', LEFT, 30), ('area', 9, 16), ('wait', 40)]
            + go([(108, 200), (108, 214)], 9, 11)
            + [('walk', 268, 84), ('walk', 268, 62), ('hold', UP, 30), ('area', 9, 12), ('wait', 40)]
            + go([(172, 40), (172, 28)], 9, 13)
            + go([(44, 40), (44, 28)], 9, 14)
            + go([(172, 40), (172, 28)], 9, 15)
            + [('events',)] + talk_at(80, 136) + talk_at(88, 160) + talk_at(104, 176)
            + [('a_at', 110, 184, 'R')])


def wall(x, y):
    """a cracked wall (A-up script trigger): stand at (x, y) pushing up, A three times (as run 1 did at 10-15)"""
    return [('walk', x, y), ('hold', UP, 20)] + [('hold', A, 3), ('hold', UP, 21)] * 3 + [('wait', 60)]


def door(x, y, key, w, a):
    """walk to (x, y) beside an exit rect, hold KEY into it until the area is W-A"""
    return [('walk', x, y), ('hold', key, 30), ('area', w, a), ('wait', 40)]


# Pilaf's Castle west wing (state tour_castle_p5.ss: run 2 row 894,560, 10-5 on the first way in), all never
# entered: 10-6 (its cracked wall: A up, opens the way on), 10-7, 10-8, 10-9, 10-10, 10-16 (wall), 10-17 (wall),
# 10-25, 10-26 (wall) -> 10-31
PW = [  # (room, goals from its arrival to the next room's)
    ('10-5', door(24, 12, UP, 10, 6)),
    ('10-6', [('fight', 5000)] + wall(16, 64) + [('walk', 24, 60)] + door(24, 50, UP, 10, 7)),
    ('10-7', door(40, 64, DOWN, 10, 8)),
    ('10-8', door(104, 16, UP, 10, 9)),
    ('10-9', door(24, 16, UP, 10, 10)),
    ('10-10', door(184, 56, UP, 10, 16)),
    ('10-16', [('fight', 5000)] + wall(80, 72) + [('walk', 88, 72)] + door(88, 50, UP, 10, 17)),
    ('10-17', [('fight', 5000)] + wall(40, 72) + [('walk', 48, 72)] + door(48, 50, UP, 10, 25)),
    ('10-25', door(150, 12, UP, 10, 26)),
    ('10-26', [('fight', 5000)] + wall(200, 56) + [('walk', 208, 56)] + door(208, 30, UP, 10, 31)),
    ('10-31', [('events',), ('wait', 60)]),
]
CASTLE_W = [g for _, gs in PW for g in gs]
# the same from each room's arrival (states tour_castle_pw_F.ss: castle_pilaf_w's frame F, just inside the room), to
# look past early divergences: castle_pw_10_6 .. castle_pw_10_31 run from that room to the end
PW_FRAMES = {'10-6': 325, '10-7': 885, '10-8': 1027, '10-9': 1216, '10-10': 1627, '10-16': 1887, '10-17': 2583,
             '10-25': 5987, '10-26': 6230, '10-31': 6779}
PW_TOURS = {}
for k, (room, _) in enumerate(PW):
    if room in PW_FRAMES:
        PW_TOURS['castle_pw_' + room.replace('-', '_')] = (f'tour_castle_pw_{PW_FRAMES[room]}.ss',
                                                          [g for _, gs in PW[k:] for g in gs])

# Pilaf's Castle east side (state tour_castle_p28.ss: run 2 row 912,849, 10-28 on the way back): 10-28's cracked
# wall -> 10-29 (never entered), back through 10-28, 10-27, 10-21 to 10-24 (never entered), 10-21, 10-22 (never
# entered), 10-19 (never entered), 10-23 (never entered)
CASTLE_E = ([('fight', 5000)] + wall(184, 72) + [('walk', 192, 72)] + door(192, 50, UP, 10, 29)
            + door(108, 136, DOWN, 10, 28)
            + door(266, 140, DOWN, 10, 27)
            + door(28, 136, DOWN, 10, 21)
            + door(8, 184, LEFT, 10, 24)
            + door(222, 104, RIGHT, 10, 21)
            + door(222, 184, RIGHT, 10, 22)
            + door(124, 140, DOWN, 10, 19)
            + door(172, 40, UP, 10, 23)
            + [('wait', 60)])
# .. and on: back down 10-19, 10-22, 10-21, 10-20, 10-15, then 10-18 (never entered: its cracked wall) -> 10-19
CASTLE_E2 = ([('walk', 80, 200)] + door(10, 280, DOWN, 10, 19)
             + door(44, 40, UP, 10, 22)
             + door(8, 104, LEFT, 10, 21)
             + door(108, 296, DOWN, 10, 20)
             + door(108, 216, DOWN, 10, 15)
             + door(208, 104, RIGHT, 10, 18)
             + wall(208, 72) + [('walk', 216, 72)] + door(216, 50, UP, 10, 19)
             + [('wait', 60)])
# 10-11 (never entered; state tour_castle_p5.ss, 10-5): in and back out
CASTLE_11 = door(344, 12, UP, 10, 11) + [('walk', 120, 120), ('walk', 24, 216), ('hold', DOWN, 30), ('area', 10, 5),
                                         ('wait', 40)]
# 10-1's tier-2 / tier-3 gates (state tour_castle_p1.ss: run 1 row 744,993, 10-1, Trunks; Goten L120 / L145 wanted:
# "cannot open"), then 10-14's save point (state tour_castle_p14.ss: run 1 row 761,751, 10-14)
CASTLE_GATE = [('walk', 248, 72), ('hold', UP, 20), ('wait', 5), ('hold', A, 3), ('wait', 60), ('events',)]
CASTLE_SAVE14 = save_point(187, 140)

# Babidi's Ship (states tour_castle_sA.ss: run 3, 40 rows into each room 8-A on the way through): per room, its
# breakables (and A triggers), then its save point last (the menu's B close is a known divergence)
SAVE_A = (60, 310)       # save point rect (48, 280, 41, 41)
SAVE_B = (60, 305)       # save point rect (54, 285, 29, 28)


def brk2(x0, y0, x1, y1, side='D'):
    """brk without the walk before each a_at (a walk stuck 4,000 frames hands the a_at a running goal clock)"""
    return [g for g in brk(x0, y0, x1, y1, side) if g[0] != 'walk']


# (the sides: castle_propgrid.py - the ship's walls hug these props on one side)
BRK_18 = [('a_at', 151, 277, 'R')] * 3 + [('a_at', 151, 309, 'R')] * 3 + [('wait', 20)]
BRK_22 = brk2(230, 273, 250, 288) + brk2(262, 281, 282, 296)
BRK_21 = brk2(46, 73, 66, 88)
BRK_26 = [('a_at', 151, 277, 'R')] * 3 + [('wait', 20)]
SHIP = {
    # outside: four breakables, last the hatch's A script (out to 8-16); the A trigger s326 (a chest on the cliff
    # top, 425, 65) is out of reach from here
    '17': brk(345, 152, 375, 168, 'R') + brk(353, 248, 383, 264, 'R') + brk(129, 152, 159, 168, 'R')
          + brk(73, 288, 103, 304, 'R')
          + [('a_at', 216, 312, 'U'), ('wait', 60)],
    '18': BRK_18 + save_point(*SAVE_A),
    '19': brk(70, 105, 90, 120, 'R'),
    '21': BRK_21,
    '22': BRK_22,
    '26': BRK_26 + save_point(*SAVE_B),
    '28': BRK_18 + save_point(*SAVE_B),
    # '32' (8-22's map): left out - run 3's player there has 286 HP and dies to the room's enemies
    '38': BRK_26 + save_point(*SAVE_B),
    '41': BRK_18 + save_point(*SAVE_A),
    '42': BRK_22,
    '48': BRK_21,
    '52': BRK_21,
    '53': BRK_21,
    '57': BRK_21,
    '61': BRK_21,
    '66': BRK_26 + save_point(*SAVE_B),
}
SHIP_TOURS = {f'castle_ship_{a}': (f'tour_castle_s{a}.ss', [('wait', 20)] + g) for a, g in SHIP.items()}


TOURS = {'castle_forest_w': ('tour_castle_f3b.ss', FOREST_W),
         'castle_forest_n': ('tour_castle_g87.ss', FOREST_N),
         'castle_forest_late': ('tour_castle_f1late.ss', FOREST_LATE),
         'castle_forest_mid': ('tour_castle_f12b.ss', FOREST_MID),
         'castle_forest_16': ('tour_castle_f15.ss', FOREST_16),
         'castle_den_east': ('tour_castle_den6.ss', DEN_EAST),
         'castle_den_8': ('tour_castle_den8.ss', DEN_8),
         'castle_den_9': ('tour_castle_den9.ss', DEN_9),
         'castle_den_town': ('tour_castle_den10b.ss', DEN_TOWN),
         'castle_den_town_b': ('tour_castle_den11.ss', DEN_TOWN_B),
         'castle_pilaf_w': ('tour_castle_p5.ss', CASTLE_W),
         'castle_pilaf_e': ('tour_castle_p28.ss', CASTLE_E + CASTLE_E2),
         'castle_pilaf_11': ('tour_castle_p5.ss', CASTLE_11),
         'castle_pilaf_gate': ('tour_castle_p1.ss', CASTLE_GATE),
         'castle_pilaf_save14': ('tour_castle_p14.ss', CASTLE_SAVE14),
         'castle_forest_16b': ('tour_castle_f16.ss', FOREST_16B),
         **PW_TOURS, **SHIP_TOURS}
