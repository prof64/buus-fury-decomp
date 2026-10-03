"""The Earth chapters' route (Gohan: school, Great Saiyaman, Videl, Goten & Trunks, the World Tournament) as
earth_driver.py goals, played by the real game from the chapter start state (work/earth/chapter_start.ss: the story
playthrough route (route.py, New Game -> Pikkon -> Earth 2-1) played from the soft reset; see earth_play.py).

Each segment starts where the previous ended; earth_play.py saves the emulator state at every segment's start
(work/earth/seg_<name>.ss) and the key log."""
from route import KEY

U, D, L, R, A, B = KEY.UP, KEY.DOWN, KEY.LEFT, KEY.RIGHT, KEY.A, KEY.B

SEGMENTS = [
    # Goku's house 2-1 -> 2-2 -> 2-8: Chi-Chi's lunch (s110, flag 14), Nimbus (s111) -> Hercule City 3-1
    ('house', [
        ('walk', 120, 200), ('hold', D, 30), ('area', 2, 2), ('wait', 60),
        ('walk', 208, 410), ('hold', D, 30), ('area', 2, 8), ('wait', 60),
        ('walk', 240, 395), ('hold', D, 10), ('expect', 100), ('wait', 20),
        ('walk', 176, 385), ('until', 3, 1), ('wait', 20),
    ]),
    # Hercule City 3-1: the bank robbers (s125, two field enemies; flag 18) -> 3-2
    ('robbers', [
        ('walk', 240, 140), ('hold', R, 20), ('expect', 100), ('wait', 20), ('fight', 6000), ('wait', 30),
        ('events',), ('wait', 30),
    ]),
    # 3-2 -> 3-3 -> 3-10 -> 3-11 -> classroom 3-12: the lecture (s159, flag 20); back out: s129 "close call"
    # (flag 21) -> 3-1
    ('school', [
        ('walk', 96, 24), ('hold', U, 30), ('area', 3, 3), ('wait', 30),
        ('walk', 470, 24), ('hold', U, 30), ('area', 3, 10), ('wait', 30),
        ('walk', 144, 140), ('hold', U, 30), ('area', 3, 11), ('wait', 30),
        ('walk', 12, 92), ('hold', L, 30), ('area', 3, 12), ('expect', 200), ('wait', 30),
        ('walk', 212, 200), ('hold', R, 30), ('area', 3, 11), ('wait', 30),
        ('walk', 104, 350), ('hold', D, 20), ('until', 3, 1), ('wait', 30),
    ]),
    # 3-1 world map circle (s130 -> #515) -> fly to West City (4-1) -> Capsule Corp 4-20 -> Bulma's house 4-27: s193
    # (flag 23: get the number 2402 capsule)
    ('westcity', [
        ('walk', 40, 150), ('talk', 'L'), ('wait', 60),
        ('fly', 1193, 1113, 119, 183), ('wait', 30),
        ('walk', 850, 360), ('hold', R, 30), ('area', 4, 20), ('wait', 30),
        ('walk', 360, 200), ('hold', U, 30), ('area', 4, 27), ('wait', 30),
        ('talkto', 144, 200, 'U'), ('wait', 30),
    ]),
    # 4-27 -> 4-20 -> 4-21 -> 4-24 -> the capsule room 4-30: the map item (s197, flag 24) -> back to Bulma (s194:
    # the Great Saiyaman, flag 25)
    ('capsule', [
        ('walk', 152, 290), ('hold', D, 30), ('area', 4, 20), ('wait', 30),
        ('walk', 232, 64), ('hold', U, 30), ('area', 4, 21), ('wait', 30),
        ('walk', 112, 56), ('hold', U, 30), ('area', 4, 24), ('wait', 30),
        ('walk', 592, 56), ('hold', U, 30), ('area', 4, 30), ('wait', 30),
        ('walk', 184, 128), ('talk', 'U'), ('wait', 100), ('events',), ('wait', 30),
        ('walk', 152, 290), ('hold', D, 30), ('area', 4, 24), ('wait', 30),
        ('walk', 352, 600), ('hold', D, 30), ('area', 4, 21), ('wait', 30),
        ('walk', 112, 212), ('hold', D, 30), ('area', 4, 20), ('wait', 30),
        ('walk', 360, 200), ('hold', U, 30), ('area', 4, 27), ('wait', 30),
        ('talkto', 144, 200, 'U'), ('wait', 30),
    ]),
    # back to Hercule City: 4-27 -> 4-20 -> 4-1, the world map circle -> 3-1: s131 "Stick 'em up!" (Great Saiyaman,
    # flag 26) and the robber
    ('stickup', [
        ('walk', 152, 290), ('hold', D, 30), ('area', 4, 20), ('wait', 30),
        ('walk', 4, 390), ('hold', L, 30), ('area', 4, 1), ('wait', 30),
        ('walk', 410, 356), ('talk', 'U'), ('wait', 30),
        ('fly', 2441, 1065, 127, 135), ('wait', 30),
        ('walk', 100, 140), ('hold', R, 30), ('expect', 100), ('wait', 30),
        ('fight2', 6000), ('wait', 30), ('events',), ('wait', 30),
    ]),
    # 3-1 -> 3-2 -> 3-3: s141 late for class -> s160 (flag 27) -> 3-11 -> 3-10 -> 3-3: s142 the circus clowns
    # (flag 28)
    ('lateclass', [
        ('walk', 500, 10), ('hold', U, 30), ('area', 3, 2), ('wait', 30),
        ('walk', 540, 10), ('hold', U, 30), ('area', 3, 3), ('wait', 30),
        ('walk', 470, 60), ('hold', U, 20), ('expect', 100), ('wait', 30),
        ('walk', 112, 370), ('hold', D, 30), ('area', 3, 10), ('wait', 30),
        ('walk', 150, 210), ('hold', D, 30), ('area', 3, 3), ('wait', 30), ('events',),
        ('walk', 470, 60), ('wait', 30),
    ]),
    # training before the circus fight (s143: five fighters at once): 3-3 -> 3-2 -> 3-1, the world map -> 2-8, the
    # East District 439 fighters (they come back on every load): 2-9, 2-10, 2-11, 2-12, 2-11, 2-10, 2-9 ...; stat
    # points into STR
    ('training', [
        ('walk', 544, 290), ('hold', D, 30), ('area', 3, 2), ('wait', 30),
        ('walk', 520, 210), ('hold', D, 30), ('area', 3, 1), ('wait', 30),
        ('walk', 40, 150), ('talk', 'L'), ('wait', 60),
        ('fly', 2632, 1465, 80, 71), ('wait', 30),
        ('walk', 248, 440), ('hold', D, 30), ('area', 2, 9), ('wait', 30), ('fight2', 3000), ('wait', 30),
        ('walk', 450, 220), ('hold', R, 30), ('area', 2, 10), ('wait', 30), ('fight2', 3000), ('wait', 30),
        ('walk', 200, 6), ('hold', U, 30), ('area', 2, 11), ('wait', 30), ('fight2', 3000), ('wait', 30),
        ('walk', 290, 50), ('hold', R, 30), ('area', 2, 12), ('wait', 30), ('fight2', 3000), ('wait', 30),
        ('walk', 4, 90), ('hold', L, 30), ('area', 2, 11), ('wait', 30), ('fight2', 3000), ('wait', 30),
        ('walk', 210, 450), ('hold', D, 30), ('area', 2, 10), ('wait', 30), ('fight2', 3000), ('wait', 30),
        ('walk', 4, 200), ('hold', L, 30), ('area', 2, 9), ('wait', 30), ('fight2', 3000), ('wait', 30),
        ('alloc', 0),
    ]),
]
