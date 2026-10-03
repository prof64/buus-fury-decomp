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


# enemy scenarios: a passive Majin Fighter (aggro off, HP raised) in the Southwest Forest's open band, the player at
# VS_POS facing right; `at` = the enemy footprint's offset from the player's
VS_POS = [100, 192]
VS_HP = 30000


def vs(f, kind, inp, at=(24, 0), **kw):
    en = dict(type=kw.pop('etype', 0x44), at=list(at))
    hp = kw.pop('ehp', VS_HP)
    if hp is not None:
        en['hp'] = hp
    if kw.pop('real', False):
        en.update(aggro=True, dummy=False)
    if 'state' in kw:
        en['state'] = kw.pop('state')
    if 'partner' in kw:
        en['partner'] = kw.pop('partner')
    return base(f, 'vs_' + kind, inp, arena='forest', pos=kw.pop('pos', VS_POS), dir=kw.pop('dir', 3), enemy=en, **kw)


def reactions():
    """the player's reaction to hits: every attack type injected (sub 0..18, 30 frames apart), a 0-multiplier hit, hits
    while blocking / shielding, and real enemies attacking (their own AI, not patched)"""
    out = []
    hits = [{'at': 10 + 30 * i, 'dir': 2, 'atk': i} for i in range(19)] + [{'at': 600, 'dir': 2, 'atk': 2, 'mult': 0}]
    out.append(base(2, 'hurt_kinds', [['', 640]], hits=hits))
    out.append(base(2, 'hurt_block', [['', 5], ['R', 60], ['R+B', 60], ['', 40]],
                    hits=[{'at': 20, 'dir': 2, 'atk': 2}, {'at': 30, 'dir': 0, 'atk': 4}, {'at': 100, 'dir': 2, 'atk': 4}, {'at': 140, 'dir': 2, 'atk': 2}]))
    # real enemies attacking the player (enemy at the player + at; the player faces it): melee, bullet, bomb, beam,
    # flames, area, a spawned hand
    for kind, t, at in (('melee', 0x44, (24, 0)), ('gun', 0x2C, (60, 0)), ('bomb', 8, (60, 0)), ('beam', 0x3C, (70, 0)),
                        ('flames', 1, (40, 0)), ('mini', 0x38, (30, 0)), ('hand', 0x33, (60, -40))):
        out.append(vs(2, 'enemy_' + kind, [['', 240]], at=at, etype=t, real=True, ehp=None))
    return out


# real enemies attacking an idle player, one per archetype and the main bosses: (type, offset of the enemy footprint)
ENEMY_ARCHETYPES = [
    (0x02, (48, 0)), (0x03, (48, 0)), (0x05, (90, 0)), (0x07, (48, 0)), (0x09, (40, 0)), (0x0C, (40, 0)), (0x0F, (40, 0)),
    (0x10, (48, 0)), (0x11, (60, 0)), (0x13, (60, 0)), (0x14, (60, 0)), (0x18, (40, 0)), (0x20, (48, 0)), (0x21, (40, 0)),
    (0x24, (40, 0)), (0x29, (60, 0)), (0x2B, (60, 0)), (0x2E, (60, 0)), (0x2F, (40, 0)), (0x30, (60, 0)), (0x3A, (70, 0)),
    (0x3E, (48, 0)), (0x40, (70, 0)), (0x41, (48, 0)), (0x45, (40, 0)), (0x48, (60, 0)), (0x4B, (70, 0)), (0x4C, (60, 0)),
    (0x4D, (80, 0)), (0x4E, (60, 0)), (0x52, (48, 0)), (0x58, (48, 0)), (0x59, (48, 0)), (0x5D, (48, 0)), (0x5E, (60, 0)),
    (0x61, (40, 0)), (0x62, (40, 0)), (0x63, (40, 0)), (0x65, (48, 0)), (0x67, (40, 0)), (0x68, (48, 0)), (0x6A, (48, 0)),
    (0x6B, (40, 0)), (0x6C, (80, 0)), (0x7F, (80, 0)), (0x83, (60, 0)), (0x85, (30, 0)), (0x8A, (48, 0)), (0x8C, (80, 0)),
    (0x0E, (48, 0)),
]
BOSSES = [
    (0x00, (80, 0)), (0x06, (80, 0)), (0x0A, (60, 0)), (0x12, (60, 0)), (0x1A, (80, 0)), (0x1C, (80, 0)), (0x23, (60, 0)),
    (0x28, (60, 0)), (0x3F, (70, 0)), (0x46, (60, 0)), (0x50, (60, 0)), (0x5A, (60, 0)), (0x5B, (60, 0)), (0x60, (48, 0)),
    (0x64, (70, 0)), (0x73, (60, 0)), (0x79, (60, 0)), (0x7A, (70, 0)), (0x81, (70, 0)), (0x8E, (80, 0)), (0x91, (60, 0)),
]


