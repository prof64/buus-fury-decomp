"""LoG2 / Buu field (map) rendering.

A map layer is a ROM 'field resource' object (vtable 0x080056A1 in LoG2):
  +0x10 bits16..23 = BG layer index, +0x14 u8 chunk columns, +0x15 u8 chunk rows,
  +0x18 u32[cols*rows] -> webfoot-lz blobs, each 0x800 bytes = 32x32 GBA text-mode screen entries.
At runtime each BG layer is driven by a field-layer object in EWRAM (vtable 0x08023868 in LoG2) whose
+0x10 points at the resource. Tiles and palette are whatever the map loader put in VRAM/palette RAM, so
maps are rendered from an emulator snapshot taken after the map is loaded.
(Layout cross-checked against work/ref/log2 legacy2_extended_view.cpp.)
"""
import struct
from PIL import Image
from gba import wf_decompress, bgr555

# per game: (layer object vtable, offset of resource ptr in layer object, resource vtable)
VT = {'log2': (0x08023868, 0x10, 0x080056A1), 'buu': (0x08049410, 0x08, 0x08008087)}


def find_layers(e, game, R):
    lay_vt, roff, res_vt = VT[game]
    ew = e.ewram()
    found = []
    for off in range(0, 0x40000 - 0x60, 4):
        if struct.unpack_from('<I', ew, off + (8 if game == 'log2' else 0))[0] != lay_vt:
            continue
        res = struct.unpack_from('<I', ew, off + roff)[0]
        if not R.isptr(res) or R.u32(res) != res_vt:
            continue
        cols, rows = R.u8(res + 0x14), R.u8(res + 0x15)
        if cols and rows and cols * rows <= 0xFF:
            found.append((res, 0x02000000 + off))
    if game == 'buu':
        # layer object +4 = BG index (resource +4/+8 = scroll rate, 0x400 = 1x)
        return {struct.unpack_from('<I', ew, o - 0x02000000 + 4)[0] & 3: (r, o) for r, o in found}
    # LoG2: the four layer objects are allocated BG3 first (verified visually)
    return {3 - i: ro for i, ro in enumerate(found[:4])}


def decode_layer(R, res):
    cols, rows = R.u8(res + 0x14), R.u8(res + 0x15)
    ents = [[0] * (cols * 32) for _ in range(rows * 32)]
    for cy in range(rows):
        for cx in range(cols):
            p = R.u32(res + 0x18 + 4 * (cy * cols + cx))
            if not R.isptr(p):
                continue
            d, _ = wf_decompress(R.d, p & 0x1FFFFFF)
            for ty in range(32):
                for tx in range(32):
                    ents[cy * 32 + ty][cx * 32 + tx] = struct.unpack_from('<H', d, (ty * 32 + tx) * 2)[0]
    return cols, rows, ents


def tile_cache(vram, charbase, bpp, pal):
    cache = {}

    def get(tid, palbank):
        k = (tid, palbank)
        if k in cache:
            return cache[k]
        im = Image.new('RGBA', (8, 8), (0, 0, 0, 0))
        px = im.load()
        if bpp == 8:
            base = charbase + tid * 64
            data = vram[base:base + 64]
            for i, c in enumerate(data):
                if c:
                    px[i & 7, i >> 3] = pal[c] + (255,)
        else:
            base = charbase + tid * 32
            data = vram[base:base + 32]
            for i in range(64):
                b = data[i >> 1] if (i >> 1) < len(data) else 0
                c = (b >> 4) if i & 1 else (b & 15)
                if c:
                    px[i & 7, i >> 3] = pal[palbank * 16 + c] + (255,)
        cache[k] = im
        return im
    return get


def render_entries(ents, get):
    H = len(ents)
    W = len(ents[0])
    img = Image.new('RGBA', (W * 8, H * 8), (0, 0, 0, 0))
    for y in range(H):
        row = ents[y]
        for x in range(W):
            se = row[x]
            t = get(se & 0x3FF, se >> 12)
            if se & 0x400:
                t = t.transpose(Image.FLIP_LEFT_RIGHT)
            if se & 0x800:
                t = t.transpose(Image.FLIP_TOP_BOTTOM)
            img.alpha_composite(t, (x * 8, y * 8))
    return img


