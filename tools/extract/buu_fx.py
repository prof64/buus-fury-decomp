"""Buu's Fury beam graphics: one entity per beam level definition (beam object ctor 0x080206B8, vtable 0x0804AEF8).

Level definition (16 bytes, e.g. Kamehameha 0x0871DA68 + 16*level): u8 step_v, u8 step_h (body segment step for
down/up and left/right: obj+0x12 = def[dir>>1]), u16 mult, u8 flags, u8 mode, u16 attack sub, u32 charge record
(state 0 sparkle, a record of the run 0x08721164), u32 piece table.

Piece table: 4 directions x 16 records (0xC0 bytes per direction, down/up/left/right). Indices: 0 start, 2 start
mirror (flag bit0), 3/4/5 body 8/16/32 px, 6 gfx-only record (second body gfx), 7 head, 9 head mirror (bit2),
10 impact (head stopped by a hit or a wall), 12 impact mirror (bit3), 13 tail (retracting), 15 tail mirror (bit1).

Drawing (0x08020DAC, pieces via 0x08020CF4): a piece's OBJ top-left = beam position + offset + (ox, oy) - the
record origin is ADDED here (the base projectile draw subtracts it). The OBJ shape comes from the piece record, the
tiles from one of three per-beam VRAM slots (start/tail, body, head/impact) that the update (0x0802084C) refills
every 4 ticks (phase = t >> 2):
  mode 0/1 (k = phase & 1): start slot <- record k (firing) or 13 + k (retracting); head slot <- record 7 + k, or
      10 + k once stopped; body slot <- mode 0: record 5 gfx (k = 0) or the gfx-only record 6 (k = 1); mode 1:
      record 5 gfx + 64 * (phase & 3) (the body scrolls one tile per phase).
  mode 2 (k = phase & 3, 3 -> 1): start slot <- record 0 gfx + 64k; head slot <- record 7 gfx + 64k, or once
      stopped record 10 + (k & 1); body slot <- record 5 gfx + 256k.
Each exported animation is one piece; its steps (one per direction) are the slot variants in that order, so the
renderer picks `steps[variant]`. Images are anchored on the piece position (anchor = -(ox, oy)).
"""
import struct
from gba import palette, Rom
from render import obj_image, compose
from export import EntityBuilder, hx, DIRS
from wf_sprites import OBJ_SIZE

PAL = 0x805652C

# slug, label, level definition
BEAMS = [
    ('kamehameha_1', 'Kamehameha Lv1', 0x871DA68),
    ('kamehameha_2', 'Kamehameha Lv2', 0x871DA78),
    ('kamehameha_3', 'Kamehameha Lv3', 0x871DA88),
    ('electric_kamehameha_1', 'Electric Kamehameha Lv1', 0x8725018),
    ('electric_kamehameha_2', 'Electric Kamehameha Lv2', 0x8725028),
    ('electric_kamehameha_3', 'Electric Kamehameha Lv3', 0x8725038),
    ('kamekameha_1', 'Kamekameha Lv1', 0x87315A8),
    ('kamekameha_2', 'Kamekameha Lv2', 0x87315B8),
    ('kamekameha_3', 'Kamekameha Lv3', 0x87315C8),
    ('banshee_blast', 'Banshee Blast', 0x876BA2C),
    ('big_bang_kamehameha', 'Big Bang Kamehameha', 0x87710FC),
    ('enemy_beam_red', 'Enemy beam (red)', 0x877C41C),
]


def rec(R, a):
    ox, oy, w, h, a0, a1, gp = struct.unpack('<bbBBHHI', R.bytes(a, 12))
    sh, sz = a0 >> 14, a1 >> 14
    W, H = OBJ_SIZE.get((sh, sz), (8, 8)) if (a0 or a1) else (0, 0)
    return {'ox': ox, 'oy': oy, 'W': W, 'H': H, 'gfx': gp, 'hflip': bool(a1 & 0x1000), 'vflip': bool(a1 & 0x2000),
            'addr': a}


# runs holding the state-0 charge sparkle records (fx_<addr> entities of misc_export.effects)
CHARGE_RUNS = (0x8721164, 0x877B4F8)


