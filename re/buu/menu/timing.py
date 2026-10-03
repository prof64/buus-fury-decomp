"""timing.py -- CPU time of menu redraws in the emulator (cycle-stamped breakpoints, emut/libemut.dylib).

    .venv/bin/python re/buu/menu/timing.py SCENARIO [DRAW_ENTRY_HEX]

For every redraw (from the page's draw entry to the update's return 0x0803D810) prints the elapsed cycles, the
cycles spent in the IRQ handler meanwhile (0x03000B6C .. 0x03000C94), the per-primitive totals, the start and end
positions in the frame (scanline) and the frames the iteration took.
"""
import sys, json
from pathlib import Path
HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
sys.path.insert(0, str(HERE / 'emut'))
from emut import emut  # noqa: E402
from cap import program  # noqa: E402
import capture  # noqa: E402

FRAME = 280896
IRQ_IN, IRQ_OUT = 0x03000B6C, 0x03000C94
UPD_RET = 0x0803D810  # the mode loop right after update() returns
INPUT = 0x0803D4A0
PRIMS = {
    'bg': (0x08004F1C, [0x08004F4E]),
    'panel': (0x08004F50, [0x08005094]),
    'blit': (0x0803F922, [0x0803F95A]),
    'pic': (0x0803D940, [0x0803D95C]),
    'glyphS': (0x0803F5A4, [0x0803F6FE + 2]),
    'glyph': (0x0803F444, []),
    'str': (0x0803DADA, [0x0803DB78]),
}


def measure(name, draw_entry, extra=()):
    state, setup, pre, keys = capture.SCENARIOS[name]
    e = emut(state)
    if setup:
        setup(e)
    for k in program(pre):
        e.set_keys(k)
        e.run(1)
    addrs = {IRQ_IN: 'irq_in', IRQ_OUT: 'irq_out', UPD_RET: 'upd', INPUT: 'input', draw_entry: 'draw'}
    for a, nm in extra:
        addrs[a] = nm
    for nm, (a, rets) in PRIMS.items():
        addrs[a] = nm + '>'
        for r in rets:
            addrs[r] = nm + '<'
    for a in addrs:
        e.breakpoint(a)
    e.hits()
    events = []
    starts = []
    for i, k in enumerate(program(keys)):
        starts.append(e.cycles())
        e.set_keys(k)
        e.run(1)
        for h in e.hits():
            if h['kind'] != 'break':
                continue
            nm = addrs.get(h['addr'] & ~1)
            if nm:
                events.append((h['cycles'], i, nm, h['vcount']))
    # redraws
    out = []
    cur = None
    irq_depth = 0
    irq_t0 = 0
    for c, fr, nm, vc in events:
        if nm == 'irq_in':
            if irq_depth == 0:
                irq_t0 = c
            irq_depth += 1
        elif nm == 'irq_out':
            irq_depth = max(0, irq_depth - 1)
            if irq_depth == 0 and cur is not None:
                cur['irq'] += c - irq_t0
                cur['nirq'] += 1
        elif nm == 'draw':
            cur = dict(frame=fr, t0=c, vc0=vc, irq=0, nirq=0, prims={})
        elif nm == 'upd' and cur is not None:
            cur['t1'] = c
            cur['vc1'] = vc
            cur['frame1'] = fr
            cur['elapsed'] = c - cur['t0']
            cur['work'] = cur['elapsed'] - cur['irq']
            out.append(cur)
            cur = None
        elif cur is not None and nm.endswith('>'):
            p = nm[:-1]
            cur['prims'].setdefault(p, [0, 0, None])
            cur['prims'][p][2] = c
            cur['prims'][p][1] += 1
        elif cur is not None and nm.endswith('<'):
            p = nm[:-1]
            st = cur['prims'].get(p)
            if st and st[2] is not None:
                st[0] += c - st[2]
                st[2] = None
    return out, starts


if __name__ == '__main__':
    name = sys.argv[1]
    entry = int(sys.argv[2], 16) if len(sys.argv) > 2 else 0x080154D4
    res, starts = measure(name, entry)
    for r in res:
        pr = ' '.join(f"{k}:{v[1]}/{v[0]}" for k, v in r['prims'].items())
        print(f"f{r['frame']:4d}..{r['frame1']:4d} line {r['vc0']:3d}->{r['vc1']:3d} elapsed {r['elapsed']:7d} irq {r['irq']:6d}/{r['nirq']} work {r['work']:7d} | {pr}")
