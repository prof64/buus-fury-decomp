"""Export LoG1 sprite entities (uncompressed 4bpp strips)."""
import sys
from gba import palette
from log1_sprites import Log1Sprites
from render import obj_image, compose
from export import EntityBuilder, hx, DIRS, write_json
from sprites_wf_export import load_timing
import names


def export(write_frames=True, only=None):
    S = Log1Sprites()
    defs = S.all_defs()
    N = names.load('log1')
    timing = load_timing('log1', defs)
    out = []
    for d in defs:
        if not any(d['anims']):
            continue
        if only is not None and d['id'] not in only:
            continue
        info = N.entity(d['id'])
        pal = palette(S.R.bytes(d['palette_addr'], 32), 16)
        B = EntityBuilder('log1', info['slug'])
        cache = {}

        def frame_ref(p, k):
            if (p, k) not in cache:
                s, g = S.frame_gfx(p, k)
                im = obj_image(g, s['W'], s['H'], 4, pal)
                o = {'dx': s['dx'], 'dy': s['dy'], 'W': s['W'], 'H': s['H'], 'hflip': s['hflip']}
                im, ox, oy = compose([o], [im])
                cache[(p, k)] = B.add(im, ox, oy, hx(s['gfx'] + k * s['ntiles'] * 32))
            return cache[(p, k)]

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
                for k, (p, fk) in enumerate(fl):
                    fi, flip = frame_ref(p, fk)
                    dur = t['durations'][k] if t and k < len(t.get('durations', [])) else (t or {}).get('default', 8)
                    seq.append({'frame': fi, 'duration_frames': dur, 'flip': flip, 'rom': hx(p)})
                dirs[DIRS[di]] = seq
            anims[aname] = {'raw_index': ai, 'loop': (t or {}).get('loop', True),
                            'timing_source': (t or {}).get('source', 'default'),
                            'frame_count_source': 'inferred from ROM layout',
                            'rom': hx(d['anim_table'] + 16 * ai), 'directions': dirs}
        sheet, frames = B.write(write_frames)
        boxes = [{'dir': DIRS[i], 'x0': b[0], 'y0': b[1], 'x1': b[2], 'y1': b[3]} for i, b in enumerate(d['boxes'])]
        out.append({
            'id': info['slug'], 'sprite_index': d['id'], 'name': info['name'], 'category': info['category'],
            'name_confidence': info.get('confidence', 'low'), 'notes': info.get('notes'),
            'rom_labels': d.get('labels'),
            'rom': {'sprite_def': hx(d['addr']), 'anim_table': hx(d['anim_table']), 'palette': hx(d['palette_addr']),
                    'gfx_format': 'uncompressed 4bpp tiles, 16-colour palette per entity type'},
            'header': d['header'], 'sheet': sheet, 'frames': frames, 'animations': anims,
            'hitboxes': {'body_per_direction': boxes,
                         'note': 'rectangle (x0,y0)-(x1,y1) relative to entity position (sprite def +4: 4 x s16 x0,y0,x1,y1)'},
        })
        print('log1', d['id'], info['slug'], len(frames), 'frames', len(anims), 'anims', flush=True)
    return out


if __name__ == '__main__':
    ents = export()
    write_json('log1', 'sprites_entities.json', ents)
