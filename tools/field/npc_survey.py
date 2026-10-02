"""npc_survey.py CONTENT_BASE: areas with NPCs whose behaviour moves (wander / wander_rect / walk_to)."""
import json, glob, sys, collections
base = sys.argv[1]
for f in sorted(glob.glob(base + '/maps/*/objects.json')):
    o = json.load(open(f))
    for a in o['areas']:
        c = collections.Counter()
        for x in a['objects']:
            if x['type'] in ('npc',):
                ops = tuple(b['op'] for b in x['behaviour'] or [])
                c[(('talk' if x.get('talk') is not None else 'plain'), ops, str(x['condition'])[:30])] += 1
        mov = {k: v for k, v in c.items() if any(op in ('wander', 'wander_rect', 'walk_to', 'face_player') for op in k[1])}
        if mov:
            print(a['key'], a['name'], f.split('/')[-2], a['variant'], mov)
