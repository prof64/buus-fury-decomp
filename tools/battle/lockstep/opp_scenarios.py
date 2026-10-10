"""Opponent visual audit scenarios (tools/battle/lockstep/visual.py records them, the wf-app example opp_audit compares).

Every Battle-list opponent type t gets the same set, named oa_tXX_<kind> (XX = type in hex, as scenarios.py):
  idle    the player stands still 600 frames, the opponent `at` to his right (its approach, attacks, projectiles; a
          boss KOs the player: knockdown, the game-over freeze)
  left    the opponent to the player's left, the player faces it (the opponent faces right) - 360 frames
  above   the opponent above the player (facing up / down) - 360 frames
  below   the opponent below the player - 360 frames
  punch   the player next to it presses A every 12 frames (hurt / knockback / knockdown, hit flashes) - 420 frames
  ki      Energy Blasts from 64 px, one every 20 frames (energy hits) - 360 frames
  ko      its HP set to 20, the player punches (the death animation and what it leaves) - 360 frames
The player is the lockstep default (form 2, Goku; HP / EP 900, stats 40) at POS (390, 290: a clearing) in the Southwest
Forest 8-3;
the opponent is the real one (`real`: its own AI from frame 1, aggro on, no target-dummy patch). Add a scenario: append
to SCENARIOS (vs() as scenarios.py: input [[keys, frames]..], at=(dx, dy) the enemy footprint's offset, etype, ehp,
state (its starting AI state), stats / hp for the player, partner for a second enemy), then
    .venv/bin/python tools/battle/lockstep/visual.py 'oa_t44_*'
"""
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
from scenarios import vs, ENEMY_ARCHETYPES, BOSSES  # noqa: E402

# the Battle list's opponents (launcher enemy_list): the game types it offers
OPPONENTS = [0, 6, 10, 18, 35, 37, 39, 40, 45, 49, 51, 60, 70, 80, 85, 129, 90, 91, 63, 96, 100, 112, 116, 118, 115, 121, 122,
             59, 130, 132, 142, 145, 148, 1, 2, 3, 5, 8, 9, 12, 15, 16, 17, 19, 20, 21, 22, 23, 86, 32, 33, 36, 41, 43, 44, 47,
             48, 50, 62, 64, 65, 66, 67, 68, 103, 69, 72, 74, 75, 76, 56, 77, 78, 82, 88, 89, 93, 94, 97, 98, 99, 101, 104, 106,
             107, 108, 113, 123, 124, 125, 128, 131, 133, 138, 149]
AT = dict(ENEMY_ARCHETYPES + BOSSES)
# the player's spot: the clearing right of the forest's centre trees (tree tops cover sprites at the lockstep VS_POS)
POS = [390, 290]
PUNCH = [['', 5]] + [['A', 1], ['', 11]] * 35
KI = [['', 5]] + [['B', 1], ['', 19]] * 18


def kinds(t):
    at = AT.get(t, (60, 0))
    d = max(abs(at[0]), 40)
    real = dict(etype=t, real=True, pos=POS)
    n = 'oa_t%02x_' % t
    return [
        dict(vs(2, 'x', [['', 600]], at=at, ehp=None, **real), name=n + 'idle'),
        dict(vs(2, 'x', [['', 360]], at=(-d, 0), dir=2, ehp=None, **real), name=n + 'left'),
        dict(vs(2, 'x', [['', 360]], at=(0, -40), dir=1, ehp=None, **real), name=n + 'above'),
        dict(vs(2, 'x', [['', 360]], at=(0, 40), dir=0, ehp=None, **real), name=n + 'below'),
        dict(vs(2, 'x', PUNCH, at=(16, 0), ehp=None, **real), name=n + 'punch'),
        dict(vs(2, 'x', KI, at=(64, 0), ehp=None, skills=[7], **real), name=n + 'ki'),
        dict(vs(2, 'x', PUNCH[:31] + [['', 180]], at=(16, 0), ehp=20, **real), name=n + 'ko'),
    ]


SCENARIOS = [s for t in OPPONENTS for s in kinds(t)]


# (opponent audit G2a: the bosses' longer patterns - an idle player that survives (HP 9999), and a fight with strong
# punches that reaches the HP-threshold phases)
G2A = [51, 122, 56, 18, 96, 145, 10, 142, 6, 100]


def g2a(t):
    at = AT.get(t, (60, 0))
    real = dict(etype=t, real=True, pos=POS)
    n = 'oa_t%02x_' % t
    fight = [['', 5]] + [['A', 1], ['', 11]] * 150
    return [
        dict(vs(2, 'x', [['', 1800]], at=at, ehp=None, hp=9999, **real), name=n + 'long'),
        dict(vs(2, 'x', fight, at=(16, 0), ehp=None, hp=9999, stats=[120, 120, 120, 120], **real), name=n + 'fight'),
    ]


SCENARIOS += [s for t in G2A for s in g2a(t)]
# (Yakon's summon of the Mini-Yakons: started in its state 0x325, PlayAnim12Yakon + SummonMiniYakons)
SCENARIOS += [dict(vs(2, 'x', [['', 900]], at=AT.get(145, (60, 0)), ehp=None, hp=9999, state=0x325, etype=145, real=True,
                      pos=POS), name='oa_t91_summon')]
