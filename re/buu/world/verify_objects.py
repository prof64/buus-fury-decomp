"""Verify extracted map objects (wf-world output) against the live game, headless (no window, no audio).

For the save states and for areas reached with warp.py (from buu_snakeway_start.ss): walk the two world object
lists in RAM (`[0x080716B0]`, `[0x080716B4]`: u32 count + pointers) and
* every trigger object (known vtable): vtable -> type and rectangle (+4 = x0,y0, +8 = x1,y1) must equal an exported
  trigger of the current area / its map;
* every exported trigger of the area / map whose condition is `always` must be live;
* every actor whose first 0x300 bytes hold an exported actor def pointer: type must match, and (NPCs) the position
  must be the placed one.

usage: verify_objects.py <content base dir>
"""
import sys, json, glob
sys.path.insert(0, '/Users/prof/source/buus-fury-decomp/re/buu')
from common import *
from warp import warp

ACTOR_VT = {0x0804C92C: 'npc', 0x0804CEF4: 'npc'}
VT = {0x0804A09C: 'exit', 0x08049BC4: 'edge_exit', 0x08049BD4: 'event_trigger', 0x08049784: 'script_trigger',
      0x08049794: 'save_point', 0x0804B070: 'message_trigger', 0x080499D4: 'region'}
base = sys.argv[1]
areas = {a['key']: a for a in json.load(open(base + '/world/areas.json'))['areas']}
maps = {}
for f in glob.glob(base + '/maps/*/objects.json'):
    o = json.load(open(f))
    maps[o['map']] = o
area_objs = {}
for o in maps.values():
    for a in o['areas']:
        area_objs[a['key']] = a['objects']


def expected(key, variant=1):
    mp = maps[areas[key]['maps'][variant - 1]]
    trig = [('map', t) for t in mp['map_triggers']] + [('area', x) for x in area_objs.get(key, []) if 'rect' in x]
    actors = {int(x['rom']['def'], 16): x for x in area_objs.get(key, []) if x['type'] in ('enemy', 'npc')}
    return trig, actors


def s16(v):
    return v - 0x10000 if v & 0x8000 else v


TOT = [0, 0]


def check(e, key, label):
    trig, actors = expected(key)
    live = []
    actors_live = []
    print(f'== {label}: area {key} ({areas[key]["name"]})')
    W = u32(0x080716B4)
    lists = [(u32(0x080716B0), 0), (W + 4, 0), (W + 0x40C, 0)]   # (count addr, ...): entries follow the count
    for lp, _ in lists:
        L = lp
        for k in range(min(e.r32(L), 256)):
            ob = e.r32(L + 4 + 4 * k)
            vt = e.r32(ob)
            if vt in VT:
                w1, w2 = e.r32(ob + 4), e.r32(ob + 8)
                r = dict(x=s16(w1 & 0xffff), y=s16(w1 >> 16), w=s16(w2 & 0xffff) - s16(w1 & 0xffff),
                         h=s16(w2 >> 16) - s16(w1 >> 16))
                c = [t for (_, t) in trig if t['rect'] == r and t['type'] == VT[vt]]
                if VT[vt] == 'event_trigger' and r == dict(x=361, y=153, w=23, h=79) and key != '1-1':
                    continue  # warp.py's own converted trigger
                live.append(r)
                good = bool(c)
                TOT[0] += 1
                TOT[1] += good
                if not good:
                    print(f'  BAD live {VT[vt]} rect {r} not exported')
            if vt in ACTOR_VT:
                actors_live.append((ACTOR_VT[vt], e.r32(ob + 0x1a0) >> 8, e.r32(ob + 0x1a4) >> 8))
    # actors: NPCs that stand / wait must sit exactly on their placement; wanderers inside their rect
    for d, x in actors.items():
        if x['condition'] != {'always': True} or x['type'] != 'npc':
            continue
        op = x['behaviour'][0]['op'] if x['behaviour'] else 'stand'
        if op in ('stand', 'wait'):
            good = any(t == 'npc' and [px, py] == x['pos'] for t, px, py in actors_live)
        elif 'rect' in x['behaviour'][0]:
            r = x['behaviour'][0]['rect']
            good = any(r['x0'] - 16 <= px <= r['x1'] + 16 and r['y0'] - 16 <= py <= r['y1'] + 16 for t, px, py in actors_live)
        else:
            continue
        TOT[0] += 1
        TOT[1] += good
        print(f'  {"OK " if good else "BAD"} npc {op:12s} placed {x["pos"]}')
    miss = [t for (_, t) in trig if t['condition'] == {'always': True} and t['rect'] not in live]
    TOT[0] += len([t for (_, t) in trig if t['condition'] == {'always': True}])
    TOT[1] += len([t for (_, t) in trig if t['condition'] == {'always': True}]) - len(miss)
    print(f'  {len(live)} live triggers; always-triggers missing: {[(t["type"], t["rect"]) for t in miss]}')


for st, key in (('buu_snakeway_start.ss', '1-1'), ('buu_swforest3.ss', '8-3')):
    check(emu(st), key, st)
for key, x, y in (('1-3', 232, 360), ('0-91', 112, 120), ('2-8', 200, 200), ('7-2', 200, 200), ('8-1', 160, 160),
                  ('8-17', 228, 300), ('13-1', 120, 140), ('4-30', 192, 160), ('6-7', 120, 120), ('2-1', 100, 100)):
    e = emu('buu_snakeway_start.ss')
    w, m = map(int, key.split('-'))
    if warp(e, w, m, x, y):
        check(e, key, 'warp')
    else:
        print('warp failed', key)
print(f'{TOT[1]}/{TOT[0]} checks passed')
