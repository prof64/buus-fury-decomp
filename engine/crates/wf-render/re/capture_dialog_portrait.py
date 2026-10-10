"""Dialogue boxes with a speaker portrait (headless, muted) -> work/frontend/dialog_portrait/.

Scenarios: the new-game intro from buu_menus.ss (narration boxes without portrait; the King Kai / Goku cutscene:
portrait right, bottom; player-slot arg 2 and arg 87), and map-NPC talks in Yemma's Office / Grand Kai's Planet (rec_npc's warp) from below (box top, portrait
left), above (bottom, right), the left (top, right) and a Yes / No choice (option lines). For every page that settles
(fully typed, box at full size, the choice cursor blinked off) and for one frame of each box's opening zoom, the game
state is cloned, BG VRAM cleared (every BG pixel transparent -> backdrop) and one frame run: `<name>_<k>.png` is then
the box OBJs (+ other OBJs) over the backdrop colour. `index.json` lists each capture: the page text from RAM (dialog
object +0x10 .. terminator, prefix / `^` already consumed), the node arg, the box's centre y (+0x60), portrait OBJ x,
centred flag (+0x28), affine PA, backdrop colour. Used by `cargo test -p wf-app --test dialog_portrait_parity`.

  .venv/bin/python engine/crates/wf-render/re/capture_dialog_portrait.py
"""
import json
import struct
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[4]
sys.path.insert(0, str(ROOT / 'tools' / 'field'))
sys.path.insert(0, str(ROOT / 're' / 'buu'))
from common import emu, KEY  # noqa: E402
from PIL import Image  # noqa: E402

OUT = ROOT / 'work' / 'frontend' / 'dialog_portrait'
OUT.mkdir(parents=True, exist_ok=True)
ENTRY = 0x08012012     # r2 = the text node's arg
TALK = 0x08017D44      # a map NPC's talk scene: r2 = player.x - npc.x, r0 = player.y - npc.y (24.8)
SPEAKER = 0x08012148   # 0x08012012 after the prefix: r2 = text, r6 = arg
CTOR_RET = 0x08012156  # r0 = dialog object (0x0800B188)


def s32(v):
    return v - (1 << 32) if v >= 1 << 31 else v


def text_at(e, p, limit=2048):
    out = []
    for k in range(limit):
        c = e.r16(p + 2 * k)
        if c == 0:
            break
        out.append(chr(c))
    return ''.join(out)


SIZES = {(0, 0): (8, 8), (0, 1): (16, 16), (0, 2): (32, 32), (0, 3): (64, 64), (1, 0): (16, 8), (1, 1): (32, 8),
         (1, 2): (32, 16), (1, 3): (64, 32), (2, 0): (8, 16), (2, 1): (8, 32), (2, 2): (16, 32), (2, 3): (32, 64)}


def dialog_objs(e, n=8):
    """visible OAM entries 0..n-1 (the dialog builds its OBJs first)"""
    o = e.oam()
    ents = []
    for i in range(n):
        a0, a1, a2 = struct.unpack_from('<HHH', o, i * 8)
        if (a0 >> 8) & 3 == 2:
            continue
        x = a1 & 0x1FF
        x = x - 512 if x >= 256 else x
        y = a0 & 0xFF
        aff = (a0 >> 8) & 1
        pa = struct.unpack_from('<h', o, ((a1 >> 9) & 31) * 32 + 6)[0] if aff else 256
        w, h = SIZES.get((a0 >> 14, a1 >> 14), (0, 0))
        dbl = (a0 >> 9) & 1 if aff else 0
        ents.append({'i': i, 'x': x, 'y': y, 'w': w << dbl, 'h': h << dbl, 'shape': a0 >> 14, 'size': a1 >> 14,
                     'prio': (a2 >> 10) & 3, 'pa': pa, 'aff': aff})
    return ents