def locate_view(ents, ring, hofs, vofs):
    """Find the layer tile (tx,ty) shown at the top-left of the screen by matching the hardware 32x32
    screen-entry ring against the decoded layer. Returns world pixel (x, y) of screen pixel (0,0)."""
    H, W = len(ents), len(ents[0])
    rx, ry = hofs >> 3, vofs >> 3
    win = [[ring[((ry + j) & 31) * 32 + ((rx + i) & 31)] for i in range(30)] for j in range(20)]
    best = None
    for ty in range(-3, H):
        for tx in range(-3, W):
            score = 0
            tot = 0
            for j in range(0, 20, 3):
                yy = ty + j
                for i in range(0, 30, 3):
                    xx = tx + i
                    v = ents[yy][xx] if 0 <= yy < H and 0 <= xx < W else 0
                    w = win[j][i]
                    tot += 10 if w else 1
                    if v == w:
                        score += 10 if w else 1
            if best is None or score > best[0]:
                best = (score, tx, ty, tot)
    score, tx, ty, tot = best
    return tx * 8 + (hofs & 7), ty * 8 + (vofs & 7), score / tot


def snapshot_render(e, game, R):
    """Render every field layer of the currently loaded map; layers are aligned so that they line up the
    way they appear on screen in the snapshot. Returns (layers, composite)."""
    io = e.read(0x04000000, 0x60)
    vram = e.vram()
    pr = e.palram()
    pal = [bgr555(struct.unpack_from('<H', pr, i * 2)[0]) for i in range(256)]
    layers = find_layers(e, game, R)
    out = {}
    for layer, (res, obj) in sorted(layers.items()):
        cnt = struct.unpack_from('<H', io, 8 + 2 * layer)[0]
        charbase = ((cnt >> 2) & 3) * 0x4000
        sbb = ((cnt >> 8) & 31) * 0x800
        bpp = 8 if cnt & 0x80 else 4
        hofs = struct.unpack_from('<H', io, 0x10 + 4 * layer)[0] & 0x1FF
        vofs = struct.unpack_from('<H', io, 0x12 + 4 * layer)[0] & 0x1FF
        cols, rows, ents = decode_layer(R, res)
        if game == 'log2':
            # the engine keeps the full authored scroll in BGxHOFS/VOFS (high bits select the chunk)
            vx = struct.unpack_from('<H', io, 0x10 + 4 * layer)[0]
            vy = struct.unpack_from('<H', io, 0x12 + 4 * layer)[0]
        else:
            # Buu: layer object +0x24/+0x28 = floor((scroll - (8,48)) / 256)
            cx = e.r32(obj + 0x24)
            cy = e.r32(obj + 0x28)
            cx -= (1 << 32) if cx & 0x80000000 else 0
            cy -= (1 << 32) if cy & 0x80000000 else 0
            hx8, vy8 = hofs & 0xFF, vofs & 0xFF
            vx = cx * 256 + hx8 + (256 if hx8 < 8 else 0)
            vy = cy * 256 + vy8 + (256 if vy8 < 48 else 0)
        conf = 1.0
        img = render_entries(ents, tile_cache(vram, charbase, bpp, pal))
        rate = (R.u32(res + 4), R.u32(res + 8)) if game == 'buu' else (0x400, 0x400)
        out[layer] = (img, {'resource': res, 'priority': cnt & 3, 'bpp': bpp, 'charbase': charbase,
                            'chunks': [cols, rows], 'view_origin': [vx, vy], 'match': round(conf, 2),
                            'scroll_rate': [rate[0] / 1024, rate[1] / 1024]})
    comp = None
    if out:
        # reference = layer with the best ring match among 1x-scrolling layers
        ref = max(out.items(), key=lambda kv: (kv[1][1]['scroll_rate'] == [1.0, 1.0], kv[1][1]['match']))
        rvx, rvy = ref[1][1]['view_origin']
        offs = {k: (rvx - v[1]['view_origin'][0], rvy - v[1]['view_origin'][1]) for k, v in out.items()}
        x0 = min(min(o[0] for o in offs.values()), 0)
        y0 = min(min(o[1] for o in offs.values()), 0)
        W = max(offs[k][0] + v[0].width for k, v in out.items()) - x0
        H = max(offs[k][1] + v[0].height for k, v in out.items()) - y0
        comp = Image.new('RGBA', (W, H), pal[0] + (255,))
        # GBA: lower priority value is in front; equal priority -> lower BG index in front
        for layer, (img, info) in sorted(out.items(), key=lambda kv: (-kv[1][1]['priority'], -kv[0])):
            comp.alpha_composite(img, (offs[layer][0] - x0, offs[layer][1] - y0))
            info['offset_in_composite'] = [offs[layer][0] - x0, offs[layer][1] - y0]
    return out, comp
