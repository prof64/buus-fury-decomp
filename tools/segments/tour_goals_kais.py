"""field coverage tours of worlds 15 (Nataday Village, Northern Wilderness, Volcano), 18 (HFIL), 19 (Planet of the
Kais), 20/21 (Barren Wasteland, Buu's body) and 22 (Heaven). States work/states/tour_kais_*.ss:
  run 2 rows (tools/run2/state_at.py ROW OUT --chain run2):
    stomach 1,595,130 (21-1)   intest 1,848,545 (21-25)   kai7 1,358,300 (19-7)   north3 993,600 (15-3)
    hfil5 1,229,800 (18-5)     hfil9 1,236,200 (18-9)     hfil10e 1,245,770 (18-10, walking into 18-11)
    summit 1,068,090 (15-32)   volc19 1,052,760 (15-19)   volc20 1,053,100 (15-20)   volc25 1,057,300 (15-25)
    volc25rock / volc25exit: kais_volc25 driven 266 / 505 frames (work/shipfix/kais_tourcut.py)
  post-game (work/shipfix/kais_pg.sh: tools/side/hfil.py driven from the real post-game frame work/side/pg_start.ss):
    natland (0-50: the flight to Nataday cut at its descent, work/shipfix/kais_cut.py)
    waste (20-1: the marker under Kami's Lookout at altitude 0)   lookout (6-14 Kami's Lookout, by its pad)
    planet (19-1: 6-14's pad, the Other World map, two LEFT)
Heaven 22-1 is reached only through the Other World travel menu (event 511: the pads of 18-1 HFIL, 1-5 Snake Way,
6-14 Kami's Lookout (flag 149), 19-1, 22-1); Heaven is always on its list, but runs 1-3 never picked it."""
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from tours import area_goals  # noqa: E402
from tour_goals_kami import trig_goals, npc_goals, ex, last  # noqa: E402,F401  (also installs 'talk_near')

B = 2

# Stomach 21-1: both A warp spots (walk_to scripts) and the save point
STOMACH = area_goals('21-1', 1)

# Intestines 21-25: the save point
INTEST = area_goals('21-25', 1)

# Planet of the Kais 19-7: the two NPCs (s465), from above
KAI7 = [('wait', 20), ('a_at', 136, 422, 'D'), ('a_at', 168, 422, 'D')]

# Planet of the Kais 19-1 -> 19-2 -> 19-4 -> 19-5 (the A-up rock: the way to the Z-Mart 0-94)
PLANET = ([('wait', 30)]
          + ex([(424, 20), (424, 2)], 19, 2)
          + ex([(292, 370), (292, 390)], 19, 4)
          + ex([(284, 256), (300, 256)], 19, 5)
          + trig_goals('19-5')
          + ex([(160, 70), (160, 50)], 0, 94))

# Kami's Lookout 6-14 (state lookout: hfil.py fly + goto 6-14) -> its pad (A, flag 149) -> the Other World map (one
# LEFT = Heaven) -> Heaven 22-1 (the travel screen and the map load included): the four NPCs, the save point, the pad
L_ = 32
TO_HEAVEN = ([('a_at', 144, 188, 'U'), ('wait', 40), ('hold', L_, 3), ('wait', 40), ('hold', 1, 3), ('area', 22, 1),
              ('wait', 40)] + npc_goals('22-1') + [('a_at', 35, 49, 'U'), ('a_at', 104, 112, 'U'), ('wait', 60)])

# Nataday Village 15-1 (post-game; state natland: the world-map flight of kais_pg.sh cut at the descent, A held to
# land): the level gates (tier 1 L100 then tier 2 L140, same rect, A facing up; the
# tier-3 one wants flag 408 clear), the save point, the five NPCs; the house 15-33 (its NPC s438)
NATADAY = ([('hold', 1, 79), ('wait', 120), ('a_at', 264, 128, 'U'), ('wait', 30), ('a_at', 264, 128, 'U'), ('wait', 30),
            ('a_at', 459, 591, 'U')]
           + npc_goals('15-1')
           + ex([(157, 226), (157, 204)], 15, 33)
           + npc_goals('15-33'))

# Northern Wilderness 15-3 (run 2 row 993,600, story: post-game the way north of Nataday is shut by a character
# gate, "Goku cannot open this gate!") -> 15-4 -> 15-5 -> 15-6 (never entered; its way on to 15-9 was not found: stuck, then killed)
NORTH = ([('wait', 10)]
         + ex([(316, 30), (316, 2)], 15, 4)
         + ex([(232, 30), (232, 2)], 15, 5)
         + ex([(52, 30), (52, 2)], 15, 6) + [('wait', 60)])

# Barren Wasteland 20-1 (post-game): the save point, the take-off pad (A -> the world map) last
WASTE = [('wait', 30), ('a_at', 227, 217, 'U'), ('a_at', 144, 220, 'U'), ('wait', 120)]

# Barren Wasteland 20-1 -> 20-2 -> 20-3 -> 20-4: the post-game tier gate (s517 via inline 0x0808D1E2, L175; flag 185
# is set, 186 clear), A facing up
WASTE4 = ([('wait', 30)]
          + ex([(56, 30), (56, 2)], 20, 2)
          + ex([(136, 30), (136, 2)], 20, 3)
          + ex([(600, 136), (622, 136)], 20, 4)
          + [('a_at', 368, 80, 'U'), ('wait', 60)])

