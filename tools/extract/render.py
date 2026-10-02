"""Rendering helpers: OBJ tiles -> RGBA images, frame composition, sheet packing."""
from PIL import Image
from gba import palette


def obj_image(data, W, H, bpp, pal, remap=None, palbank=0):
    """Decode 1D-mapped OBJ tile data into an RGBA image of W x H."""
    im = Image.new('RGBA', (W, H), (0, 0, 0, 0))
    px = im.load()
    tw = W // 8
    tsz = 64 if bpp == 8 else 32
    ntiles = (W // 8) * (H // 8)
    for t in range(min(ntiles, len(data) // tsz)):
        ox, oy = (t % tw) * 8, (t // tw) * 8
        base = t * tsz
        for i in range(64):
            if bpp == 8:
                c = data[base + i]
                if remap:
                    c = remap[c]
            else:
                b = data[base + (i >> 1)]
                c = (b >> 4) if i & 1 else (b & 15)
                if c:
                    c += palbank * 16
            if c & 15 if bpp == 4 else c:
                px[ox + (i & 7), oy + (i >> 3)] = pal[c] + (255,)
    return im


def compose(objs, images):
    """objs: list of {dx,dy,W,H,hflip,vflip}; images aligned. Returns (img, ox, oy) where (ox,oy) is
    the position of the entity origin inside img."""
    if not objs:
        return Image.new('RGBA', (1, 1)), 0, 0
    x0 = min(o['dx'] for o in objs)
    y0 = min(o['dy'] for o in objs)
    x1 = max(o['dx'] + o['W'] for o in objs)
    y1 = max(o['dy'] + o['H'] for o in objs)
    out = Image.new('RGBA', (x1 - x0, y1 - y0), (0, 0, 0, 0))
    # OAM priority: earlier OBJ drawn on top -> paste in reverse order
    for o, im in reversed(list(zip(objs, images))):
        if o.get('hflip'):
            im = im.transpose(Image.FLIP_LEFT_RIGHT)
        if o.get('vflip'):
            im = im.transpose(Image.FLIP_TOP_BOTTOM)
        out.alpha_composite(im, (o['dx'] - x0, o['dy'] - y0))
    return out, -x0, -y0


def trim(im, ox, oy):
    bb = im.getbbox()
    if not bb:
        return Image.new('RGBA', (1, 1)), ox, oy
    return im.crop(bb), ox - bb[0], oy - bb[1]


def pack(images, max_w=512, pad=1):
    """Simple shelf packer. images: list of PIL images. Returns (sheet, [(x,y)])."""
    order = sorted(range(len(images)), key=lambda i: -images[i].height)
    pos = [None] * len(images)
    x = y = shelf = 0
    width = max([max_w] + [im.width + pad for im in images])
    for i in order:
        im = images[i]
        if x + im.width > width:
            x = 0
            y += shelf + pad
            shelf = 0
        pos[i] = (x, y)
        x += im.width + pad
        shelf = max(shelf, im.height)
    H = y + shelf
    W = max((p[0] + images[i].width for i, p in enumerate(pos)), default=1)
    sheet = Image.new('RGBA', (max(W, 1), max(H, 1)), (0, 0, 0, 0))
    for i, im in enumerate(images):
        sheet.paste(im, pos[i])
    return sheet, pos
