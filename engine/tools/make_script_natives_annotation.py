"""Merge the per-range native decodes into engine/annotations/script_natives.json (compiled into wf-script).

Inputs (any missing range falls back to work/buu/script/sig_guess.json placeholders):
  engine/annotations/script_natives_a.json  (0x00..0x57)
  engine/annotations/script_natives_b.json  (0x58..0xA7)
  engine/annotations/script_natives_c.json  (0xA8..0xF7)
Run: .venv/bin/python engine/tools/make_script_natives_annotation.py
"""
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
ANN = ROOT / 'engine' / 'annotations'
FIELDS = ['id', 'name', 'pops', 'pushes', 'args', 'category', 'kind', 'blocking', 'handler', 'engine_fn',
          'semantics', 'status', 'evidence', 'uses']

by_id = {}
guess_p = ROOT / 'work' / 'buu' / 'script' / 'sig_guess.json'
if guess_p.exists():
    for k, v in json.loads(guess_p.read_text()).items():
        n = int(k)
        by_id[n] = {'id': n, 'name': f'native_{n:02x}', 'pops': v['pops'], 'pushes': v['pushes'],
                    'args': [f'a{i}' for i in range(v['pops'])], 'status': 'GUESS'}
for part in 'abc':
    p = ANN / f'script_natives_{part}.json'
    if not p.exists():
        continue
    for e in json.loads(p.read_text()):
        n = e['id'] if isinstance(e['id'], int) else int(e['id'], 0)
        e['id'] = n
        by_id[n] = e
# API names must be unique per arity (Rhai overloads by arity): resolve clashes between the three decodes
RENAME = {
    0x4a: 'actor_wait',              # 0x88 is the plain modal wait(frames)
    0x5c: 'fire_beam_alt',           # 0x46 fire_beam
    0x8a: 'spawn_trigger_force',     # trigger list +0x10 (0x36 spawn_map_object = actor list +0x14; round-2 emulator)
    0x32: 'camera_scroll_to',        # modal; 0x6b camera_pan_to is the queued command
    0xbd: 'hide_stopwatch_hud',      # same body as 0xb1 hide_timer_hud, pairs with 0xbc
    0xb5: 'run_event', 0xb6: 'run_event', 0xb7: 'run_event', 0xb8: 'run_event', 0xb9: 'run_event',
    0xd6: 'run_script', 0xd7: 'run_script', 0xd8: 'run_script',
}
for n, nm in RENAME.items():
    if n in by_id and by_id[n].get('name') != nm:
        by_id[n]['decoded_name'] = by_id[n].get('name')
        by_id[n]['name'] = nm
# warp / warp_fade: the 3rd argument is the map variant (confirmed by the wf-world map-object extraction)
for n in (0x76, 0x77):
    if n in by_id and by_id[n].get('args'):
        by_id[n]['args'] = ['world', 'map', 'variant', 'x', 'y']
        by_id[n]['semantics'] = (by_id[n].get('semantics', '') +
                                 ' NOTE: the 3rd argument is the map VARIANT (layout), not a facing '
                                 '(cross-checked by engine/crates/wf-world warps).')
for n in (0xb5, 0xb6, 0xb7, 0xb8, 0xb9):
    if n in by_id and by_id[n].get('args'):
        by_id[n]['args'][0] = 'event'
seen = {}
for n in sorted(by_id):
    k = (by_id[n]['name'], by_id[n]['pops'])
    if k in seen:
        raise SystemExit(f'name clash {k}: 0x{seen[k]:02x} and 0x{n:02x}')
    seen[k] = n
out = []
for n in range(248):
    e = by_id.get(n, {'id': n, 'name': f'native_{n:02x}', 'pops': 0, 'pushes': 0, 'args': [], 'status': 'UNKNOWN'})
    e = {k: e[k] for k in FIELDS if k in e} | {k: v for k, v in e.items() if k not in FIELDS}
    e['pushes'] = int(bool(e.get('pushes')))
    out.append(e)
(ANN / 'script_natives.json').write_text(json.dumps(out, indent=1, ensure_ascii=False) + '\n')
print(len(out), 'natives;', sum(1 for e in out if e.get('status') in ('VERIFIED', 'INFERRED')), 'decoded')
