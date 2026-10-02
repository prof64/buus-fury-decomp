"""obj_show.py data/field/objects_X.json -- summary of an objects trace (events, object changes, pickups)"""
import json
import sys

t = json.load(open(sys.argv[1]))
print(t['name'], 'area', t['area'], 'init player', t['init']['player'], 'zenie', t['init']['zenie'])
print('init objs', [o[:2] + o[2:] for o in t['init']['objs']])
print('init triggers', t['init']['triggers'])
for ev in t['events']:
    if ev[1] == 'script' and len(sys.argv) < 3:
        continue
    print('EV', ev)
prev = None
for fr in t['frames']:
    sig = [(o[0], tuple(o[2:])) for o in fr['objs'] if o[0] not in ('player', 'hud', 'npc', 'enemy')]
    kinds = [o[0] for o in fr['objs']]
    extra = {k: fr[k] for k in ('zenie', 'supplies_changed', 'triggers') if k in fr}
    if sig != prev or extra:
        print(fr['f'], 'L' if fr['logic'] else '-', fr['p'], kinds, [o for o in fr['objs'] if o[0] in ('breakable', 'coin', 'drop_item', 'map_item')], extra)
    prev = sig
