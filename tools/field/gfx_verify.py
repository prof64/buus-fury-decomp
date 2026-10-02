"""gfx_verify.py CONTENT_BASE: check the exported field object graphics against the running game (headless).

For each case: run a scenario to a frame where the object is static, find its OAM entry, screenshot, and compare
every opaque pixel of the exported frame (effects/<id>/sheet.png + effect.json, drawn at the OAM position) with the
screenshot (tolerance 8 per channel for the 5-bit colour expansion). Also checks the camera-relative position
implied by the draw rule against the OAM position (static frames only)."""
import json
import os
import sys

from PIL import Image

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import rec_objects as R  # noqa: E402
from gfx_probe import oam, cam  # noqa: E402
from warp import warp  # noqa: E402

BASE = sys.argv[1]


def frame_img(eid, k):
    e = json.load(open(os.path.join(BASE, 'effects', eid, 'effect.json')))
    sh = Image.open(os.path.join(BASE, 'effects', eid, 'sheet.png')).convert('RGBA')
    f = e['frames'][k]
    return sh.crop((f['x'], f['y'], f['x'] + f['w'], f['y'] + f['h'])), (f['anchor']['x'], f['anchor']['y'])


def compare(shot, img, sx, sy):
    sp = shot.load()
    ip = img.load()
    bad = n = 0
    for y in range(img.size[1]):
        for x in range(img.size[0]):
            c = ip[x, y]
            if c[3] == 0:
                continue
            X, Y = sx + x, sy + y
            if not (0 <= X < shot.size[0] and 0 <= Y < shot.size[1]):
                continue
            if 8 <= X < 88 and 8 <= Y < 24:  # the HUD (OBJ priority 1) covers it
                continue
            n += 1
            s = sp[X, Y]
            if max(abs(s[i] - c[i]) for i in range(3)) > 8:
                bad += 1
    return n, bad


def run_scenario(name, frame):
    sc = next(s for s in R.SCENARIOS if s['name'] == name)
    e = R.emu(sc['state'])
    sc['setup'](e)
    keys = []
    for k, n in sc['input']:
        keys += [R.keymask(k)] * n
    for f in range(frame + 1):
        e.set_keys(keys[f] if f < len(keys) else 0)
        e.run(1)
    return e


def cam_from_player(e):
    """the camera the OAM was built with: player position - its ground shadow's OAM position (16x8 4bpp)"""
    p = e.r32(R.PLAYER)
    px, py = e.r32(p + 0x1A0) >> 8, e.r32(p + 0x1A4) >> 8
    sh = [o for o in oam(e) if o['w'] == 16 and o['h'] == 8 and o['bpp8'] == 0 and o['prio'] == 3]
    best = min(sh, key=lambda o: abs(o['x'] - (px - cam(e)[0])) + abs(o['y'] - (py - cam(e)[1])))
    return px - best['x'], py - best['y']


def shot(e, path):
    e.screenshot(path, scale=1)
    return Image.open(path).convert('RGBA')


OUT = os.path.join(R.ROOT, 'work', 'field_gfx')
os.makedirs(OUT, exist_ok=True)
results = []


def check(label, s, eid, k, sx, sy):
    img, anc = frame_img(eid, k)
    n, bad = compare(s, img, sx, sy)
    results.append((label, n, bad))
    print(f'{label:40s} pixels {n:4d} mismatched {bad}')


# 1. coins (landed, static) + their shadows; props of 8-1 (breakable #1 still there)
e = run_scenario('breakable_8_1', 141)
s = shot(e, os.path.join(OUT, 'v_coins.png'))
cx, cy = cam_from_player(e)
W = R.WORLD
coins = []
for k in range(e.r32(W + 4)):
    o = e.r32(W + 8 + 4 * k)
    if e.r32(o) == 0x08049CC4:
        x, y, z = e.r32(o + 0x48), e.r32(o + 0x4C), 0
        coins.append((e.r32(o + 0x48), e.r32(o + 0x4C)))
# coins draw at (x>>8 - 4, y>>8 - 8); objs report x,y 24.8 at +0x1A0? use the recorder's object list
for o in R.objects(e):
    if o[0] == 'coin':
        x, y = o[2] >> 8, o[3] >> 8
        img, anc = frame_img('coin_0', 0)
        check(f'coin at ({x},{y})', s, 'coin_0', 0, x - anc[0] - cx, y - anc[1] - cy)
        sh, sa = frame_img('shadow_4x2', 0)
        n, bad = compare(s, sh, o[7] - sa[0] - cx, o[8] - sa[1] - cy)
        results.append(('coin shadow', n, bad))
        print(f'{"  shadow_4x2 at box":40s} pixels {n:4d} mismatched {bad}')
    if o[0] == 'breakable':
        pass
check('prop 8-1 #2 (breakable #1)', s, 'props_area_08_001', 2, 272 - cx, 96 - cy)

# 2. dropped items, landed
e = run_scenario('breakable_drops_8_1', 150)
s = shot(e, os.path.join(OUT, 'v_drops.png'))
cx, cy = cam_from_player(e)
for o in R.objects(e):
    if o[0] == 'drop_item':
        item, x0, y0 = o[2], o[8], o[9]
        k = item if item >= 0 else 100 - item
        check(f'dropped item {item} at ({x0},{y0})', s, 'field_items', k, x0 - cx, y0 - cy)
        sh, sa = frame_img('shadow_10x4', 0)
        n, bad = compare(s, sh, x0 - sa[0] - cx, y0 + 10 - sa[1] - cy)
        results.append(('item shadow', n, bad))
        print(f'{"  shadow_10x4 at (x0, y0+10)":40s} pixels {n:4d} mismatched {bad}')

# 3. map item 8-57 (supply 37 at 112,120) + its breakable prop (player placed clear of both)
e = R.emu('buu_snakeway_start.ss')
assert warp(e, 8, 57, 112, 170)
R.place(e, 150, 150, 0, settle=40)
s = shot(e, os.path.join(OUT, 'v_item.png'))
cx, cy = cam_from_player(e)
check('map item supply 37 (112,120)', s, 'field_items', 37, 112 - cx, 120 - cy)
check('prop 8-52 #0 (breakable)', s, 'props_area_08_052', 0, 40 - cx, 56 - cy)

# 4. level sign label 8-2 (def 120,40, "70" colour 4) and its gate prop
e = R.emu('buu_snakeway_start.ss')
assert warp(e, 8, 2, 130, 90)
R.place(e, 130, 90, 0, settle=30)
s = shot(e, os.path.join(OUT, 'v_label.png'))
cx, cy = cam_from_player(e)
lab = [o for o in oam(e) if o['w'] == 32 and o['h'] == 16 and o['prio'] == 3]
objs = json.load(open(os.path.join(BASE, 'maps', 'area_08_002', 'objects.json')))
ms = [o for a in objs['areas'] if a['key'] == '8-2' for o in a['objects'] if o['type'] == 'map_sprite']
f = ms[0]['label']['frame']
img, anc = frame_img('field_labels', f)
print('label OAM', lab, 'def', ms[0]['pos'], 'cam', (cx, cy), 'bob =', lab[0]['y'] - (ms[0]['pos'][1] - anc[1] - cy))
check('label "70" (OAM position)', s, 'field_labels', f, lab[0]['x'], lab[0]['y'])
p0 = objs['props'][0]
check('prop 8-2 #0 (gate)', s, 'props_area_08_002', 0, p0['rect']['x'] - cx, p0['rect']['y'] - cy)

print('summary', sum(r[2] for r in results), 'mismatched pixels in', len(results), 'checks')
