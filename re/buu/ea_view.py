"""ea_view.py type [...] -- print data/battle/buu_enemy_anim.json for some enemy types, compactly."""
import json
import sys
from common import ROOT

d = json.load(open(ROOT / 'data' / 'battle' / 'buu_enemy_anim.json'))
for a in sys.argv[1:]:
    e = d['entities']['0x%02X' % int(a, 0)]
    print('=====', a, e['name'], e['sprite'], 'recorded', e['recorded'])
    for s in e['states']:
        sb = '; '.join('%s@%s %s x%d' % (x['call'], x['site'], (x['name'] or '')[:40], x['n']) for x in s['set_by'][:3])
        extra = ''
        if s['rule'] == 'sequence':
            extra = 'seq %s len %s det %s var %s lens %s exit %s' % (
                s.get('sequence')[:8], s.get('length'), s.get('deterministic'), s.get('variants'), s.get('lengths_seen'),
                [(x['call'], x['name'][:30], x['n']) for x in s.get('exit', [])[:2]])
        else:
            extra = '%s %s' % (s.get('verified'), s.get('entry_ticks'))
        print('  raw %s (%s, n%s) %s %s obs %d | %s | ai %s' % (s['raw'], s['anim'], s['nframes'], s['controller'], s['rule'], s['observed'], sb, s['ai_states'][:4]))
        print('       ', extra)
        if s.get('sfx_at'):
            print('        sfx', [(c['at'], c['name'][:30], c['n']) for c in s['sfx_at'][:6]])
    for x in e['sfx']:
        print('  SFX', x['site'], x['name'][:50], x['ids'], x['vol'], x['n'], x['scenarios'])
