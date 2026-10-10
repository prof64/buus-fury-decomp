"""engine/annotations/script_inline.json: compact list of inline bytecode scripts (address, purpose, referrer kinds)
taken from the full catalog engine/annotations/script_catalog.json (built by re/buu/script_cat_build.py).
wf-script compiles this list in to know which raw-code addresses are scripts."""
import json
from pathlib import Path

ANN = Path(__file__).resolve().parents[1] / 'annotations'
cat = json.loads((ANN / 'script_catalog.json').read_text())
out = []
for s in cat['scripts']:
    if s.get('table_index') is not None or not s['id'].startswith('inline_'):
        continue
    kinds = sorted({r['kind'] for r in s.get('referenced_by', [])})
    out.append({'addr': s['code_addr'], 'purpose': s.get('purpose'), 'refs': kinds,
                'where': [r.get('where', '') for r in s.get('referenced_by', [])][:4]})
out.sort(key=lambda e: int(e['addr'], 16))
(ANN / 'script_inline.json').write_text(json.dumps(out, indent=0) + '\n')
print(len(out), 'inline scripts')

# per-event purpose / referrer kinds (script_events.json)
ev = []
for s in cat['scripts']:
    if s.get('table_index') is None:
        continue
    kinds = sorted({r['kind'] for r in s.get('referenced_by', [])})
    ev.append({'event': s['table_index'], 'purpose': s.get('purpose'), 'refs': kinds})
ev.sort(key=lambda e: e['event'])
(ANN / 'script_events.json').write_text(json.dumps(ev, indent=0) + '\n')
print(len(ev), 'events')
