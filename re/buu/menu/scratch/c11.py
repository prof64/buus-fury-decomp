import json, glob
R = '/Users/prof/source/buus-fury-decomp/'
d = json.load(open(R + 'data/menu/title_continue.json'))
s = d['meta']['saves'][0]
print([(k, c['form'], c['flags']) for k, c in enumerate(s['chars'])][:4], s['current'])
B = R + 'engine/target/tmp/mods-base/base/'
for f in (s['chars'][0]['form'], s['chars'][1]['form'], s['chars'][2]['form']):
    fj = json.load(open(glob.glob(B + 'forms/%02d_*.json' % f)[0]))
    print(f, fj['gfx_id'], fj['name'])
    for k in range(4):
        g = glob.glob(B + 'sprites/%03d_*/sprite.json' % (fj['gfx_id'] - 9 - k))
        if g:
            sj = json.load(open(g[0]))
            an = sj['animations']
            print('  sprite', g[0].split('/')[-2], [(n, a['raw_index']) for n, a in an.items()][:10], sj.get('shadow'))
            idle = [a for a in an.values() if a['raw_index'] == 1][0]
            print('   idle', idle.get('timing_source'), {dd: [(st['frame'], st['duration_frames'], st.get('attach')) for st in v] for dd, v in idle['directions'].items()})
            break
