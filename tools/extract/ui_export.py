"""UI captures (HUD, menus, title) rendered from emulator snapshots reached from the ROM alone.

The HUD/menu art is assembled at runtime from many small pieces (OBJ + BG tiles), so instead of guessing
layouts this renders what the game itself put on screen:
  * hud.png      - the HUD OBJs only (OAM entries whose top edge is in the top 40 px), transparent background
  * screen.png   - full screenshot of the state
  * obj_tiles.png- every OBJ tile currently in VRAM with the palette each OAM entry uses (reference sheet)
"""
import struct
from PIL import Image
from gba import ASSETS, bgr555
from wf_sprites import OBJ_SIZE
from render import obj_image
from export import write_json  # noqa: F401


def _pal(pr, base):
    return [bgr555(struct.unpack_from('<H', pr, base + i * 2)[0]) for i in range(256)]


def render_oam(e, select=lambda a0, a1, x, y, w, h: True):
    oam, vram, pr = e.oam(), e.vram(), e.palram()
    pal = _pal(pr, 0x200)
    dispcnt = e.r16(0x04000000)
    one_d = bool(dispcnt & 0x40)
    canvas = Image.new('RGBA', (240, 160), (0, 0, 0, 0))
    used = 0
    for i in reversed(range(128)):
        a0, a1, a2 = struct.unpack_from('<HHH', oam, i * 8)
        if (a0 >> 8) & 3 == 2 or (a0 >> 14) == 3:
            continue
        W, H = OBJ_SIZE[(a0 >> 14, a1 >> 14)]
        y = a0 & 0xFF
        x = a1 & 0x1FF
        if y >= 160:
            y -= 256
        if x >= 240:
            x -= 512
        if not select(a0, a1, x, y, W, H):
            continue
        bpp = 8 if a0 & 0x2000 else 4
        tile = a2 & 0x3FF
        tsz = 64 if bpp == 8 else 32
        tw = W // 8
        data = bytearray()
        for ty in range(H // 8):
            for tx in range(tw):
                if one_d:
                    t = tile + (ty * tw + tx) * (2 if bpp == 8 else 1)
                else:
                    t = tile + ty * 32 + tx * (2 if bpp == 8 else 1)
                off = 0x10000 + (t & 0x3FF) * 32
                data += vram[off:off + tsz]
        im = obj_image(bytes(data), W, H, bpp, pal, palbank=a2 >> 12)
        if a1 & 0x1000 and not a0 & 0x100:
            im = im.transpose(Image.FLIP_LEFT_RIGHT)
        if a1 & 0x2000 and not a0 & 0x100:
            im = im.transpose(Image.FLIP_TOP_BOTTOM)
        canvas.alpha_composite(im, (max(0, x), max(0, y)) if x >= 0 and y >= 0 else (0, 0))
        used += 1
    return canvas, used


def _states(game):
    from emu_states import KEY
    if game == 'log2':
        from emu_states import log2_warp
        e = log2_warp(60)
        e.run(120)
        yield 'gameplay', e
        s = e.save()
        e.press(KEY.START, 3, 60)
        yield 'menu_status', e
        e.press(KEY.RIGHT, 3, 40)
        yield 'menu_page2', e
        e.load(s)
    elif game == 'buu':
        from emu_states import buu_snakeway
        e = buu_snakeway()
        yield 'gameplay', e
        s = e.save()
        e.press(KEY.START, 3, 60)
        yield 'menu', e
        e.load(s)
        e.press(KEY.SELECT, 3, 60)
        yield 'map_screen', e
    else:
        from emu_states import log1_warp
        e = log1_warp(2)
        e.run(60)
        yield 'gameplay', e
        e.press(KEY.START, 3, 60)
        yield 'menu', e


def export(game):
    out = ASSETS / game / 'ui'
    out.mkdir(parents=True, exist_ok=True)
    caps = []
    for name, e in _states(game):
        d = out / name
        d.mkdir(exist_ok=True)
        e.screenshot(d / 'screen.png', scale=1)
        hud, n = render_oam(e, lambda a0, a1, x, y, w, h: y < 40)
        bb = hud.getbbox()
        rec = {'name': name, 'screen': f'assets/{game}/ui/{name}/screen.png'}
        if bb:
            hud.crop(bb).save(d / 'hud.png')
            rec['hud'] = f'assets/{game}/ui/{name}/hud.png'
            rec['hud_origin'] = [bb[0], bb[1]]
        allobj, _ = render_oam(e)
        if allobj.getbbox():
            allobj.save(d / 'obj_layer.png')
            rec['obj_layer'] = f'assets/{game}/ui/{name}/obj_layer.png'
        caps.append(rec)
    return {'ui_captures': caps,
            'ui_note': 'runtime captures (screen, HUD OBJs, full OBJ layer) - see tools/extract/ui_export.py'}
