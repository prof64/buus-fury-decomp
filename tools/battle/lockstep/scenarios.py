"""Lockstep scenarios: what the player does, frame by frame. Each scenario is a dict

    name     unique id (also the trace file name)
    form     player form (form table 0x086B82EC, 0..53); the character follows from the form
    input    [[keys, frames], ...]  keys = 'A+RIGHT' style (A B SELECT START RIGHT LEFT UP DOWN R L), '' = none
    hits     [{at, dir, atk, mult, str, pow}]  attack message injected at the start of frame `at` (1-based):
             dir = direction the hit travels (0 down 1 up 2 left 3 right), atk = attack-type index (2 punch, 4 energy)
    skills   the record's skill list (slot 0 selected unless `slot`), skill_level 0..2
    hp, ep   record HP / EP (max stays >= 900), stats [STR, POW, END, SPD]
    pos, dir starting footprint (world px, Hyperbolic Time Chamber 6-9) and facing

Add a scenario: append to SCENARIOS (or extend one of the generators below), run
    .venv/bin/python tools/battle/lockstep/record.py NAME_GLOB
and the simulator test picks the new trace up (site/tests/battle/buu_lockstep.test.ts).
"""
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(os.path.dirname(os.path.dirname(HERE)))
sys.path.insert(0, os.path.join(ROOT, 're', 'buu'))
from common import u8  # noqa: E402

# playable forms of the simulator's fighter list (rulesets/buu/index.ts FIGHTERS)
FORMS = [2, 0, 1, 3, 4, 5, 6, 7, 8, 10, 9, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30,
         31, 32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43, 44, 46, 47, 48, 49, 50, 51, 52, 53]
# skills each character knows (characters.json new_game.skills + the transformations)
CHAR_SKILLS = {0: [7, 20, 15, 14], 1: [7, 20, 6, 17], 2: [7, 20, 16, 8], 3: [7, 20, 4, 11], 4: [7, 20, 10, 2], 5: [7, 20, 13, 23],
               6: [7, 20, 1, 9], 7: [7, 3, 5], 8: [7]}
CHAR_NAMES = ['goku', 'gohan', 'goten', 'trunks', 'vegeta', 'gotenks', 'vegito', 'gogeta', 'other']


def form_char(f):
    return u8(0x086B82EC + 24 * f + 0xC)


def base(f, kind, inp, **kw):
    return dict(name='f%02d_%s' % (f, kind), form=f, input=inp, **kw)


def generic(f):
    """idle / walk / dash / punch / guard / hurt / KO for one form"""
    out = []
    out.append(base(f, 'move', [['', 20], ['RIGHT', 30], ['LEFT', 60], ['RIGHT', 30], ['UP', 30], ['DOWN', 50], ['UP', 20], ['UP+RIGHT', 20],
                                ['DOWN+LEFT', 20], ['', 10], ['RIGHT', 1], ['', 2], ['RIGHT', 20], ['', 12], ['LEFT', 1], ['', 2], ['LEFT', 12],
                                ['LEFT+UP', 10], ['', 12], ['UP', 1], ['', 3], ['UP', 15], ['', 12], ['DOWN', 1], ['', 2], ['DOWN', 10],
                                ['DOWN+RIGHT', 8], ['', 10], ['DOWN', 1], ['', 2], ['DOWN+A', 1], ['', 30]],
                    pos=[280, 170]))  # clear of the HTC's Trunks NPC (marks tiles 24..25 x 11 on the walk grid)
    out.append(base(f, 'punch', [['', 5], ['A', 1], ['', 25], ['A', 1], ['', 8], ['A', 1], ['', 25], ['RIGHT', 10], ['RIGHT+A', 1], ['RIGHT', 25],
                                 ['A', 2], ['', 20], ['LEFT', 1], ['A', 1], ['', 30]]))
    out.append(base(f, 'guard', [['', 5], ['R', 30], ['R+B', 60], ['R', 10], ['', 10], ['B', 1], ['', 60], ['R+B', 20], ['B', 10], ['', 30]]))
    out.append(base(f, 'hurt', [['', 5], ['', 50], ['R', 40], ['', 10], ['RIGHT', 60], ['', 30]],
                    hits=[{'at': 10, 'dir': 2, 'atk': 2}, {'at': 30, 'dir': 0, 'atk': 4}, {'at': 70, 'dir': 0, 'atk': 2},
                          {'at': 80, 'dir': 1, 'atk': 2}, {'at': 120, 'dir': 2, 'atk': 4}]))
    out.append(base(f, 'bump', [['', 5], ['LEFT', 1], ['', 2], ['LEFT', 40], ['', 30], ['DOWN', 1], ['', 2], ['DOWN', 30], ['', 40]],
                    arena='crater', pos=[96, 216]))
    out.append(base(f, 'ko',[['', 5], ['', 300]], hp=1, hits=[{'at': 10, 'dir': 2, 'atk': 2}]))
    return out


