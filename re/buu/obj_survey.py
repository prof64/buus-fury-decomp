import json, glob, collections
C = '/tmp/wfcontent/base/maps/*/objects.json'
types = collections.Counter(); acts = collections.Counter(); ex = {}
for f in sorted(glob.glob(C)):
    o = json.load(open(f))
    objs = [(None, x) for x in o.get('map_triggers', [])]
    for a in o['areas']:
        objs += [(a['key'], x) for x in a['objects']]
    for k, x in objs:
        types[x['type']] += 1
        if 'activation' in x:
            m = json.dumps(x['activation'])
            acts[(x['type'], m)] += 1
            ex.setdefault((x['type'], m), (f.split('/')[-2], k))
        if x['type'] in ('breakable', 'item', 'save_point'):
            ex.setdefault(x['type'] + str(len(ex)), (f.split('/')[-2], k, json.dumps(x)[:400]))
print(types)
for k, v in sorted(acts.items()):
    print(v, k, ex[k])
for k, v in ex.items():
    if isinstance(k, str):
        print(k, v)
