"""cycles.py SCENARIO [...] -- the long iterations of menu scenarios (capture.py's SCENARIOS) measured in CPU cycles
(re/buu/ship/menu_timing.py `measure`: own cycles, interrupt mixes, the catch-up update, frames), for the redraw
timing model (engine/crates/wf-core/src/sndsim.rs; engine/docs/segments.md §9). Headless, muted, instruction-
stepped (~30 frames / s). Output: work/buu/menu/cycles/<scenario>.json + one line per iteration."""
import json
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
sys.path.insert(0, str(HERE.parent / 'ship'))
from cap import Cap, program, OUT  # noqa: E402
import capture  # noqa: E402
import menu_timing  # noqa: E402


def run(name, prof=False):
    state, setup, pre, keys, *rest = capture.SCENARIOS[name]
    extra = rest[0] if rest else {}
    c = Cap(state)
    for b in c.bps:
        c.e.clear(b)
    if setup:
        setup(c.e)
    for k in program(pre):
        c.e.set_keys(k)
        c.e.run(1)
    if extra.get('trigger'):
        extra['trigger'](c.e)
    ks = program(keys)
    res = menu_timing.measure(c.e, ks + [0], 0, len(ks), prof, '--all' in sys.argv)
    if '--all' in sys.argv:
        name += '_all'
    d = OUT / 'cycles'
    d.mkdir(parents=True, exist_ok=True)
    (d / f'{name}.json').write_text(json.dumps(res))
    for r in res:
        print(name, r['f0'], 'k%x' % r['keys'], 'own', r['own'], 'irq', [x[2] for x in r['irq']], 'post', r['post'], 'end', r['end'], 'frames', r['frames'], flush=True)


if __name__ == '__main__':
    names = [a for a in sys.argv[1:] if not a.startswith('--')] or sorted(capture.SCENARIOS)
    for n in names:
        run(n, '--prof' in sys.argv)
