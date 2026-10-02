"""Effects/projectiles (code-driven raw OBJ frames) and dialogue portraits for LoG2 / Buu."""
from gba import palette, wf_decompress, Rom
from wf_sprites import WFSprites
from wf_effects import scan, raw_gfx
from render import obj_image, compose
from export import EntityBuilder, hx
from PIL import Image
import names

PAL = {'log2': 0x81DA6C8, 'buu': 0x805652C}
PORTRAITS = {'log2': (0x83EC9F4, 107), 'buu': (0x86F395C, 155)}
# player-character portraits shown on the menu BG layer use the menu BG palette (0x081D4F50 in LoG2)
PORTRAIT_ALT_PAL = {'log2': (range(0, 18), 0x81D4F50)}


def single_entity(game, slug, name, category, frames, subdir, extra=None, conf='low', durations=4, loop=True,
                  anim_name='default'):
    """frames: list of (img, ox, oy, rom). Builds an entity with one animation over all frames."""
    B = EntityBuilder(game, slug, subdir)
    seq = []
    for im, ox, oy, rom in frames:
        fi, flip = B.add(im, ox, oy, rom)
        seq.append({'frame': fi, 'duration_frames': durations, 'flip': flip, 'rom': rom})
    sheet, recs = B.write()
    e = {'id': slug, 'name': name, 'category': category, 'name_confidence': conf, 'sheet': sheet, 'frames': recs,
         'animations': {anim_name: {'raw_index': 0, 'loop': loop, 'timing_source': 'default',
                                    'directions': {'any': seq}}}}
    if extra:
        e.update(extra)
    return e


def effects(game, defs):
    S = WFSprites(game)
    R = S.R
    pal = palette(R.bytes(PAL[game], 512), 256)
    known = set()
    for d in defs:
        if d:
            for a in d['anims']:
                if a:
                    for fl in a['dirs']:
                        known.update(fl)
    N = names.load(game)
    fx_names = N.d.get('effects', {})
    out = []
    for ri, run in enumerate(scan(game, known)):
        key = f'{run[0]["addr"]:08X}'
        info = fx_names.get(key, {})
        name = info.get('name', f'Effect {key}')
        slug = info.get('slug') or f'fx_{key.lower()}' + ('_' + names.slugify(info['name']) if 'name' in info else '')
        frames = []
        for r in run:
            im = obj_image(raw_gfx(R, r), r['W'], r['H'], r['bpp'], pal)
            o = {'dx': -r['ox'], 'dy': -r['oy'], 'W': r['W'], 'H': r['H'], 'hflip': r['hflip'], 'vflip': r['vflip']}
            im, ox, oy = compose([o], [im])
            frames.append((im, ox, oy, hx(r['addr'])))
        if 'name' not in info:
            name = f'Effect {key} ({_colour_word(frames)}, {run[0]["W"]}x{run[0]["H"]}, {len(run)} frames)'
        e = single_entity(game, slug, name, info.get('category', 'effect'), frames, 'effects',
                          {'rom': {'frame_records': hx(run[0]['addr']), 'count': len(run),
                                   'gfx_format': 'raw (uncompressed) 8bpp tiles, shared OBJ palette',
                                   'palette': hx(PAL[game])},
                           'notes': info.get('notes', 'frames listed in ROM order; grouping into per-direction '
                                                      'animations is done by game code, durations are a default')},
                          conf=info.get('confidence', 'low'))
        out.append(e)
    return out


def _colour_word(frames):
    """Rough dominant-colour description used to label uncurated effects."""
    import colorsys
    tot = [0, 0, 0]
    n = 0
    for im, *_ in frames:
        for r, g, b, a in im.getdata():
            if a:
                tot[0] += r
                tot[1] += g
                tot[2] += b
                n += 1
    if not n:
        return 'empty'
    r, g, b = (t / n / 255 for t in tot)
    h, l, s = colorsys.rgb_to_hls(r, g, b)
    if s < 0.18:
        return 'white' if l > 0.75 else 'grey' if l > 0.35 else 'dark'
    hue = h * 360
    for lim, word in ((15, 'red'), (45, 'orange'), (70, 'yellow'), (160, 'green'), (200, 'cyan'), (255, 'blue'),
                      (290, 'purple'), (335, 'pink'), (361, 'red')):
        if hue < lim:
            return word
    return 'mixed'