MASH = [['LEFT', 1], ['', 1], ['RIGHT', 1], ['', 1], ['A', 1], ['', 1], ['B', 1], ['', 1]]
MASH_DPAD = [['LEFT', 1], ['', 1], ['RIGHT', 1], ['', 1]]


def holds():
    """the player's special holds by real enemies started in their grab / effect state at frame 1"""
    out = []
    # Bio Mech grapple (state 0x21, 90 px): the arm reels the player in
    out.append(vs(2, 'hold_grapple', [['', 300]], at=(90, 0), etype=0x05, real=True, ehp=None, state=0x21))
    # Majin Buu's candy beam (state 0xBB, 80 px): candy, hops with the d-pad, the eat
    # Spopovich's bear hug with Yamu draining (types 0x6F + 0x93): mash the d-pad to escape
    out.append(vs(2, 'hold_hug', [['', 120]] + MASH_DPAD * 60 + [['', 200]], at=(14, 0), etype=0x6F, real=True, ehp=None, state=0x22E,
                  partner={'type': 0x93, 'at': [60, 24]}))
    # Dabura's stone spit (state 0x7C), the Buu fragment's freeze (0x0E), mummy wraps (0x4D at 80 px), the Enzyme latch
    # (0x18 at -8, +5), the vampire grab (0x85 at 30 px); then mash to escape
    out.append(vs(2, 'hold_stone', [['', 120]] + MASH * 30 + [['', 200]], at=(80, 0), etype=0x12, real=True, ehp=None, state=0x7C))
    out.append(vs(2, 'hold_freeze', [['', 150]] + MASH * 20 + [['', 200]], at=(48, 0), etype=0x0E, real=True, ehp=None))
    out.append(vs(2, 'hold_wraps', [['', 200]] + MASH_DPAD * 30 + [['', 200]], at=(80, 0), etype=0x4D, real=True, ehp=None))
    out.append(vs(2, 'hold_latch', [['', 200]] + MASH * 20 + [['', 200]], at=(7, -12), etype=0x18, real=True, ehp=None))
    out.append(vs(2, 'hold_vampire', [['', 150]] + MASH * 20 + [['', 200]], at=(30, 0), etype=0x85, real=True, ehp=None))
    # held keys do not count for the vampire's grab (input manager +4 = the press edge): A held 200 frames keeps it,
    # B held likewise, then separate presses (2 frames apart) free the player
    out.append(vs(2, 'hold_vampire_held', [['', 150], ['A', 200], ['', 12], ['B', 100], ['', 12]] + [['A', 1], ['', 1], ['B', 1], ['', 1]] * 10 + [['', 200]], at=(30, 0), etype=0x85, real=True, ehp=None))
    # the other escapes with keys held instead of pressed (they read the press ring 0x08040284: held keys are no
    # presses): 200 frames of a held d-pad / A, then the mash
    HELD = [['LEFT', 100], ['', 4], ['A', 100], ['', 4]]
    out.append(vs(2, 'hold_hug_held', [['', 120]] + HELD + MASH_DPAD * 30 + [['', 200]], at=(14, 0), etype=0x6F, real=True, ehp=None, state=0x22E,
                  partner={'type': 0x93, 'at': [60, 24]}))
    # (kept short: Dabura's fight after the stone diverges at frame 532 of the long form - enemy / RNG, not the hold)
    out.append(vs(2, 'hold_stone_held', [['', 120]] + HELD + MASH * 10 + [['', 60]], at=(80, 0), etype=0x12, real=True, ehp=None, state=0x7C))
    out.append(vs(2, 'hold_freeze_held', [['', 150]] + HELD + MASH * 20 + [['', 200]], at=(48, 0), etype=0x0E, real=True, ehp=None))
    out.append(vs(2, 'hold_wraps_held', [['', 200]] + HELD + MASH_DPAD * 30 + [['', 200]], at=(80, 0), etype=0x4D, real=True, ehp=None))
    out.append(vs(2, 'hold_latch_held', [['', 200]] + HELD + MASH * 20 + [['', 200]], at=(7, -12), etype=0x18, real=True, ehp=None))
    out.append(vs(2, 'hold_candy', [['', 60], ['UP', 40], ['', 20], ['DOWN', 40], ['', 440]], at=(80, 0), etype=0x1A, real=True, ehp=None, state=0xBB))
    return out


