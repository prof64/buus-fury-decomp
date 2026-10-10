"""Export LoG2 / Buu's Fury sprite entities: frames, sheets and data/<game>/sprites.json."""
import sys
import json
from pathlib import Path
from gba import palette, ROOT
from wf_sprites import WFSprites
from render import obj_image, compose
from export import EntityBuilder, hx, DIRS
import names

PAL = {'log2': 0x81DA6C8, 'buu': 0x805652C}


def load_timing(game, defs=None):
    """Measured frame durations from tools/extract/data/anim_obs_<game>.json (written by autobattle.py):
    {sprite id: {anim index: {'durations': [...], 'source': 'measured'}}}."""
    p = Path(__file__).resolve().parent / 'data' / f'anim_obs_{game}.json'
    if not p.exists() or not defs:
        return {}
    obs = json.loads(p.read_text())
    out = {}
    for d in defs:
        if not d:
            continue
        if game == 'buu':   # keyed by anim struct address
            o = {str(i): obs[f'{a["addr"]:08X}']['-'] for i, a in enumerate(d['anims'])
                 if a and f'{a["addr"]:08X}' in obs and i > 0}
        else:
            o = obs.get(f'{d["addr"]:08X}')
        if not o:
            continue
        for ai, rec in o.items():
            if ai == '0' or not rec.get('dur'):
                continue
            a = d['anims'][int(ai)] if int(ai) < len(d['anims']) else None
            if not a:
                continue
            durs = []
            for k in range(a['nframes']):
                lst = rec['dur'].get(str(k))
                durs.append(sorted(lst)[len(lst) // 2] if lst else None)
            known = [x for x in durs if x]
            if not known:
                continue
            fill = sorted(known)[len(known) // 2]
            out.setdefault(str(d['id']), {})[ai] = {
                'durations': [x or fill for x in durs], 'source': 'measured (emulator, %d segments)' % rec['count'],
                'observed_sequences': sorted(rec['seqs'].items(), key=lambda kv: -kv[1])[:3]}
    return out


def export(game, write_frames=True, only=None):
    S = WFSprites(game)
    pal = palette(S.R.bytes(PAL[game], 512), 256)
    defs = S.all_defs()
    N = names.load(game)
    timing = load_timing(game, defs)
    out = []
    for d in defs:
        if not d:
            continue
        if only is not None and d['id'] not in only:
            continue
        info = N.entity(d['id'])
        slug = info['slug']
        B = EntityBuilder(game, slug)
        cache = {}

        def frame_ref(fa):
            if fa not in cache:
                f = S.frame(fa)
                ims = [obj_image(S.gfx(o['gfx']), o['W'], o['H'], o['bpp'], pal, d['remap']) for o in f['objs']]
                im, ox, oy = compose(f['objs'], ims)
                cache[fa] = B.add(im, ox, oy, hx(fa))
            return cache[fa]

        anims = {}
        tinfo = timing.get(str(d['id']), {})
        for ai, a in enumerate(d['anims']):
            if not a:
                continue
            aname = N.anim_name(d['id'], ai, a['nframes'])
            base = N.default_timing(d['id'], ai)
            meas = tinfo.get(str(ai))
            t = dict(base or {}, **(meas or {})) if (base or meas) else None
            if base and base.get('lock'):
                t = base
            dirs = {}
            for di, fl in enumerate(a['dirs']):
                if not fl:
                    continue
                seq = []
                for k, fa in enumerate(fl):
                    fi, flip = frame_ref(fa)
                    dur = t['durations'][k] if t and k < len(t.get('durations', [])) else (t or {}).get('default', 8)
                    st = {'frame': fi, 'duration_frames': dur, 'flip': flip, 'rom': hx(fa)}
                    att = (d.get('attach') or {}).get('offsets', {}).get(ai)
                    if att:
                        st['attach'] = list(att[di][k])
                    seq.append(st)
                dirs[DIRS[di]] = seq
            anims[aname] = {'raw_index': ai, 'loop': (t or {}).get('loop', True),
                            'observed_frame_orders': [k for k, _ in (t or {}).get('observed_sequences', [])] or None,
                            'timing_source': (t or {}).get('source', 'default'),
                            'rom': hx(a.get('addr') or (a.get('lists') or [None])[0]), 'directions': dirs}
        sheet, frames = B.write(write_frames)
        boxes = [{'dir': DIRS[i], 'x0': b[0], 'y0': b[1], 'x1': b[2], 'y1': b[3]} for i, b in enumerate(d['boxes'])]
        out.append({
            'id': slug, 'sprite_index': d['id'], 'name': info['name'], 'category': info['category'],
            'name_confidence': info.get('confidence', 'low'), 'notes': info.get('notes'),
            'rom': {'def': hx(d['addr']), 'remap_table': hx(d['remap_addr'] or None),
                    'palette': hx(PAL[game]), 'gfx_format': 'webfoot-lz (type 1) 8bpp tiles, shared 256-colour OBJ palette'},
            'header': d['header'], 'sheet': sheet, 'frames': frames, 'animations': anims,
            **({'attachment': {'kind': 'halo', 'sprite': 'buu_halo', 'table': hx(d['attach']['addr']),
                               'note': 'steps carry attach = [dx, dy]: halo OBJ position relative to the entity position'}}
               if d.get('attach') else {}),
            'hitboxes': {'body_per_direction': boxes,
                         'note': 'collision rectangle (x0,y0)-(x1,y1) relative to entity position, read by 0x80079B8 (log2)'},
        })
        print(game, d['id'], slug, len(frames), 'frames', len(anims), 'anims', flush=True)
    return out


if __name__ == '__main__':
    g = sys.argv[1]
    only = set(int(x) for x in sys.argv[2].split(',')) if len(sys.argv) > 2 else None
    ents = export(g, only=only)
    if only is None:
        from export import write_json
        write_json(g, 'sprites_entities.json', ents)
