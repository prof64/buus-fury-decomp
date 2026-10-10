"""goal lists of the field coverage tours (tours.py record STATE NAME tour_goals:NAME is not used: tours.py takes a
literal or area:W-A; these are imported by run_tours.py)"""
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from tours import area_goals  # noqa: E402

B = 2

# Capsule Corp (state buu_cc_lobby.ss: run 2 row 74,990, the Lobby 4-21): the Lobby's trigger and NPCs, Dr. Brief's
# Lab (4-31), the Computer Room (4-22: the computer's database, the NPC), back to the Lobby, the Exhibit Halls
# (4-23, 4-26: the exhibits' A-up scripts)
CAPSULE = (area_goals('4-21', 1)
           + [('walk', 40, 104), ('walk', 8, 104), ('area', 4, 31), ('wait', 40)] + area_goals('4-31', 1)
           + [('walk', 30, 340), ('walk', 20, 340), ('area', 4, 22), ('wait', 40)]
           + [('talk_npc', 0), ('a_at', 108, 116, 'U'), ('wait', 80), ('hold', B, 2), ('wait', 120)]
           + [('walk', 200, 104), ('walk', 224, 104), ('area', 4, 31), ('wait', 40),
              ('walk', 420, 104), ('walk', 448, 104), ('area', 4, 21), ('wait', 40),
              ('walk', 200, 104), ('walk', 224, 104), ('area', 4, 23), ('wait', 40)]
           + area_goals('4-23', 1)
           + [('walk', 200, 104), ('walk', 224, 104), ('area', 4, 26), ('wait', 40)]
           + area_goals('4-26', 1))

TOURS = {'capsule': ('buu_cc_lobby.ss', CAPSULE)}