def portraits(game):
    R = Rom(game)
    pal = palette(R.bytes(PAL[game], 512), 256)
    alt_idx, alt_ad = PORTRAIT_ALT_PAL.get(game, ((), None))
    alt_pal = palette(R.bytes(alt_ad, 512), 256) if alt_ad else None
    tbl, n = PORTRAITS[game]
    N = names.load(game)
    pn = N.d.get('portraits', {})
    out = []
    for i in range(n):
        p = R.u32(tbl + 4 * i)
        d, _ = wf_decompress(R.d, p & 0x1FFFFFF)
        im = Image.new('RGBA', (64, 64))
        pp = alt_pal if i in alt_idx else pal
        im.putdata([pp[c] + (255,) for c in d[:4096]])
        info = pn.get(str(i), {})
        name = info.get('name', f'Portrait {i}')
        slug = f'portrait_{i:03d}' + ('_' + names.slugify(info['name']) if 'name' in info else '')
        out.append(single_entity(game, slug, name, 'portrait', [(im, 0, 0, hx(p))], 'portraits',
                                 {'portrait_index': i,
                                  'rom': {'table': hx(tbl + 4 * i), 'gfx': hx(p),
                                          'palette': hx(alt_ad if i in alt_idx else PAL[game]),
                                          'gfx_format': 'webfoot-lz, 64x64 linear 8bpp bitmap (256-colour palette)'}},
                                 conf=info.get('confidence', 'low')))
    return out


# Buu's Fury ground shadows: table 0x086B759C, 4 x {u8 w, u8 h, u8, u8, u32 gfx, u32 vram slot, u16 attr0, u16 attr1},
# looked up by the sprite header shadow bytes (+0x0A, +0x0B) in 0x08013E38. The OBJ is drawn at the entity position
# (top-left of the collision footprint), behind the body. Raw tiles, colour 1 of the OBJ palette (black).
BUU_SHADOWS = 0x86B759C
# Halo OBJ (16x8 raw 8bpp tiles) drawn for the halo forms of Goku (defs 108/110/112, see wf_sprites attach table).
BUU_HALO = 0x80725F0


def buu_overlays():
    import struct
    from wf_sprites import OBJ_SIZE
    R = Rom('buu')
    pal = palette(R.bytes(PAL['buu'], 512), 256)
    out = []
    for i in range(4):
        a = BUU_SHADOWS + 16 * i
        w, h = R.bytes(a, 2)
        g = R.u32(a + 4)
        a0, a1 = struct.unpack('<HH', R.bytes(a + 12, 4))
        W, H = OBJ_SIZE[(a0 >> 14, a1 >> 14)]
        bpp = 8 if a0 & 0x2000 else 4
        im = obj_image(R.bytes(g, W * H * bpp // 8), W, H, bpp, pal)
        out.append(single_entity('buu', f'buu_shadow_{w}x{h}', f'Ground shadow {w}x{h}', 'effect', [(im, 0, 0, hx(g))],
                                 'effects', {'rom': {'table': hx(a), 'gfx': hx(g), 'obj': f'{W}x{H} {bpp}bpp'},
                                             'shadow_size': [w, h],
                                             'notes': 'drawn at the entity position (anchor) for sprites whose header '
                                                      'shadow bytes [2], [3] are this size'}, conf='high'))
    im = obj_image(R.bytes(BUU_HALO, 128), 16, 8, 8, pal)
    out.append(single_entity('buu', 'buu_halo', 'Halo', 'effect', [(im, 0, 0, hx(BUU_HALO))], 'effects',
                             {'rom': {'gfx': hx(BUU_HALO), 'obj': '16x8 8bpp'},
                              'notes': 'drawn at entity position + the step attach offset of halo sprites'}, conf='high'))
    return out
