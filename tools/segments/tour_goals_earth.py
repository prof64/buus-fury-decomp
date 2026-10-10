"""field coverage tours on Earth: world 0 (Z-Marts), world 2 (Goku's House, East District 439),
world 3 (Hercule City). States: work/states/tour_earth_*.ss (tools/run2/state_at.py ROW OUT --chain runN)."""
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from tours import area_goals  # noqa: E402


def go(path, w, a):
    """walk the points of path (the last one inside the exit rect), then wait for area (w, a) and its fade-in"""
    return [('walk', x, y) for x, y in path] + [('area', w, a), ('wait', 40)]


# Goku's House (state tour_earth_house.ss: run 3 row 45,918, 2-1 at the start of the game): 2-1's NPCs (s104;
# the wandering s105 is usually missed), upstairs 2-3 (its door: A facing up), Gohan and Goten's Room 2-4 (s108), Chi Chi's Room 2-5 (s109)
HOUSE = ([('wait', 20), ('walk', 92, 98), ('talk', 'U'), ('walk', 64, 138), ('talk', 'U')]
         + go([(124, 200), (124, 226)], 2, 2) + area_goals('2-2', 1)
         + go([(312, 204), (312, 176)], 2, 3)
         + [('walk', 240, 96), ('walk', 112, 96), ('a_at', 104, 94, 'U'), ('area', 2, 4), ('wait', 40)] + [('a_at', 39, 112, 'U')]
         + go([(116, 210), (116, 228)], 2, 3)
         + go([(100, 132), (100, 150)], 2, 5) + [('a_at', 48, 100, 'L'), ('a_at', 48, 100, 'U'), ('wait', 60)])

TOURS = {'earth_house': ('tour_earth_house.ss', HOUSE)}

# Hercule City (state tour_earth_city.ss: run 3 row 74,100, 3-1 after its fight): north to 3-2, the
# Hercule Cafe 3-20 (the counter's s170; its table scripts need flags 216-240, which no run sets), its side rooms
# 3-21 / 3-22, out, east to 3-4, Hercule's Mansion 3-5, 3-6 (s153)
CITY = (go([(500, 40), (500, 2)], 3, 2)
        + go([(436, 80), (436, 60)], 3, 20)
        + [('a_at', 144, 241, 'U')]
        + go([(32, 190), (8, 190)], 3, 21)
        + go([(270, 190), (292, 190)], 3, 20)
        + go([(270, 190), (292, 190)], 3, 22)
        + go([(30, 190), (8, 190)], 3, 20)
        + go([(144, 290), (144, 310)], 3, 2)
        + go([(600, 150), (620, 150)], 3, 4)
        + go([(148, 124), (148, 106)], 3, 5)
        + go([(20, 280), (2, 280)], 3, 6)
        + [('a_at', 25, 96, 'U'), ('wait', 60)])

TOURS['earth_city'] = ('tour_earth_city.ss', CITY)

# Hercule's Mansion past the city tour's 3-2 divergence (state tour_earth_mansion.ss: earth_city's goals driven 1,760
# frames, the player walking in 3-4)
MANSION = (go([(148, 124), (148, 106)], 3, 5)
           + go([(20, 280), (2, 280)], 3, 6)
           + [('a_at', 25, 96, 'U'), ('wait', 60)]
           + go([(200, 124), (222, 124)], 3, 5)
           + go([(152, 290), (152, 310)], 3, 4))
TOURS['earth_mansion'] = ('tour_earth_mansion.ss', MANSION)

# Orange Star High School, late (state tour_earth_school.ss: run 3 row 77,920, 3-11 after the class event): the board
# (s171; the students are gone by now), 3-13 (never entered by a run: s162), back down to 3-10 (save point)
SCHOOL_TALKS = [('a_at', 144, 72, 'U'),
                ('walk', 185, 144), ('talk', 'L'),
                ('walk', 64, 90), ('talk', 'U'),
                ('walk', 88, 90), ('talk', 'U'),
                ('walk', 57, 264), ('talk', 'L'),
                ('walk', 160, 342), ('talk', 'D')]
SCHOOL = (SCHOOL_TALKS
          + go([(108, 56), (108, 34)], 3, 13)
          + [('walk', 232, 274), ('talk', 'U'), ('wait', 30)]
          + go([(232, 370), (232, 390)], 3, 11)
          + go([(108, 360), (108, 390)], 3, 10)
          + area_goals('3-10', 1) + [('wait', 30)])
TOURS['earth_school'] = ('tour_earth_school.ss', SCHOOL)

# Orange Star High School, early (state tour_earth_school_early.ss: run 3 row 59,195, 3-12 after the first class
# event): the classroom's s161 (A facing down), then 3-11's board and students, 3-13
SCHOOL_EARLY = ([('a_at', 104, 177, 'D')]
                + go([(200, 204), (220, 204)], 3, 11)
                + SCHOOL_TALKS
                + go([(108, 56), (108, 34)], 3, 13)
                + [('walk', 232, 274), ('talk', 'U'), ('wait', 30)])
TOURS['earth_school_early'] = ('tour_earth_school_early.ss', SCHOOL_EARLY)

# Musuka Circus west side (state tour_earth_circus.ss: run 3 row 101,300, entering 3-14 the first time): 3-16 and
# 3-18 (no run entered them; enemies only, no A-triggers), north into 3-19 and back
CIRCUS = (go([(60, 30), (60, 2)], 3, 16)
          + go([(60, 30), (60, 2)], 3, 18)
          + go([(60, 30), (60, 2)], 3, 19)
          + go([(60, 120), (60, 150)], 3, 18)
          + go([(60, 280), (60, 310)], 3, 16)
          + go([(60, 280), (60, 310)], 3, 14))