def skills(f):
    ci = form_char(f)
    out = []
    for sk in CHAR_SKILLS[ci]:
        s = [sk] + [x for x in CHAR_SKILLS[ci] if x != sk]
        if sk == 20:
            out.append(base(f, 'skill20', [['', 5], ['B', 1], ['', 300], ['B', 1], ['', 120]], skills=s))
            # small maxEP: the SS gauge (filled to maxEP) runs out after 20 * maxEP frames -> automatic revert
            out.append(base(f, 'ssdrain', [['', 5], ['B', 1], ['', 1100]], skills=s, ep=40, maxep=40, ss=40, ss3=40))
            continue
        out.append(base(f, 'skill%d' % sk, [['', 5], ['B', 1], ['', 100], ['B', 130], ['', 120], ['RIGHT', 1], ['B', 60], ['', 120]], skills=s))
    return out


def extra():
    out = []
    # Saiyaman (Gohan learns it in the story): into form 14 and back to 10; from SS Gohan 16 (up 255) it reverts
    for f in (10, 14, 15, 16):
        out.append(base(f, 'skill18', [['', 5], ['B', 1], ['', 80], ['B', 1], ['', 80], ['B', 30], ['', 60]], skills=[18, 7]))
    # Super Saiyan 3 from SS (skill 22), and back
    for f in (4, 3, 34):
        out.append(base(f, 'skill22', [['', 5], ['B', 1], ['', 260], ['B', 1], ['', 120]], skills=[22, 7]))
    # skill levels 1 / 2 for the levelled skills
    for f, sk in ((2, 15), (19, 16), (10, 6), (21, 11), (26, 10), (26, 2), (10, 17), (21, 4), (19, 8), (2, 14)):
        for lv in (1, 2):
            out.append(base(f, 'skill%d_lv%d' % (sk, lv), [['', 5], ['B', 1], ['', 100], ['B', 130], ['', 160]], skills=[sk, 7], skill_level=lv))
    # Energy Blast refire (B again before tick 40) and running out of EP mid-beam
    out.append(base(2, 'skill7_refire', [['', 5], ['B', 1], ['', 20], ['B', 1], ['', 30], ['B', 1], ['', 60]], skills=[7]))
    out.append(base(2, 'skill15_empty', [['', 5], ['B', 200], ['', 60]], skills=[15, 7], skill_level=2, ep=200))
    # low HP: the HUD beeps (sound 41, volume 32) every 61 frames
    out.append(base(2, 'lowhp', [['', 200], ['RIGHT', 30], ['', 50]], hp=100))
    # long idle: the bored animation after 1024 frames
    out.append(base(2, 'idle_long', [['', 1300]]))
    out.append(base(10, 'idle_long', [['', 1300]]))
    return out


def all_scenarios():
    out = []
    for f in FORMS:
        out += generic(f)
        out += skills(f)
    return out + extra()


SCENARIOS = all_scenarios()
