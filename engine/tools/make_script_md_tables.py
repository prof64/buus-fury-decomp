"""Generate the native / setter reference tables of re/buu/SCRIPT.md (between the GENERATED markers) from
engine/annotations/script_natives.json and script_setters.json."""
import json
import re
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
ANN = ROOT / 'engine' / 'annotations'
nat = json.loads((ANN / 'script_natives.json').read_text())
setters = json.loads((ANN / 'script_setters.json').read_text())


def cell(s):
    return (s or '').replace('|', '\\|').replace('\n', ' ').strip()


lines = ['| id | name(args) | pops→push | kind | blocks | status | semantics (engine function) |', '|---|---|---|---|---|---|---|']
for e in nat:
    args = ', '.join(e.get('args') or [])
    b = e.get('blocking')
    b = 'cond.' if b == 'conditional' else ('yes' if b is True else 'no')
    sem = cell(e.get('semantics'))
    fn = cell(e.get('engine_fn'))
    if fn:
        sem += f' ({fn})'
    lines.append(f"| 0x{e['id']:02X} | `{e['name']}({args})` | {e['pops']}→{e['pushes']} | {e.get('kind', '')} | {b} | "
                 f"{e.get('status', '')} | {sem} |")
lines += ['', '| setter | name | semantics | status |', '|---|---|---|---|']
for s in setters:
    lines.append(f"| {s['id']} | `{s['name']}` | {cell(s.get('semantics'))} | {s.get('status', '')} |")
table = '\n'.join(lines)
p = ROOT / 're' / 'buu' / 'SCRIPT.md'
md = p.read_text()
md = re.sub(r'(<!-- GENERATED natives -->\n).*?(<!-- /GENERATED -->)', lambda m: m.group(1) + table + '\n' + m.group(2), md,
            flags=re.S)
p.write_text(md)
print('tables written', len(nat), len(setters))