TOURS['earth_circus'] = ('tour_earth_circus.ss', CIRCUS)

# East District 439 south (state tour_earth_2_12.ss: run 3 row 148,300, 2-12 after its fight): none of these areas
# was entered by a run. 2-13, 2-14 (save point), 2-33 (A script: the "Gohan needs level 10" barrier toward 2-34 ..
# 2-47), back to 2-14, west 2-15 .. 2-19, 2-20 (s115)
EAST_A = (go([(170, 126), (170, 150)], 2, 13)
          + go([(200, 116), (222, 116)], 2, 14)
          + area_goals('2-14', 1)
          + go([(200, 370), (222, 370)], 2, 33)
          + [('a_at', 215, 80, 'U')]
          + go([(24, 116), (2, 116)], 2, 14)
          + go([(20, 400), (2, 400)], 2, 15)
          + go([(100, 290), (100, 310)], 2, 16)
          + go([(60, 290), (60, 310)], 2, 17)
          + go([(20, 100), (2, 100)], 2, 18)
          + go([(20, 56), (2, 56)], 2, 19)
          + go([(76, 130), (76, 150)], 2, 20)
          + [('a_at', 102, 280, 'U'), ('wait', 30)])
TOURS['earth_east_a'] = ('tour_earth_2_12.ss', EAST_A)

# East District 439 outside Goku's House (state tour_earth_2_8.ss: run 3 row 139,800, 2-8 after Gohan's return):
# the save point (s011; the NPC s112 is script-created only), then back inside Goku's House 2-2 / 2-1 (their NPCs at this point)
E28 = ([('a_at', 152, 288, 'U'), ('wait', 20)]
       + go([(264, 226), (264, 204)], 2, 2)
       + go([(156, 290), (156, 270)], 2, 1))
TOURS['earth_2_8'] = ('tour_earth_2_8.ss', E28)

# Z-Marts. Hercule City's (0-91, state tour_earth_city.ss): the wandering customer s029. The Thieves Den's (0-93, state
# tour_earth_thieves.ss: run 2 row 977,895, entering 9-11; never entered by a run): the shop counter s031 (A facing
# left; the buy menu opened, the cursor moved down twice, closed with B) and the customer s032
ZMART_CITY = (go([(140, 76), (138, 58)], 0, 91)
              + [('walk', 192, 98), ('talk', 'U'), ('walk', 176, 88), ('talk', 'R'), ('wait', 20)]
              + [('walk', 112, 214), ('area', 3, 1), ('wait', 40)])
TOURS['earth_zmart_city'] = ('tour_earth_city.ss', ZMART_CITY)
ZMART_THIEVES = ([('wait', 60)]
                 + go([(160, 250), (256, 250), (252, 214), (252, 198)], 0, 93)
                 + [('walk', 90, 120), ('hold', 0x20, 3), ('wait', 2), ('hold', 1, 3), ('wait', 120),
                    ('hold', 0x80, 2), ('wait', 40), ('hold', 0x80, 2), ('wait', 40),
                    ('hold', 2, 2), ('wait', 40), ('hold', 2, 2), ('wait', 40), ('hold', 2, 2), ('wait', 40),
                    ('events',),
                    ('walk', 184, 114), ('talk', 'U'), ('wait', 20)]
                 + [('walk', 112, 214), ('area', 9, 11), ('wait', 40)])
TOURS['earth_zmart_thieves'] = ('tour_earth_thieves.ss', ZMART_THIEVES)

# 0-93 past the shop menu (state tour_earth_zmart93.ss: earth_zmart_thieves' goals driven 770 frames, the menu
# closed): the customer s032, out to 9-11
TOURS['earth_zmart93'] = ('tour_earth_zmart93.ss', [('walk', 184, 114), ('talk', 'U'), ('wait', 20),
                                                     ('walk', 112, 214), ('area', 9, 11), ('wait', 40)])

# East District 439 later (state tour_earth_2_8_late.ss: run 1 row 258,060, 2-8 after the 2-9/2-10 training loops,
# hp 249, Goku): the walk south to 2-12 / 2-13 / 2-14 / 2-33 and its barrier ("Goku cannot open": Gohan only)
TO_2_33 = (go([(248, 446)], 2, 9)
           + go([(440, 220), (462, 220)], 2, 10)
           + go([(200, 24), (200, 2)], 2, 11)
           + go([(280, 50), (302, 50)], 2, 12)
           + go([(170, 126), (170, 150)], 2, 13)
           + go([(200, 116), (222, 116)], 2, 14)
           + go([(200, 370), (222, 370)], 2, 33))
EAST_LATE = TO_2_33 + [('a_at', 215, 80, 'U'), ('wait', 30)]
TOURS['earth_east_late'] = ('tour_earth_2_8_late.ss', EAST_LATE)

# 2-20 past earth_east_a's 2-19 exit divergence (state tour_earth_2_20.ss: earth_east_a's goals driven 2,920 frames,
# just inside 2-20): s115 (A, flag 195)
TOURS['earth_2_20'] = ('tour_earth_2_20.ss', [('a_at', 102, 280, 'U'), ('wait', 30),
                                               ('walk', 80, 40), ('walk', 80, 20), ('area', 2, 19), ('wait', 40)])
