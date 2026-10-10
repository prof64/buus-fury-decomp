"""Extract Buu's Fury map collision (walk + projectile bitmaps) for every map -> data/battle/buu_collision.json.

Format (see tools/battle/COLLISION.md): each map header (found through its BG field resources, header+0x14+4*bg)
points at two webfoot-LZ blobs, header+0x34 = walk bitmap (player layer A, copied to the enemy layer C at load) and
header+0x38 = projectile bitmap (layer B). Each inflates to 0x2000 bytes: 256 rows x 32 bytes, bit (x & 7) of byte
row*32 + (x >> 3) is tile (x, row) of 8x8 px, 1 = passable.

Output per map: {w, h} in tiles (the walkable bounding box, at least the map render's size; everything outside is
blocked) and the two layers as run lengths over the w*h bits in row-major order, starting with a blocked run, LEB128
varints, base64. The bitmaps are the bare terrain: objects that mark their own tiles at load (NPCs, enemies, rocks) are
not in them. Optional `origin` [x, y]: world position of the map composite's top-left pixel when it is not (0, 0).

usage: .venv/bin/python tools/battle/buu_collision.py [--check] [--origins [MAP ...]]
  --check:   also load work/states/buu_fight_swforest.ss and compare map 8-3 with the live RAM bitmaps
  --origins: measure the composite origin of the given maps (default: the battle arenas) in the emulator: warp to a
             walkable tile, read the camera (map object +0x18 = world px of screen 0,0) and find the screen in the
             composite PNG (Snakeway warp, re/buu/warp.py). Without the flag, origins already in the JSON are kept.
"""
import base64
import json
import os
import struct
import sys

import numpy as np

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(os.path.dirname(HERE))
sys.path.insert(0, os.path.join(ROOT, 'tools', 'extract'))
from gba import wf_decompress  # noqa: E402

ROM = open(os.path.join(ROOT, 'roms', 'buu.gba'), 'rb').read()
WORDS = np.frombuffer(ROM[: len(ROM) & ~3], dtype=np.uint32)


def u32(a):
    return struct.unpack_from('<I', ROM, a - 0x08000000)[0]


def is_blob(p):
    return 0x08000000 <= p < 0x08000000 + len(ROM) and u32(p) in (0, 1, 2) and u32(p + 4) == 0x2000


def header_for(m):
    """map header address from the BG field resources listed in sprites.json"""
    hs = set()
    for l in m.get('layers', []):
        res = int(l['rom_field_resource'], 16)
        for i in np.nonzero(WORDS == res)[0]:
            h = 0x08000000 + 4 * int(i) - 0x14 - 4 * l['bg']
            if is_blob(u32(h + 0x34)) and is_blob(u32(h + 0x38)):
                hs.add(h)
    return sorted(hs)


def bitmap(p):
    d, _ = wf_decompress(ROM, p - 0x08000000)
    a = np.frombuffer(d[:0x2000], dtype=np.uint8).reshape(256, 32)
    return np.unpackbits(a, axis=1, bitorder='little')  # [256 rows][256 cols], 1 = passable


def varint(n, out):
    while True:
        b = n & 0x7F
        n >>= 7
        if n:
            out.append(b | 0x80)
        else:
            out.append(b)
            return


def rle(bits):
    out = bytearray()
    cur = 0
    run = 0
    for b in bits.ravel():
        if b == cur:
            run += 1
        else:
            varint(run, out)
            cur ^= 1
            run = 1
    varint(run, out)
    return base64.b64encode(bytes(out)).decode()


ARENA_MAPS = ['8-71', '15-32', '18-14', '20-7', '8-68', '8-70', '8-40', '6-16', '6-9', '7-6', '8-3', '19-3', '15-1', '5-1', '20-5']


def unrle(s, n):
    b = base64.b64decode(s)
    out = np.zeros(n, dtype=np.uint8)
    i = p = cur = 0
    while i < len(b):
        v = sh = 0
        while True:
            c = b[i]
            i += 1
            v |= (c & 0x7F) << sh
            sh += 7
            if not c & 0x80:
                break
        if cur:
            out[p:p + v] = 1
        p += v
        cur ^= 1
    return out