def enemy_attacks():
    out = []
    for t, at in ENEMY_ARCHETYPES + BOSSES:
        out.append(vs(2, 'enemy_t%02x' % t, [['', 600]], at=at, etype=t, real=True, ehp=None))
    return out


# enemy distance per skill (footprint offset along the facing): melee-range skills close, the rest at 64 px
VS_DIST = {8: 18, 9: 20, 5: 40}
# one form per character for the enemy scenarios (base forms; Final Flash / Big Bang as Vegeta, the sword as Vegito)
VS_FORMS = [2, 10, 19, 21, 26, 31, 39, 41]
# levelled skills: also at Lv2 / Lv3
VS_LEVELS = {8: (1, 2), 15: (2,), 6: (2,), 16: (2,), 17: (1, 2), 2: (2,), 10: (2,), 11: (2,), 4: (2,)}
VS_SKILL_INPUT = [['', 5], ['B', 1], ['', 100], ['B', 130], ['', 150]]


def versus(f):
    ci = form_char(f)
    # punches into the enemy (adjacent: footprint centres 16 px apart, the aggro test r2 = 0 stays false), a combo, and
    # never walking through it (centres equal -> the patched aggro test passes)
    out = [vs(f, 'punch', [['', 5], ['A', 1], ['', 25], ['A', 1], ['', 6], ['A', 1], ['', 6], ['A', 1], ['', 40], ['LEFT', 6], ['RIGHT', 1],
                           ['A', 1], ['', 30], ['A', 20], ['', 60]], at=(16, 0))]
    for sk in CHAR_SKILLS.get(ci, [7]):
        if sk in (20, 22, 18):
            continue
        s = [sk] + [x for x in CHAR_SKILLS[ci] if x != sk]
        at = (VS_DIST.get(sk, 64), 0)
        out.append(vs(f, 'skill%d' % sk, VS_SKILL_INPUT, at=at, skills=s))
        for lv in VS_LEVELS.get(sk, ()):
            out.append(vs(f, 'skill%d_lv%d' % (sk, lv + 1), VS_SKILL_INPUT, at=at, skills=s, skill_level=lv))
    return out


def versus_extra():
    out = []
    # Dragon Fist steered (head up then down) and released early; Energy Sword steered toward the enemy
    out.append(vs(41, 'skill5_steer', [['', 5], ['B', 30], ['B+UP', 20], ['B+DOWN', 30], ['', 60]], at=(40, 0), skills=[5, 7]))
    out.append(vs(39, 'skill9_walk', [['', 5], ['B', 90], ['', 10], ['RIGHT', 40], ['', 120]], at=(40, 0), skills=[9, 7]))
    # Instant Transmission onto the enemy, Lv1 / Lv3
    out.append(vs(2, 'skill14_lv3', [['', 5], ['B', 1], ['', 60], ['A', 1], ['', 200]], at=(64, 0), skills=[14, 7], skill_level=2))
    # Big Bang: short charge (fizzle) and a full charge into the enemy at 120 px
    out.append(vs(26, 'skill2_tap', [['', 5], ['B', 8], ['', 80]], at=(64, 0), skills=[2, 7]))
    out.append(vs(26, 'skill2_far', [['', 5], ['B', 200], ['', 260]], at=(120, 0), skills=[2, 7], skill_level=2))
    # Masenko Lv3 aimed with the d-pad
    out.append(vs(10, 'skill17_aim', [['', 5], ['B', 20], ['B+RIGHT', 30], ['', 80]], at=(80, 0), skills=[17, 7], skill_level=2))
    return out


def all_scenarios():
    out = []
    for f in FORMS:
        out += generic(f)
        out += skills(f)
    for f in VS_FORMS:
        out += versus(f)
    out += versus_extra()
    out += reactions()
    out += enemy_attacks()
    out += holds()
    return out + extra()


SCENARIOS = all_scenarios()