def _charge(a):
    for r in CHARGE_RUNS:
        if r <= a < r + 0x100 and (a - r) % 12 == 0:
            return {'charge_fx': f'fx_{r:08x}', 'charge_index': (a - r) // 12}
    return {'charge_fx': None, 'charge_index': None}


def beams():
    R = Rom('buu')
    pal = palette(R.bytes(PAL, 512), 256)
    out = []
    for slug, label, d in BEAMS:
        step_v, step_h, mult, flags, mode, sub = struct.unpack('<BBHBBH', R.bytes(d, 8))
        charge = R.u32(d + 8)
        table = R.u32(d + 12)
        B = EntityBuilder('buu', f'buu_beam_{slug}', 'effects')
        anims = {}

        def piece(name, shape_idx, gfx_list, raw):
            dirs = {}
            for di, dn in enumerate(DIRS):
                base = table + 0xC0 * di
                s = rec(R, base + 12 * shape_idx)
                if not s['W']:
                    return
                steps = []
                for gi in gfx_list(base):
                    if gi is None:
                        continue
                    im = obj_image(R.bytes(gi, s['W'] * s['H']), s['W'], s['H'], 8, pal)
                    o = {'dx': s['ox'], 'dy': s['oy'], 'W': s['W'], 'H': s['H'], 'hflip': s['hflip'], 'vflip': s['vflip']}
                    im, ax, ay = compose([o], [im])
                    fi, flip = B.add(im, ax, ay, hx(s['addr']))
                    steps.append({'frame': fi, 'duration_frames': 4, 'flip': flip, 'rom': hx(gi)})
                dirs[dn] = steps
            anims[name] = {'raw_index': raw, 'loop': True, 'timing_source': 'code 0x0802084C (variant = slot upload)',
                           'directions': dirs}

        g = lambda base, i: rec(R, base + 12 * i)['gfx']
        if mode == 2:
            start = lambda b: [g(b, 0) + 64 * k for k in range(3)]
            head = lambda b: [g(b, 7) + 64 * k for k in range(3)]
            impact = lambda b: [g(b, 10), g(b, 11)]
            body = lambda b: [g(b, 5) + 256 * k for k in range(3)]
            tail = start
        else:
            start = lambda b: [g(b, 0), g(b, 1)]
            head = lambda b: [g(b, 7), g(b, 8)]
            impact = lambda b: [g(b, 10), g(b, 11)]
            tail = lambda b: [g(b, 13), g(b, 14)]
            if mode == 1:
                body = lambda b: [g(b, 5) + 64 * p for p in range(4)]
            else:
                body = lambda b: [g(b, 5), g(b, 6)]
        piece('start', 0, start, 0)
        piece('head', 7, head, 7)
        piece('impact', 10, impact, 10)
        piece('tail', 13, tail, 13)
        for nm, idx in (('body32', 5), ('body16', 4), ('body8', 3)):
            # a body piece reads the body slot at its own gfx offset from record 5's gfx
            piece(nm, idx, lambda b, idx=idx: [x + (g(b, idx) - g(b, 5)) for x in body(b)], idx)
        if flags & 1:
            piece('start2', 2, start, 2)
        if flags & 2:
            piece('tail2', 15, tail, 15)
        if flags & 4:
            piece('head2', 9, head, 9)
        if flags & 8:
            piece('impact2', 12, impact, 12)
        sheet, recs = B.write()
        out.append({
            'id': f'buu_beam_{slug}', 'name': f'Beam: {label}', 'category': 'effect', 'name_confidence': 'high',
            'sheet': sheet, 'frames': recs, 'animations': anims,
            'rom': {'level_def': hx(d), 'piece_table': hx(table), 'charge_record': hx(charge),
                    'palette': hx(PAL), 'gfx_format': 'raw (uncompressed) 8bpp tiles, shared OBJ palette'},
            'beam': {'mode': mode, 'flags': flags, 'step': [step_v, step_h], 'mult': mult, 'attack_sub': sub,
                     **_charge(charge)},
            'notes': 'beam pieces (tools/extract/buu_fx.py): animation = piece, step index = VRAM slot variant; '
                     'anchor = piece position (record origin added)',
        })
    return out


# ---------------------------------------------------------------------------------------------------------------
# Record tables the run scanner (wf_effects.scan) misses: their w/h fields are the hit box, not the OBJ shape
# (e.g. 16x8 boxes on 8x16 OBJs). Exported as fx_<first record> runs, OBJ shape from the attributes, standard
# convention (anchor = record origin: OBJ top-left = position - (ox, oy)).
EXTRA_RUNS = [
    (0x87571F4, 12, 'Mummy bandage', 'Bandage (class 0x0802D79C): straight records 0-2 (down/up) / 3-5 '
     '(left/right, also the stuck pose 0x08757218), diagonal 6-11'),
    (0x872D6B8, 20, 'Flame', 'Flame / Pikkon barrage (class 0x080246B4): 5 records per direction (60 bytes), '
     'record = 12*(5*dir + (t >> 2)), shown while t >> 2 <= 4'),
]
# raw single OBJs (no record): bomb explosion debris (class 0x0800C454 / 0x0801C750), 8x8 8bpp
RAW_OBJS = [(0x8072500, 8, 8, 'buu_debris', 'Debris', 'bomb / breakable-object debris (8x8, top-left = position)')]


def extra_runs():
    from misc_export import single_entity
    R = Rom('buu')
    pal = palette(R.bytes(PAL, 512), 256)
    out = []
    for base, n, name, notes in EXTRA_RUNS:
        frames = []
        for i in range(n):
            s = rec(R, base + 12 * i)
            im = obj_image(R.bytes(s['gfx'], s['W'] * s['H']), s['W'], s['H'], 8, pal)
            o = {'dx': -s['ox'], 'dy': -s['oy'], 'W': s['W'], 'H': s['H'], 'hflip': s['hflip'], 'vflip': s['vflip']}
            im, ax, ay = compose([o], [im])
            frames.append((im, ax, ay, hx(base + 12 * i), [s['ox'], s['oy'], s['W'], s['H'], s['hflip'], s['vflip']]))
        out.append(single_entity('buu', f'fx_{base:08x}', name, 'effect', frames, 'effects',
                                 {'rom': {'frame_records': hx(base), 'count': n, 'palette': hx(PAL),
                                          'gfx_format': 'raw (uncompressed) 8bpp tiles, shared OBJ palette'},
                                  'notes': notes}, conf='high'))
    for gfx, W, H, slug, name, notes in RAW_OBJS:
        im = obj_image(R.bytes(gfx, W * H), W, H, 8, pal)
        out.append(single_entity('buu', slug, name, 'effect', [(im, 0, 0, hx(gfx))], 'effects',
                                 {'rom': {'gfx': hx(gfx), 'obj': f'{W}x{H} 8bpp'}, 'notes': notes}, conf='high'))
    return out


# Big Bang Kamehameha extra object (ctor 0x08038886, vtable 0x0804DD6C, draw 0x0803893E; pieces at position + origin):
# flare record 0x08770D0C + 60*dir + 12k (k = (t >> 2) & 3), the direction's 5th record (no gfx: the flare's tiles
# with its own flips) and the ball 0x08770CF4 + 12 * ((t >> 2) & 1)
BBK_FLARE = 0x8770D0C
BBK_BALL = 0x8770CF4


def bbk_flare():
    R = Rom('buu')
    pal = palette(R.bytes(PAL, 512), 256)
    B = EntityBuilder('buu', 'buu_bbk_flare', 'effects')
    anims = {}

    def add(s, gfx):
        im = obj_image(R.bytes(gfx, s['W'] * s['H']), s['W'], s['H'], 8, pal)
        o = {'dx': s['ox'], 'dy': s['oy'], 'W': s['W'], 'H': s['H'], 'hflip': s['hflip'], 'vflip': s['vflip']}
        im, ax, ay = compose([o], [im])
        fi, flip = B.add(im, ax, ay, hx(s['addr']))
        return {'frame': fi, 'duration_frames': 4, 'flip': flip, 'rom': hx(gfx)}

    flare, mirror = {}, {}
    for di, dn in enumerate(DIRS):
        base = BBK_FLARE + 60 * di
        recs = [rec(R, base + 12 * k) for k in range(4)]
        m = rec(R, base + 48)
        flare[dn] = [add(s, s['gfx']) for s in recs]
        mirror[dn] = [add(m, s['gfx']) for s in recs]
    ball = [add(rec(R, BBK_BALL + 12 * k), rec(R, BBK_BALL + 12 * k)['gfx']) for k in range(2)]
    anims['flare'] = {'raw_index': 0, 'loop': True, 'timing_source': 'code 0x080387DC', 'directions': flare}
    anims['mirror'] = {'raw_index': 4, 'loop': True, 'timing_source': 'code 0x080387DC', 'directions': mirror}
    anims['ball'] = {'raw_index': 0, 'loop': True, 'timing_source': 'code 0x080387DC', 'directions': {'any': ball}}
    sheet, recs_ = B.write()
    return [{'id': 'buu_bbk_flare', 'name': 'Big Bang Kamehameha flare', 'category': 'effect',
             'name_confidence': 'high', 'sheet': sheet, 'frames': recs_, 'animations': anims,
             'rom': {'flare_records': hx(BBK_FLARE), 'ball_records': hx(BBK_BALL), 'palette': hx(PAL)},
             'notes': 'flare / mirror steps = k = (t >> 2) & 3; ball step (t >> 2) & 1; anchor = piece position '
                      '(record origin added)'}]


# ---------------------------------------------------------------------------------------------------------------
# Names of the beam piece runs (generated); every other effect run is named in data/names_buu.json `effects`
# (keyed by the first record address, from the effect reverse engineering in re/buu/notes.md "Effect graphics").
PIECES = ['start', 'start (frame 2)', 'start mirror', 'body 8', 'body 16', 'body 32', 'body (2nd gfx)', 'head',
          'head (frame 2)', 'head mirror', 'impact', 'impact (frame 2)', 'impact mirror', 'tail', 'tail (frame 2)',
          'tail mirror']


def run_names():
    """{'XXXXXXXX': {'name', 'confidence', 'notes'}} for misc_export.effects"""
    R = Rom('buu')
    out = {}
    for slug, label, d in BEAMS:
        table = R.u32(d + 12)
        for di, dn in enumerate(DIRS):
            for i, pn in enumerate(PIECES):
                a = table + 0xC0 * di + 12 * i
                out[f'{a:08X}'] = {'name': f'{label} ({dn}): {pn}', 'confidence': 'high',
                                   'notes': f'beam piece table {table:#010x} (level definition {d:#010x}); drawn as '
                                            f'buu_beam_{slug}'}
    return out
