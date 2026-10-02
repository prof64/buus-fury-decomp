"""The player's animation timing as the game runs it -> data/battle/buu_anim_timing.json.

Buu's Fury has no animation scripts (tools/extract/GRAPHICS.md §4): the actor's state object (obj+0x14) advances the
frame. SetAnim(id) (player vfunc +0x28, 0x0800ECF0) picks the animation raw = table 0x08072670[id] + 1 and the state
object 0x0807273C[id] (id 0 = the form's idle state). This reads both tables and classifies every state's tick
(vtable +4) and end callback (vtable +0x24):

  once  0x0800DE6C  frame = tick >> shift; past the last frame -> end callback
  loop  0x0800DEB0  frame = tick >> shift, wraps to frame 0
  idle  0x0800DFD4  2-frame animations: frame 1 while (tick & 0xFF) > 0xF0 (blink), else a shift-3 loop; the state's
                    enter draws tick = rand(255)
  bored 0x0800DF8A  frame = tick >> 5, then loops frames 2.. (tick 64)
  hold  0x0804E4E0  no tick (frame kept)
  end   0x0800DE94 -> SetAnim(0) (idle), 0x0800DED0 -> the hold state 0x03001B00 (last frame kept), 0x0800DEA6 -> none

`durations` = frames shown per animation frame in the state SetAnim gives that raw animation (the lowest id), used for
the simulator's sprite previews; the battle itself runs the state machines (site/src/battle/rulesets/buu/panim.ts,
player.ts), verified frame by frame by the lockstep harness (tools/battle/LOCKSTEP.md). `skills` lists the skill
states' timing from the decoded state code (same verification).

usage: .venv/bin/python tools/battle/gen_buu_anim_timing.py
"""
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(os.path.dirname(HERE))
sys.path.insert(0, os.path.join(ROOT, 're', 'buu'))
from common import emu, u8, u32, s32  # noqa: E402

ANIM_MAP = 0x08072670
ANIM_STATE = 0x0807273C
TICKS = {0x0800DE6D: 'once', 0x0800DEB1: 'loop', 0x0800DFD5: 'idle', 0x0800DF8B: 'bored', 0x0804E4E1: 'hold',
         0x08012FC5: 'loop+dash sound'}
ENDS = {0x0800DE95: 'idle', 0x0800DED1: 'hold', 0x0800DEA7: 'none'}
NAMES = {0: 'idle', 1: 'walk', 2: 'dash', 3: 'kiblast', 4: 'attack1', 5: 'attack2', 6: 'attack3', 7: 'guard', 8: 'bored',
         9: 'hurt', 10: 'special_11', 11: 'special_12', 12: 'special_13', 13: 'special_14', 14: 'special_15',
         15: 'special_16', 16: 'knockdown', 17: 'special_19', 18: 'special_20', 19: 'special_21', 20: 'special_22',
         21: 'special_23', 22: 'special_24', 23: 'transform (SS)', 24: 'revert'}

