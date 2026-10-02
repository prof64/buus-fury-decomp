"""Maps / tilesets / backgrounds export (emulator-driven: tiles and palettes are taken from VRAM after the
game itself loaded the map, layer tilemaps are decoded from ROM)."""
import hashlib
import struct
from gba import ASSETS, Rom, bgr555
from export import hx


def _tileset_image(vram, charbase, bpp, pal, ntiles=1024):
    from PIL import Image
    tsz = 64 if bpp == 8 else 32
    data = vram[charbase:charbase + ntiles * tsz]
    # trim trailing empty tiles
    n = len(data) // tsz
    while n and not any(data[(n - 1) * tsz:n * tsz]):
        n -= 1
    cols = 16
    rows = max(1, (n + cols - 1) // cols)
    im = Image.new('RGBA', (cols * 8, rows * 8), (0, 0, 0, 0))
    px = im.load()
    for t in range(n):
        ox, oy = (t % cols) * 8, (t // cols) * 8
        for i in range(64):
            if bpp == 8:
                c = data[t * 64 + i]
            else:
                b = data[t * 32 + (i >> 1)]
                c = (b >> 4) if i & 1 else (b & 15)
            if c:
                px[ox + (i & 7), oy + (i >> 3)] = pal[c] + (255,)
    return im, n


def _maps_iter(game, limit=None):
    """Yield (map_key, label, emu) for every map we can load from the ROM."""
    if game == 'log2':
        from emu_states import log2_warp
        n = 0x147 if limit is None else limit
        for mi in range(n):
            yield mi, f'map_{mi:03d}', log2_warp(mi)
    elif game == 'buu':
        from emu_states import buu_warp
        R = Rom('buu')
        keys = []
        for i in range(0x1C4):        # area table 0x0808E2E0, 0x38-byte records keyed (world, area)
            b = R.bytes(0x0808E2E0 + 0x38 * i, 2)
            if (b[0], b[1]) not in keys:
                keys.append((b[0], b[1]))
        for k in keys[:limit]:
            e, ok = buu_warp(*k)
            if ok:
                yield f'{k[0]}-{k[1]}', f'area_{k[0]:02d}_{k[1]:03d}', e


def export_maps(game, limit=None):
    from wf_maps import snapshot_render
    R = Rom(game)
    out_dir = ASSETS / game / 'maps'
    out_dir.mkdir(parents=True, exist_ok=True)
    seen = {}
    tilesets = {}
    maps = []
    for key, label, e in _maps_iter(game, limit):
        layers, comp = snapshot_render(e, game, R)
        if not layers:
            continue
        rkey = tuple(sorted(info['resource'] for _, info in layers.values()))
        vram = e.vram()
        pr = e.palram()
        pal = [bgr555(struct.unpack_from('<H', pr, i * 2)[0]) for i in range(256)]
        info0 = next(iter(layers.values()))[1]
        tsim, ntiles = _tileset_image(vram, info0['charbase'], info0['bpp'], pal)
        th = hashlib.md5(tsim.tobytes()).hexdigest()[:10]
        if th not in tilesets:
            tsim.save(out_dir / f'tileset_{th}.png')
            tilesets[th] = {'id': th, 'png': f'assets/{game}/maps/tileset_{th}.png', 'tiles': ntiles,
                            'bpp': info0['bpp'], 'first_map': key}
        if rkey in seen:
            maps.append({'map': key, 'same_as': seen[rkey]})
            continue
        seen[rkey] = key
        bb = None
        for img, info in layers.values():
            b = img.getbbox()
            if not b:
                continue
            ox, oy = info['offset_in_composite']
            b = (b[0] + ox, b[1] + oy, b[2] + ox, b[3] + oy)
            bb = b if bb is None else (min(bb[0], b[0]), min(bb[1], b[1]), max(bb[2], b[2]), max(bb[3], b[3]))
        if not bb:
            continue
        d = out_dir / label
        d.mkdir(exist_ok=True)
        comp.crop(bb).convert('RGB').save(d / 'composite.png')
        lay = []
        for li, (img, info) in sorted(layers.items()):
            img.save(d / f'bg{li}.png')
            lay.append({'bg': li, 'png': f'assets/{game}/maps/{label}/bg{li}.png', 'priority': info['priority'],
                        'bpp': info['bpp'], 'chunks': info['chunks'], 'scroll_rate': info['scroll_rate'],
                        'offset_in_composite': [info['offset_in_composite'][0] - bb[0],
                                                info['offset_in_composite'][1] - bb[1]],
                        'rom_field_resource': hx(info['resource'])})
        e.screenshot(d / 'screenshot.png', scale=1)
        maps.append({'map': key, 'composite': f'assets/{game}/maps/{label}/composite.png',
                     'screenshot': f'assets/{game}/maps/{label}/screenshot.png',
                     'size': [bb[2] - bb[0], bb[3] - bb[1]], 'tileset': th, 'layers': lay})
        print(game, 'map', key, maps[-1]['size'], flush=True)
    return {'maps': maps, 'tilesets': list(tilesets.values()),
            'maps_note': 'tilemaps decoded from ROM field resources; tiles + palette captured from VRAM after the game '
                         'loaded the map (log2: debug MAP TEST; buu: exit-trigger warp). Layers are aligned as on screen; '
                         'alpha-blended overlays are composited opaquely; parallax layers align at the capture camera.'}


def export_log1(limit=None):
    from emu_states import log1_warp
    from log1_maps import render_map, map_count
    R = Rom('log1')
    out_dir = ASSETS / 'log1' / 'maps'
    out_dir.mkdir(parents=True, exist_ok=True)
    maps = []
    tilesets = {}
    n = map_count(R) if limit is None else limit
    for mi in range(n):
        e = log1_warp(mi)
        planes, comp, rec = render_map(e, R, mi)
        if not planes:
            print("log1 map", mi, "has no planes"); continue
        vram, pr = e.vram(), e.palram()
        pal = [bgr555(struct.unpack_from('<H', pr, i * 2)[0]) for i in range(256)]
        info0 = next(iter(planes.values()))[1]
        tsim, ntiles = _tileset_image(vram, info0['charbase'], info0['bpp'], pal)
        th = hashlib.md5(tsim.tobytes()).hexdigest()[:10]
        if th not in tilesets:
            tsim.save(out_dir / f'tileset_{th}.png')
            tilesets[th] = {'id': th, 'png': f'assets/log1/maps/tileset_{th}.png', 'tiles': ntiles,
                            'bpp': info0['bpp'], 'first_map': mi}
        label = f'map_{mi:02d}'
        d = out_dir / label
        d.mkdir(exist_ok=True)
        comp.convert('RGB').save(d / 'composite.png')
        lay = []
        for bg, (img, info) in sorted(planes.items()):
            img.save(d / f'bg{bg}.png')
            lay.append({'bg': bg, 'png': f'assets/log1/maps/{label}/bg{bg}.png', 'priority': info['priority'],
                        'bpp': info['bpp'], 'rom_screen_list': hx(info['rom_screen_list'])})
        e.screenshot(d / 'screenshot.png', scale=1)
        maps.append({'map': mi, 'rom_record': hx(rec), 'composite': f'assets/log1/maps/{label}/composite.png',
                     'screenshot': f'assets/log1/maps/{label}/screenshot.png', 'size': list(comp.size),
                     'tileset': th, 'layers': lay})
        print('log1 map', mi, comp.size, flush=True)
    return {'maps': maps, 'tilesets': list(tilesets.values()),
            'maps_note': 'screens (30x20 raw text-mode entries) read from ROM map records; tiles + palette captured '
                         'from VRAM after load_map(map, default entrance) was forced at boot'}


def export(game):
    if game in ('log2', 'buu'):
        return export_maps(game)
    if game == 'log1':
        return export_log1()
    return {}