def measure_origin(e, st0, mid, entry, composite):
    """warp into map `mid`, return (origin [x, y], match ratio, camera)"""
    from PIL import Image
    from warp import warp
    wd, mp = (int(v) for v in mid.split('-'))
    bits = unrle(entry['walk'], entry['w'] * entry['h']).reshape(entry['h'], entry['w'])
    ys, xs = np.nonzero(bits)
    # a walkable tile near the top-left of the walkable area: the screen then shows map edges (no periodic match)
    k = int(np.argmin((ys - ys.min() - 4) ** 2 + (xs - xs.min() - 4) ** 2))
    e.load(st0)
    warp(e, wd, mp, int(xs[k]) * 8, int(ys[k]) * 8)
    e.run(60)
    cam = [struct.unpack('<i', struct.pack('<I', e.r32(0x030019BC + 4 * i)))[0] for i in range(2)]
    scr = np.asarray(Image.frombytes('RGB', (240, 160), e.pixels())).astype(np.int32)
    comp = np.asarray(Image.open(composite).convert('RGB')).astype(np.int32)
    H, W = comp.shape[:2]
    sy = np.arange(0, 160, 4)
    sx = np.arange(0, 240, 4)
    S = scr[np.ix_(sy, sx)]
    best = None
    for oy in range(cam[1] - 96, cam[1] + 97):
        for ox in range(cam[0] - 96, cam[0] + 97):
            yy = sy + oy
            xx = sx + ox
            my = (yy >= 0) & (yy < H)
            mx = (xx >= 0) & (xx < W)
            if my.sum() < 20 or mx.sum() < 30:
                continue
            eq = (np.abs(comp[np.ix_(yy[my], xx[mx])] - S[np.ix_(my, mx)]).sum(axis=2) < 24).mean()
            if best is None or eq > best[0]:
                best = (eq, ox, oy)
    eq, ox, oy = best
    return [cam[0] - ox, cam[1] - oy], float(eq), cam


def main():
    sprites = json.load(open(os.path.join(ROOT, 'data', 'buu', 'sprites.json')))
    path = os.path.join(ROOT, 'data', 'battle', 'buu_collision.json')
    old = json.load(open(path))['maps'] if os.path.exists(path) else {}
    out = {
        'source': 'tools/battle/buu_collision.py: ROM map header +0x34 (walk, player + enemies) / +0x38 (projectiles), '
                  'webfoot-LZ 256x256 bit grid of 8x8 tiles, 1 = passable; see tools/battle/COLLISION.md',
        'encoding': 'per layer: run lengths over w*h tiles (row-major), first run = blocked, LEB128 varints, base64; '
                    'tiles outside w*h are blocked',
        'maps': {},
    }
    missing = []
    for m in sprites['maps']:
        hs = header_for(m)
        if len(hs) != 1:
            missing.append(m['map'])
            continue
        h = hs[0]
        walk = bitmap(u32(h + 0x34))
        shot = bitmap(u32(h + 0x38))
        ys, xs = np.nonzero(walk)
        if not len(xs):
            missing.append(m['map'])
            continue
        size = m.get('size') or [0, 0]
        w = min(256, max(int(xs.max()) + 1, (size[0] + 7) // 8))
        hh = min(256, max(int(ys.max()) + 1, (size[1] + 7) // 8))
        ent = out['maps'][m['map']] = {
            'header': '0x%08X' % h,
            'w': w,
            'h': hh,
            'walk': rle(walk[:hh, :w]),
            'shot': rle(shot[:hh, :w]),
        }
        if old.get(m['map'], {}).get('origin'):
            ent['origin'] = old[m['map']]['origin']
    if '--origins' in sys.argv:
        sys.path.insert(0, os.path.join(ROOT, 're', 'buu'))
        from common import emu
        mids = [a for a in sys.argv[sys.argv.index('--origins') + 1:] if not a.startswith('--')] or ARENA_MAPS
        e = emu('buu_snakeway_start.ss')
        st0 = e.save()
        comps = {m['map']: m for m in sprites['maps']}
        for mid in mids:
            ent = out['maps'][mid]
            org, match, cam = measure_origin(e, st0, mid, ent, os.path.join(ROOT, 'site', 'public', comps[mid]['composite']))
            print('origin', mid, org, 'match %.3f' % match, 'camera', cam)
            if match < 0.8:
                print('  low match: not stored')
                continue
            ent.pop('origin', None)
            if org != [0, 0]:
                ent['origin'] = org
    with open(path, 'w') as f:
        json.dump(out, f, separators=(',', ':'))
    print(len(out['maps']), 'maps,', os.path.getsize(path), 'bytes; no header / layers:', ' '.join(missing))

    if '--check' in sys.argv:
        sys.path.insert(0, os.path.join(ROOT, 're', 'buu'))
        from common import emu
        e = emu('buu_fight_swforest.ss')
        e.run(1)
        M = 0x030019A4  # map object; +0xA8 walk (player), +0xAC projectile, +0xB0 enemy walk (copy of +0xA8)
        h = int(out['maps']['8-3']['header'], 16)
        for name, off, blob in (('walk/player', 0xA8, 0x34), ('shot', 0xAC, 0x38), ('walk/enemy', 0xB0, 0x34)):
            ram = np.unpackbits(np.frombuffer(e.read(e.r32(M + off), 0x2000), dtype=np.uint8).reshape(256, 32), axis=1, bitorder='little')
            rom = bitmap(u32(h + blob))
            diff = np.argwhere(ram != rom)
            print(name, 'tiles differing from ROM (objects marked at load):', len(diff), [tuple(int(v) for v in t[::-1]) for t in diff[:12]])


if __name__ == '__main__':
    main()
