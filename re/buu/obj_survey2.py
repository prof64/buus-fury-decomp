import json, glob, collections
C = '/tmp/wfcontent/base/maps/*/objects.json'
hits = collections.Counter()
for f in sorted(glob.glob(C)):
    o = json.load(open(f))
    for a in o['areas']:
        c = collections.Counter(x['type'] for x in a['objects'])
        for x in a['objects']:
            if x['type'] == 'breakable':
                hits[(x['hits'], x['param_a'])] += 1
        if c['breakable'] or c['item'] or c['save_point']:
            en = [(x['condition'], x.get('enemy_type')) for x in a['objects'] if x['type'] == 'enemy']
            print(f.split('/')[-2], a['key'], a['name'], dict(c), 'enemy conds', set(json.dumps(e[0]) for e in en))
print(hits)
