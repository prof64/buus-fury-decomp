"""The Earth chapters' route (Gohan: school, Great Saiyaman, Videl, Goten & Trunks, the World Tournament) as
earth_driver.py goals, played by the real game from the chapter start state (work/earth/chapter_start.ss: the story
playthrough route (route.py, New Game -> Pikkon -> Earth 2-1) played from the soft reset; see earth_play.py).

Each segment starts where the previous ended; earth_play.py saves the emulator state at every segment's start
(work/earth/seg_<name>.ss) and the key log."""
from route import KEY

U, D, L, R, A, B = KEY.UP, KEY.DOWN, KEY.LEFT, KEY.RIGHT, KEY.A, KEY.B

# fight in 2-9 (two criminals: melee only, Energy Blasts from range finish them), up to 2-8 and back (the fighters
# come back on every load)
CYCLE_2_9 = (
    ('fight2', 3000), ('wait', 30),
    ('walk', 248, 8), ('hold', U, 30), ('area', 2, 8), ('wait', 30),
    ('walk', 248, 440), ('hold', D, 30), ('area', 2, 9), ('wait', 30),
)
# as CYCLE_2_9, fighting on at low HP (the criminals only punch: Energy Blasts from range)
CYCLE_2_9_STAY = (('fight2', 3000, 'stay'),) + CYCLE_2_9[1:]
# the circus fighters (flag 25): fight in 3-15, down to 3-14 and back up
CYCLE_3_15 = (
    ('fight2', 3000), ('wait', 30),
    ('walk', 160, 290), ('hold', D, 30), ('area', 3, 14), ('wait', 30),
    ('walk', 400, 8), ('hold', U, 30), ('area', 3, 15), ('wait', 30),
)
# Inventory: START, R (Inventory), A on the first supply, A = use, B, START
USE_FIRST_SUPPLY = 'S:3,0:60,RS:5,0:40,A:6,0:40,A:6,0:40,B:6,0:30,S:6,0:60'

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
        ('walk', 248, 440), ('hold', D, 30), ('area', 2, 9), ('wait', 30),
        ('levelup', 4, CYCLE_2_9),
        ('wait', 120), ('alloc', 0),
    ]),
    # the circus fight: 2-9 -> 2-8, the world map circle -> Hercule City 3-1 -> 3-2 -> 3-3: s143 (five fighters,
    # their defeat events s144-s148, flags 29-33) -> s149 (flag 34) -> 3-2
    ('circus', [
        ('walk', 248, 8), ('hold', U, 30), ('area', 2, 8), ('wait', 30),
        ('walk', 176, 380), ('talk', 'U'), ('wait', 60),
        ('fly', 2441, 1065, 127, 135), ('wait', 30),
        ('walk', 500, 10), ('hold', U, 30), ('area', 3, 2), ('wait', 30),
        ('walk', 540, 10), ('hold', U, 30), ('area', 3, 3), ('wait', 30),
        ('walk', 300, 180), ('hold', L, 20), ('expect', 100), ('wait', 20),
        ('fight2', 12000), ('wait', 30), ('until', 3, 2), ('wait', 30),
        # low on HP: the Rice Ball (+40 HP), then the circus fighters until a level-up (refills HP / EP)
        ('wait', 60), ('menu', USE_FIRST_SUPPLY),
        ('walk', 4, 140), ('hold', L, 30), ('area', 3, 14), ('wait', 30),
        ('walk', 400, 8), ('hold', U, 30), ('area', 3, 15), ('wait', 30),
        ('levelup', 1, CYCLE_3_15),
        ('walk', 160, 290), ('hold', D, 30), ('area', 3, 14), ('wait', 30),
        ('walk', 400, 48), ('walk', 450, 140), ('hold', R, 30), ('area', 3, 2), ('wait', 30),
    ]),
    # the Musuka Circus: 3-2 -> 3-14 -> 3-15 (fighters) -> 3-17 (fighters) -> 3-19 (three thugs): Musuka (s169,
    # flag 36)
    ('musuka', [
        ('walk', 4, 140), ('hold', L, 30), ('area', 3, 14), ('wait', 30),
        ('walk', 400, 8), ('hold', U, 30), ('area', 3, 15), ('wait', 30), ('fight2', 4000), ('wait', 30),
        ('walk', 160, 8), ('hold', U, 30), ('area', 3, 17), ('wait', 30), ('fight2', 4000), ('wait', 30),
        ('walk', 160, 8), ('hold', U, 30), ('area', 3, 19), ('wait', 30), ('fight2', 6000), ('wait', 30),
        ('talkto', 224, 48, 'D'), ('wait', 30),
    ]),
    # back through the circus: 3-19 -> 3-17 -> 3-15 -> 3-14: s168 "Stop that man!" (flag 37) and the tank
    ('tank', [
        ('walk', 400, 128), ('hold', D, 30), ('area', 3, 17), ('wait', 30), ('fight2', 3000, 'melee'), ('wait', 30),
        ('walk', 160, 290), ('hold', D, 30), ('area', 3, 15), ('wait', 30), ('fight2', 3000, 'melee'), ('wait', 30),
        ('wait', 120), ('alloc', 1),
        ('walk', 160, 290), ('hold', D, 30), ('area', 3, 14), ('wait', 30),
        ('walk', 400, 48), ('walk', 300, 140), ('events',), ('wait', 20), ('fight2', 12000, 'snipe'), ('wait', 30),
        ('events',), ('wait', 30),
    ]),
    # 3-14 -> 3-2 -> 3-1: s132 "Well, well! Look who it is!" (Videl, s133: flag 38); the world map circle (s130 ->
    # s134: the World Tournament, flag 39)
    ('videl', [
        ('walk', 450, 140), ('hold', R, 30), ('area', 3, 2), ('wait', 30),
        ('walk', 520, 210), ('hold', D, 30), ('area', 3, 1), ('wait', 30),
        ('walk', 230, 150), ('walk', 205, 150), ('events',), ('wait', 30),
        ('walk', 40, 150), ('talk', 'L'), ('wait', 60),
    ]),
    # the world map -> 2-8 -> 2-2: s106 (Chi-Chi: Goku comes back for the tournament) -> s057 ... s113 (Goten
    # trains with Gohan, flag 43) -> 2-8
    ('chichi', [
        ('fly', 2632, 1465, 80, 71), ('wait', 30),
        ('walk', 264, 220), ('hold', U, 30), ('area', 2, 2), ('events',), ('until', 2, 8), ('wait', 30),
    ]),
    # a Rice Ball (Inventory: START, R, A, A = use), then through East District 439: 2-8 -> 2-9 -> 2-10 -> 2-11 ->
    # 2-90 (Goten's rock drill, s119)
    ('rocks', [
        ('walk', 248, 440), ('hold', D, 30), ('area', 2, 9), ('wait', 30),
        # low on HP after the circus: the 2-9 criminals until a level-up refills it
        ('levelup', 1, CYCLE_2_9_STAY),
        ('fight2', 3000), ('wait', 30),
        ('walk', 450, 220), ('hold', R, 30), ('area', 2, 10), ('wait', 30), ('fight2', 3000), ('wait', 30),
        ('walk', 200, 6), ('hold', U, 30), ('area', 2, 11), ('wait', 30), ('fight2', 3000), ('wait', 30),
        ('walk', 270, 6), ('hold', U, 30), ('area', 2, 90), ('wait', 30),
    ]),
    # the gate to Goten's clearing wants level 65: train in 2-11 / 2-12 / 2-10, back to 2-90, A at the gate, Goten
    ('gate', [
        ('walk', 96, 290), ('hold', D, 30), ('area', 2, 11), ('wait', 30), ('fight2', 3000), ('wait', 30),
        ('walk', 290, 50), ('hold', R, 30), ('area', 2, 12), ('wait', 30), ('fight2', 3000), ('wait', 30),
        ('walk', 4, 90), ('hold', L, 30), ('area', 2, 11), ('wait', 30), ('fight2', 3000), ('wait', 30),
        ('walk', 210, 450), ('hold', D, 30), ('area', 2, 10), ('wait', 30), ('fight2', 3000), ('wait', 30),
        ('walk', 200, 6), ('hold', U, 30), ('area', 2, 11), ('wait', 30), ('fight2', 3000), ('wait', 30),
        ('wait', 120), ('alloc', 1),
        ('walk', 270, 6), ('hold', U, 30), ('area', 2, 90), ('wait', 30),
        ('walk', 96, 226), ('talk', 'U'), ('wait', 30),
    ]),
    # through the gate: Goten's rock drill (s119): he throws 10 rocks (object 0x0804CE50, lands where the player
    # stood; a hit = s120 and a new round), dodge them -> s121 (Goten goes Super Saiyan) -> the fight (enemy type 37,
    # on-defeat s122) -> the gravity chamber 4-25 (Trunks vs Vegeta)
    ('goten', [
        ('hold', U, 40), ('talkto', 96, 176, 'D'), ('wait', 20),
        ('dodge', 120, 260, 100, 281), ('wait', 30),
        ('fight2', 8000), ('wait', 30), ('events',), ('until', 4, 25), ('wait', 30),
    ]),
    # Trunks vs Vegeta (type 136: keeps 60 px away, jumps back to the middle): dash at him and punch -> s192
    # (flag 44) -> West City 4-1
    ('vegeta', [
        ('herd', 8000), ('until', 4, 1), ('wait', 30),
    ]),
    # 4-1 -> Capsule Corp 4-20: Bulma (s184 "Are you ready to go to the World Tournament?" Yes: flag 45)
    ('totournament', [
        ('walk', 850, 360), ('hold', R, 30), ('area', 4, 20), ('wait', 30),
        ('talkto', 232, 248, 'D'), ('wait', 30),
    ]),
    # Papaya Island 7-1 -> 7-2: register Trunks (s239, A at the desk: flag 47); the door swaps to Gohan (inline
    # 0x0807A142: only registered fighters), register Gohan (flag 46), through the door -> 7-3: s262 (flag 48)
    ('register', [
        ('walk', 220, 6), ('hold', U, 30), ('area', 7, 2), ('wait', 30),
        ('walk', 310, 144), ('talk', 'U'), ('wait', 30),
        ('walk', 228, 96), ('hold', U, 60), ('events',), ('hold', U, 60), ('events',), ('wait', 30),
        ('walk', 310, 144), ('talk', 'U'), ('wait', 30),
        ('walk', 228, 96), ('hold', U, 60), ('events',), ('hold', U, 60), ('until', 7, 3), ('events',), ('wait', 30),
    ]),
    # 7-3: the announcer (s266 "qualification preliminaries?" Yes): Hercule, the punching machine (native 0xA7
    # run_punch_minigame, A mashed; < 100 points = "try again?" Yes) -> 7-5 -> the Junior Division in the ring 7-6
    # (Trunks: the fighters come at him, A mashed) -> s302 (flag 49) -> 7-5 (Gohan)
    ('prelims', [
        ('talkto', 264, 88, 'D'), ('until', 7, 5), ('wait', 30),
    ]),
    # 7-5: the announcer at the ring door (moved to (106, 147) by then): s288 "Is everyone ready?" Yes -> s303 /
    # s304 the main tournament (scripted matches; Videl vs Spopovich played as Videl, A mashed), Spopovich and Yamu
    # take Gohan's energy, s306 -> s260 "Hey, Gohan! Where are you going?" (flag 50) -> Goku in 7-2 (the Babidi
    # chapter starts here)
    ('tournament', [
        ('walk', 106, 136), ('talk', 'D'), ('fight2', 20000, 'melee'), ('until', 7, 2), ('wait', 30),
    ]),
]