# skill states (heap objects, vtables in the skill table's activate functions); decoded in tools/battle/LOCKSTEP.md
SKILLS = {
    7: dict(raw=4, timing='frame 0, at tick 16 frame 1 + the blast; tick 40: a queued B press (EP) toggles frame 1/2 and '
                          'rewinds to tick 16, else idle'),
    15: dict(raw=19, timing='charge frame 0 for 24 ticks; held at 24: fire, frame ((tick >> 3) & 1) + 1 while held'),
    16: dict(raw=19, timing='as Kamehameha'), 6: dict(raw=19, timing='as Kamehameha'),
    1: dict(raw=19, timing='as Kamehameha with frame (tick >> 3) & 1'), 3: dict(raw=19, timing='as Kamehameha'),
    11: dict(raw=19, timing='charge frame 0 for 24 ticks; then the tick advances 2 per frame: frame ((tick >> 3) & 1) + 1'),
    10: dict(raw=20, timing='charge: frame (tick >> 2) & 1, 2 from tick 20; fired at 24: frame ((tick >> 3) & 1) + 3'),
    2: dict(raw=19, timing='held: frame (tick >> 3) & 1; released: frame kept, idle 17 ticks later'),
    17: dict(raw=13, timing='frame 0 until the B release; then frame 1 for 31 frames'),
    4: dict(raw=4, timing='frame 0, at tick 16 frame 1 (launch); idle at tick 32'),
    23: dict(raw=4, timing='frame tick >> 4 (0, 1, 2); idle after tick 32'),
    13: dict(raw=20, timing='frame tick >> 3 (0..4) up to tick 40; held after 40: frame 0'),
    5: dict(raw=20, timing='frame tick >> 3 until the dragon (tick 16); then ((tick >> 3) & 1) + 2; after the launch 24 ticks'),
    8: dict(raw=20, timing='frame tick >> 2 until tick 8; then ((tick >> 2) & 1) + 2 up to tick 59'),
    9: dict(raw=20, timing='charging: frame (tick >> 2) & 1; swinging (90 frames): frame ((counter / 3) & 7) + 2'),
    14: dict(raw=20, timing='target pick: hold state frame 0; teleport: frames (-t >> 3), (24 - t) >> 3, punch (t - 24) >> 2'),
    18: dict(raw=20, timing='once, shift 2 (into form 14); from form 14: raw 12, once shift 2'),
    20: dict(raw=11, timing='once, shift 4 (16 frames per frame); the gauge fills with the tick'),
    22: dict(raw=11, timing='as Super Saiyan'),
}


def main():
    e = emu('buu_fight_swforest.ss')  # the state singletons live in IWRAM
    e.run(2)
    setanim = []
    for i in range(25):
        n = u8(ANIM_MAP + i)
        obj = u32(ANIM_STATE + 4 * i)
        vt = e.r32(obj) if obj else 0
        slot = lambda s: (vt + e.r32(vt + 4 * s)) & 0xFFFFFFFF if vt else 0  # noqa: E731
        tick = TICKS.get(slot(1), 'custom 0x%08X' % slot(1))
        end = ENDS.get(slot(9), '-') if tick in ('once',) else '-'
        shift = e.r8(obj + 4) if tick in ('once', 'loop', 'loop+dash sound') else None
        if obj == 0x03002B44:
            tick, shift, end = 'idle', 3, '-'
        setanim.append({'id': i, 'name': NAMES.get(i, ''), 'raw': n + 1, 'state': '0x%08X' % obj, 'vtable': '0x%08X' % vt,
                        'tick': tick, 'shift': shift, 'end': end})
    durations = {}
    # the player's raws 11 / 12 are the transformation / revert (ids 23 / 24) rather than the generic loop of id 10 / 11
    for s in [setanim[23], setanim[24]] + setanim[:23]:
        if str(s['raw']) in durations:
            continue
        if s['tick'] == 'idle':
            durations[str(s['raw'])] = {'two_frame': [241, 15], 'other': 8, 'loop': True}
        elif s['tick'].startswith('loop'):
            durations[str(s['raw'])] = {'each': 1 << s['shift'], 'loop': True}
        elif s['tick'] == 'once':
            durations[str(s['raw'])] = {'each': 1 << s['shift'], 'loop': False, 'end': s['end']}
        elif s['tick'] == 'bored':
            durations[str(s['raw'])] = {'each': 32, 'loop': True, 'loop_from': 2}
    for k, v in SKILLS.items():
        if str(v['raw']) not in durations:
            durations[str(v['raw'])] = {'each': 8, 'loop': True, 'note': 'skill-driven (see skills)'}
    doc = {'source': 'tools/battle/gen_buu_anim_timing.py (ROM tables 0x08072670 / 0x0807273C + the IWRAM state objects)',
           'setanim': setanim, 'durations': durations, 'skills': {str(k): v for k, v in sorted(SKILLS.items())}}
    path = os.path.join(ROOT, 'data', 'battle', 'buu_anim_timing.json')
    with open(path, 'w') as f:
        json.dump(doc, f, indent=1)
    for s in setanim:
        print(s)
    print('wrote', path)


if __name__ == '__main__':
    main()
