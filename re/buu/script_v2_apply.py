"""script_v2_apply.py -- write the round-2 emulator results (work/buu/script/v2_results.json, produced by
script_v2_t*.py) into engine/annotations/script_natives_{a,b,c}.json: status, evidence, and the renames /
semantics corrections the tests showed. Arity (pops/pushes) is never changed (all tests agreed with it)."""
import json
from common import *

ANN = ROOT / 'engine' / 'annotations'
res = json.loads((WORK / 'script' / 'v2_results.json').read_text())

FIX = {
    0x29: dict(name='set_bg_offset', args=['layer', 'dx', 'dy'], category='screen',
               semantics='Shift BG tilemap layer `layer` (slot in 0x03001990: the map\'s layer objects, vtable '
                         '0x08049410): offset (+0x44, +0x48) = the layer definition\'s offset + (dx, dy) px (<<10). '
                         'Method 0x08007FE8. Unused by table scripts.'),
    0x8a: dict(name='spawn_trigger_force', args=['index'], category='map',
               semantics='Create trigger #index of the current area\'s trigger list (area +0x10, {condition, def}) '
                         'and add it to the trigger registry [0x080716A0+0x10] (= 0x03002A64), ignoring its '
                         'condition. An index beyond the list reads garbage (hang).'),
    0x8b: dict(name='refresh_map_triggers', category='map',
               semantics='0x08008AA0(area, 0): clear the trigger registry, reload the map layout objects '
                         '(0x08009670: exits, props), then re-create every trigger of the area list (+0x10) whose '
                         'condition (0x08004784) holds now. Used after story flags change.'),
    0xad: dict(name='respawn_breakables',
               semantics='0x08008AFE: for each breakable prop of the current area (area +0x18 list, count area byte '
                         '5) whose condition (def+0) holds now, create it (0x0800C7E2, bound to its map prop by '
                         'def byte 4) unless it already exists.'),
    0x87: dict(semantics='Drop a zenie coin of SIZE at (X, Y) (0x08018346, zero velocity): a coin object/sprite '
                         'appears. Pickup was not observed in the test (the player stood still beside it).'),
    0x8e: dict(semantics='Abort the game: 0x03002A58 = 1, clear the registries, stop music, pop all modes '
                         '(0x0803D8F2) and install mode vtable 0x0804A404 (its enter 0x080053CC loads the title '
                         'palettes and plays song 45 = title theme). The script returns at once. In the injected '
                         'test the mode stayed installed but the screen stayed black for 1800 frames (the mode '
                         'expects to be entered from the boot path), so the visible title is not confirmed.'),
}
EXTRA_EVIDENCE = {
    0x36: 'round 2: in Yemma\'s Office spawn_map_object(4) created actor-list placement 4 whose condition is 1 '
          '(never) -> the native indexes the ACTOR list (+0x14, 0x08009108) and does not test the condition',
}
KEEP_INFERRED_REASON = {
    0xc9: 'emulator (round 2): no RAM effect in any available state (Snake Way, Yemma\'s Office, two South-West '
          'Forest fights): no entity there answers world message 0x1F; unused by table scripts',
    0xd4: 'emulator (round 2): SWI Stop is entered but the headless core does not wake it with L+R+Select (still '
          'asleep after 200 frames); behaviour read from 0x0803FA38 only',
}

changed = []
for part in 'abc':
    p = ANN / f'script_natives_{part}.json'
    t = json.loads(p.read_text())
    for e in t:
        n = e['id'] if isinstance(e['id'], int) else int(e['id'], 0)
        k = f'0x{n:02x}'
        if n in FIX:
            for f, v in FIX[n].items():
                if f == 'name' and e.get('name') != v:
                    e['previous_name'] = e.get('name')
                e[f] = v
        if n in EXTRA_EVIDENCE:
            e['evidence'] = (e.get('evidence') or '') + '; ' + EXTRA_EVIDENCE[n]
            e['semantics'] = ('Instantiate actor #index of the current area\'s actor list (area +0x14: enemies, NPCs) '
                              'now, without testing its condition (0x08009108).')
        if e.get('status') == 'INFERRED' and k in res:
            r = res[k]
            if r['status'] == 'VERIFIED':
                e['status'] = 'VERIFIED'
                e['evidence'] = 'emulator (round 2, re/buu/script_v2_t*.py): ' + r['evidence'] + \
                    ('; previously: ' + e['evidence'] if e.get('evidence') else '')
                changed.append(k)
            elif n in KEEP_INFERRED_REASON:
                e['evidence'] = KEEP_INFERRED_REASON[n] + ('; ' + e['evidence'] if e.get('evidence') else '')
    p.write_text(json.dumps(t, indent=1, ensure_ascii=False) + '\n')
print(len(changed), 'INFERRED -> VERIFIED:', ' '.join(changed))