# Volcano Summit 15-32 (run 2 row 1,068,090: right after the summit boss, flag 111): the Z-Fighter Exhibit 10 (item
# 77, walked onto), down to 15-19, left to 15-31: its save point
SUMMIT = ([('wait', 10), ('walk', 216, 236), ('wait', 60)]
          + ex([(224, 366), (224, 390)], 15, 19)
          + ex([(97, 121), (26, 121)], 15, 31)
          + [('a_at', 140, 160, 'U')])

# Volcano 15-20 (run 2 row 1,053,100) -> 15-21 (never entered: the left exit) -> its bottom exit -> the 15-19 ledge
# of the Evil Talisman chest (s432, flag 210)
VOLC21 = ([('wait', 10)]
          + ex([(12, 108), (2, 108)], 15, 21)
          + ex([(100, 204), (100, 222)], 15, 19)
          + [('a_at', 72, 443, 'U'), ('wait', 30)])

# Volcano 15-25 (run 2 row 1,057,300): the boulder (breakable, flag 258) by the cave mouth, then the top-right exit
# -> 15-26 (never entered) -> its bottom exit -> the 15-19 ledge of the Hercule Exhibit chest (s434, flag 212)
VOLC25 = ([('wait', 10), ('walk', 262, 72), ('hold', 64, 4)] + [('hold', 1, 2), ('wait', 12)] * 5
          + [('wait', 40)]
          + [('walk', 290, 24), ('hold', 64, 30), ('area', 15, 26), ('wait', 40)]
          + ex([(104, 204), (104, 222)], 15, 19)
          + [('a_at', 504, 70, 'U'), ('wait', 30)])

# HFIL 18-5 (run 2 row 1,229,800): the Talisman of Light chest (s440, flag 213)
HFIL5 = [('wait', 10), ('walk', 272, 199), ('walk', 272, 88), ('walk', 239, 93), ('a_at', 192, 48, 'U'), ('wait', 30)]

# HFIL 18-9 (run 2 row 1,236,200): the A spot at (544, 240) (event 12: the save menu) and the Spirit Geta chest (s441)
HFIL9 = [('wait', 10), ('a_at', 544, 248, 'U'), ('a_at', 520, 71, 'U'), ('wait', 30)]

# HFIL 18-11 (run 2 row 1,245,840): the save point, the three wandering Vegetas (s443..s445; not the real one, s446:
# he joins), then back into 18-10 (its chest s442 and the A switch inline 0x0808C5E0 are inside the gate maze: the
# real route toggles its switches by Instant Transmission, tools/side/hfil.py 'maze10'; not walked here)
# (the save point last: in the run-2 states its menu diverges in RNG - see the report)
HFIL11 = ([('wait', 30)] + npc_goals('18-11')[:3] + [('a_at', 136, 84, 'U')]
          + ex([(10, 70), (0, 70)], 18, 10))

# Volcano 15-19 (run 2 row 1,052,760, in from 15-18): the save point. (The chest s433 at (401, 393) is on a ledge
# the walk grid does not reach from here - the volcano's levels; the ledge's door pairs with 15-22's (272, 296).)
VOLC19 = [('wait', 10), ('a_at', 532, 633, 'U')]

# Planet of the Kais 19-1 (post-game, state planet): the travel pad (inline 0x0808CC14: event 511 from the Kais'
# planet, flag 149), the travel screen, A on its default row
PAD191 = [('wait', 10), ('a_at', 192, 244, 'U'), ('wait', 60), ('hold', 1, 3), ('wait', 100), ('hold', 1, 3),
          ('wait', 200)]

TOURS = {
    'kais_volc19': ('tour_kais_volc19.ss', VOLC19),
    'kais_pad191': ('tour_kais_planet.ss', PAD191),
    'kais_hfil5': ('tour_kais_hfil5.ss', HFIL5),
    'kais_hfil9': ('tour_kais_hfil9.ss', HFIL9),
    # (state hfil10e: run 2 row 1,245,770, walking right into 18-11: the area load in the segment - a state taken
    # inside 18-11 imports the wandering NPCs' animation wrongly)
    'kais_hfil11': ('tour_kais_hfil10e.ss', [('hold', 16, 70)] + HFIL11),
    'kais_summit': ('tour_kais_summit.ss', SUMMIT),
    'kais_volc21': ('tour_kais_volc20.ss', VOLC21),
    'kais_volc25': ('tour_kais_volc25.ss', VOLC25),
    # (the same from the boulder: kais_volc25's frame 266, work/shipfix/kais_tourcut.py)
    'kais_volc25b': ('tour_kais_volc25rock.ss', VOLC25[2:]),
    # (from the 15-26 entry, kais_volc25's frame 505: 15-26 and the chest)
    'kais_volc26': ('tour_kais_volc25exit.ss', VOLC25[17:]),
    'kais_lookheaven': ('tour_kais_lookout.ss', TO_HEAVEN),
    'kais_nataday': ('tour_kais_natland.ss', NATADAY),
    'kais_north': ('tour_kais_north3.ss', NORTH),
    'kais_waste': ('tour_kais_waste.ss', WASTE),
    'kais_waste4': ('tour_kais_waste.ss', WASTE4),
    'kais_stomach': ('tour_kais_stomach.ss', STOMACH),
    'kais_intest': ('tour_kais_intest.ss', INTEST),
    'kais_kai7': ('tour_kais_kai7.ss', KAI7),
    'kais_planet': ('tour_kais_planet.ss', PLANET),
}
