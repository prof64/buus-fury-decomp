"""Dump LoG1 collision layers for the battle simulator arenas.

The game keeps one bit per 8x8 tile and layer at 0x02005040 + layer*0x1D4C (row-major, map width bits per row,
bit set = passable). Layers: 0 ground (walking, ground enemies), 1 flying (flight, pterodactyls), 2 projectile
walls. Tested by 0x08004FFA(map, rect, layer). Each map is loaded from work/states/log1_maps/log1_mapNN.ss
(load_map forced at the default entrance, see re/log1/warp.py).

At load NPC actors, pickups (0x08005DA6) and breakable obstacles mark their own rectangle as blocked on the ground
layer (0x0800507A(map, rect, 0, 0)). The simulator spawns its own actors (and re-blocks its pickups), so those
rectangles are made passable again here using the placements of data/log1/maps.json; the result is the bare
terrain. (Enemies do not block their tiles.)

usage: .venv/bin/python tools/battle/log1_collision.py 2 9 23 ...   -> data/battle/log1_collision.json
"""
import base64, json, os, sys
HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(os.path.dirname(HERE))
sys.path.insert(0, os.path.join(ROOT, 're/log1'))
from common import emu  # noqa: E402

BASE = 0x02005040
STRIDE = 0x1D4C
maps = {m['index']: m for m in json.load(open(os.path.join(ROOT, 'data/log1/maps.json')))['maps']}
items = {i['id']: i for i in json.load(open(os.path.join(ROOT, 'data/log1/items.json')))['items']}

out = {'source': 'tools/battle/log1_collision.py (emulator RAM 0x02005040 after load_map, 1 bit per tile, set = passable; actor/pickup blocks removed)', 'maps': {}}
for m in [int(a) for a in sys.argv[1:]]:
    e = emu(os.path.join(ROOT, 'work/states/log1_maps/log1_map%02d.ss' % m))
    e.run(2)
    mp = e.r32(0x03000600)
    w, h = e.r16(mp + 4), e.r16(mp + 6)
    n = (w * h + 7) // 8
    layers = [bytearray(e.read(BASE + l * STRIDE, n)) for l in range(3)]
    rects = [(a['x'], a['y'], a['x'] + 16, a['y'] + 8) for a in maps[m]['actors']]
    rects += [(p['x'], p['y'], p['x'] + items[p['item_id']]['size_px']['w'], p['y'] + items[p['item_id']]['size_px']['h']) for p in maps[m]['pickups']]
    # breakable obstacles (0x0800697E) are sprite objects the simulator does not place: free their 16x16 too
    rects += [(o['x'], o['y'], o['x'] + 16, o['y'] + 16) for o in maps[m]['obstacles']]
    freed = 0
    for x0, y0, x1, y1 in rects:
        for ty in range(max(0, y0 >> 3), min(h, (y1 + 7) >> 3)):
            for tx in range(max(0, x0 >> 3), min(w, (x1 + 7) >> 3)):
                i = ty * w + tx
                if not (layers[0][i >> 3] >> (i & 7)) & 1:
                    freed += 1
                layers[0][i >> 3] |= 1 << (i & 7)
    out['maps'][str(m)] = {'w': w, 'h': h, 'layers': [base64.b64encode(bytes(l)).decode() for l in layers]}
    print(m, w, h, n, 'tiles freed from actor/pickup blocks', freed)
json.dump(out, open(os.path.join(ROOT, 'data/battle/log1_collision.json'), 'w'))
