"""objs.py W-A : non-enemy objects of an area + exits (enemies counted)"""
import json, sys, glob
R = str(__import__('pathlib').Path(__file__).resolve().parents[2]) + '/'
w, a = map(int, sys.argv[1].split('-'))
for f in sorted(glob.glob(R + f'work/field_content/maps/area_{w:02d}_{a:03d}*/objects.json')):
    d = json.load(open(f))
    print('==', f.split('/')[-2])
    for t in d.get('map_triggers', []):
        print('  map', t.get('type'), t.get('rect'), t.get('to', {}).get('key'), t.get('push'), t.get('condition'))
    for ar in d['areas']:
        print(' area', ar['key'], 'v', ar.get('variant'), ar.get('name'))
        ne = 0
        for o in ar['objects']:
            if o.get('type') == 'enemy':
                ne += 1
                print('   enemy', o.get('pos'), o.get('condition'), o.get('enemy_type', o.get('type_id')), o.get('sprite_index'))
                continue
            print('  ', o.get('type'), o.get('event') or o.get('script') or '', o.get('rect') or o.get('pos'),
                  (o.get('activation') or {}).get('mode'), (o.get('activation') or {}).get('facing', ''), o.get('condition'),
                  (o.get('talk') or {}).get('event', ''), (o.get('to') or {}).get('key', ''))