class Run:
    def __init__(self, e, name):
        self.e, self.name = e, name
        e.set_volume(0)
        e.breakpoint(SPEAKER)
        e.breakpoint(ENTRY)
        e.breakpoint(TALK)
        self.anchor = [-1, -1]
        self.arg = None
        self.prefix = None
        e.breakpoint(CTOR_RET)
        self.cur = None      # {obj, arg, text_ptr}
        self.pending = None
        self.prev = None
        self.same = 0
        self.done = set()
        self.zoomed = set()
        self.caps = []
        self.serial = 0

    def frame(self):
        e = self.e
        e.run(1)
        for h in e.hits():
            r = h['regs']
            if h['addr'] == ENTRY:
                self.arg = r[2]
                self.prefix = chr(e.r16(r[1]))
            elif h['addr'] == TALK:
                self.anchor = [s32(r[2]), s32(r[0])]
            elif h['addr'] == SPEAKER:
                self.pending = {'global': r[6], 'text_ptr': r[2], 'arg': self.arg, 'anchor': self.anchor,
                                'prefix': self.prefix if self.prefix in '!@#' else None}
            elif self.pending:
                self.serial += 1
                self.cur = dict(self.pending, obj=r[0], serial=self.serial)
                self.pending = None
        if not self.cur:
            return
        o = self.cur['obj']
        if e.r32(o) != 0x08049F60:   # gone (vtable)
            return
        portrait = e.r32(o + 0x24) == 1
        # the page is typed out (+0x20 set, +0x14 at the page end +0x18: 0x0800AE74 waits for A)
        typed = e.r32(o + 0x20) != 0 and e.r32(o + 0x14) == e.r32(o + 0x18)
        self.same = self.same + 1 if typed else 0
        objs = dialog_objs(e)
        box = [b for b in objs if b['prio'] == 0 and b['aff'] and b['size'] == 3]
        if len(box) != (4 if portrait else 3):
            return
        pa = box[0]['pa']
        cursor = any(b['prio'] == 0 and b['shape'] == 0 and b['size'] == 1 for b in objs)  # the choice arrow (16x16)
        key = (self.cur['serial'], e.r32(o + 0x10))
        if pa == 682 and self.cur['serial'] not in self.zoomed:
            self.zoomed.add(self.cur['serial'])
            self.capture(objs, pa)
        elif pa == 256 and self.same >= 3 and not cursor and key not in self.done:
            self.done.add(key)
            self.capture(objs, pa)

    def capture(self, objs, pa):
        e = self.e
        o = self.cur['obj']
        page = text_at(e, e.r32(o + 0x10)) if pa == 256 else None
        info = {'portrait': e.r32(o + 0x24) == 1, 'prefix': self.cur['prefix'], 'arg': self.cur['arg'], 'anchor': self.cur['anchor'], 'global': self.cur['global'], 'page_text': page, 'full_text': text_at(e, self.cur['text_ptr']),
                'centre': [s32(e.r32(o + 0x5C)), s32(e.r32(o + 0x60))], 'centred': e.r32(o + 0x28),
                'obj_x': [s32(e.r32(o + k)) for k in (0x68, 0x6C, 0x70, 0x74)], 'pa': pa, 'oam': objs,
                'style': (e.r32(o + 0x4C) - 0x08071730) // 20}
        st = e.save()
        info['backdrop'] = e.r16(0x05000000)
        Image.frombytes('RGB', (240, 160), e.pixels()).save(OUT / f'{self.name}_{len(self.caps):02d}_screen.png')
        e.L.emu_clear_hits(e.h)
        # clear BG VRAM (tiles + maps): every BG pixel transparent, the screen = OBJs over the backdrop
        for a in range(0x06000000, 0x06010000, 4):
            e.w32(a, 0)
        e.run(1)
        e.hits()
        # the frame just run shows the OAM built in it (copied at its VBlank; measured: the zoom scale matches)
        info['oam_shown'] = dialog_objs(e, 128)
        info['pa'] = info['oam_shown'][0]['pa']
        f = f'{self.name}_{len(self.caps):02d}.png'
        Image.frombytes('RGB', (240, 160), e.pixels()).save(OUT / f)
        info['png'] = f
        info['name'] = f'{self.name}_{len(self.caps):02d}'
        e.load(st)
        self.caps.append(info)
        print(info['name'], 'global', info['global'], 'centre', info['centre'], 'pa', pa, repr((page or info['full_text'])[:50]))

    def frames(self, n, keys=0):
        self.e.set_keys(keys)
        for _ in range(n):
            self.frame()


def intro():
    r = Run(emu('buu_menus.ss'), 'intro')
    for _ in range(12):
        r.frames(2, KEY.A)
        r.frames(60)
    for _ in range(48):
        r.frames(2, KEY.A)
        r.frames(90)
    return r.caps


def talk(name, area, pos, inp):
    import rec_npc
    e, _, _, _ = rec_npc.goto_f0({'area': area, 'pos': pos})
    r = Run(e, name)
    for keys, n in inp:
        r.frames(n, keys)
    return r.caps


def main():
    caps = []
    caps += talk('yemma_below', (1, 3), (200, 432), [[0, 40], [KEY.UP, 40], [0, 10], [KEY.A, 2], [0, 150], [KEY.A, 2], [0, 90]])
    caps += talk('yemma_above', (1, 3), (176, 372), [[0, 40], [KEY.RIGHT, 14], [KEY.DOWN, 12], [0, 10], [KEY.A, 2], [0, 150]])
    caps += talk('yemma_left', (1, 3), (176, 372), [[0, 40], [KEY.DOWN, 30], [KEY.RIGHT, 4], [0, 10], [KEY.A, 2], [0, 150]])
    caps += talk('choice', (1, 11), (236, 150), [[0, 40], [KEY.UP, 4], [0, 10], [KEY.A, 2], [0, 120]])
    caps += intro()
    (OUT / 'index.json').write_text(json.dumps(caps, indent=1))
    print(len(caps), 'captures ->', OUT.relative_to(ROOT))


if __name__ == '__main__':
    main()
