"""Chapter segments (headless, muted): replay a route's key log (run_route.py --log) from its start state and cut
it at every importable area entry (an exit arrival: the end of the first complete logic frame with the leader in
the new area, autowalking in, no event running — tools/segments/segment.py's rule), one segment file per cut, in
tools/segments/segment.py's format (data/segments/NAME.json shape; spec engine/docs/segments.md).

usage: rec_segments.py RUNDIR OUTDIR [--prefix ship] [--from-frame N] [--first N] [--max N] [--at-start]
  --from-frame: no cut before key frame N (the first segment starts at the first cut from there); --first: its number
  RUNDIR: run_route.py output (start.ss + keys.json); segments go to OUTDIR/<prefix>_<nnn>_<w>-<a>.json
  The emulator is rewound one frame at each cut (the cut is known when the next logic frame starts)."""
import json
import os
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(ROOT / 'tools' / 'segments'))
import segment as sg  # noqa: E402
from route import start_emu  # noqa: E402


def main():
    a = sys.argv[1:]
    run, out = Path(os.path.abspath(a[0])), Path(os.path.abspath(a[1]))
    prefix = a[a.index('--prefix') + 1] if '--prefix' in a else 'ship'
    mx = int(a[a.index('--max') + 1]) if '--max' in a else 10 ** 9
    ff = int(a[a.index('--from-frame') + 1]) if '--from-frame' in a else 0
    n0 = int(a[a.index('--first') + 1]) - 1 if '--first' in a else 0
    out.mkdir(parents=True, exist_ok=True)
    log = json.loads((run / 'keys.json').read_text())
    keys = log['keys']
    e = start_emu(str(run / (log['from'] + '.ss' if log.get('from') else 'start.ss')))
    seg = None            # {'init', 'rec', 'rows', 'n', 'area'}
    nseg = n0
    pending = False       # an area changed: looking for the cut frame
    prev_state = None
    prev_row = None
    prev_lf = e.r32(0x03000F94)
    change_at = 0
    flags = e.read(sg.GD + 0x55, 0x80).hex()
    area0 = [e.r8(sg.WS), e.r8(sg.WS + 1), e.r8(sg.WS + 2)]

    def finish(s):
        nonlocal nseg
        if s is None or not s['rows']:
            return
        s['rec'].disarm()
        ar = s['init']['area']
        name = f'{prefix}_{s["n"]:03d}_{ar[0]}-{ar[1]}'
        doc = {'kind': 'segment', 'name': name,
               'source': {'kind': 'route', 'run': str(run.relative_to(ROOT)) if run.is_relative_to(ROOT) else str(run),
                          'frame': s['f0']},
               'init': s['init'], 'objs': s['rec'].objs, 'frames': s['rows']}
        p = sg.save(name, doc) if out == sg.OUT else save_to(out, name, doc)
        print(p.name, len(s['rows']), 'frames', flush=True)

    def save_to(d, name, doc):
        old = sg.OUT
        sg.OUT = d
        try:
            return sg.save(name, doc)
        finally:
            sg.OUT = old

    if '--at-start' in a:
        # the start state is itself a cut (a previous run's segment state): the first segment starts at frame 0
        init, rec = sg.init_of(e)
        rec.arm()
        nseg += 1
        seg = {'init': init, 'rec': rec, 'rows': [], 'n': nseg, 'f0': 0}
    f = 0
    while f < len(keys):
        k = keys[f]
        if pending:
            prev_state = e.save()
        e.set_keys(k)
        e.run(1)
        row = {'keys': k}
        if seg is not None:
            row = seg['rec'].row(k)
        flags = sg.row_extras(e, row, flags)
        if prev_row is not None and row['area'] != prev_row['area']:
            pending = True
            change_at = len(seg['rows']) if seg is not None else 0
        elif pending and prev_row is not None and e.r32(0x03000F94) != prev_lf and prev_row['p'] is not None:
            # the previous frame closed the load's first complete logic frame: cut there if importable
            pending = False
            p = prev_row['p']
            if prev_row.get('depth') == 0 and p and p[3] == 'autowalk' and f >= ff:
                if seg is not None:
                    # the old segment ends before the area change: the game's load spans several video frames
                    # (objects appear while it runs); the next segment starts after the load's logic frame
                    cut_lf = seg['rows'][change_at]['lf'] if change_at < len(seg['rows']) else None
                    del seg['rows'][change_at:]
                    # a logic frame that spans video frames (a lag frame before the load): its first video frames
                    # show the old area mid-frame; drop the incomplete logic frame too
                    while seg['rows'] and seg['rows'][-1]['lf'] == cut_lf:
                        seg['rows'].pop()
                finish(seg)
                if nseg >= mx:
                    return
                e.load(prev_state)
                e.hits()                        # drop the discarded frame's breakpoint hits
                (out / 'states').mkdir(exist_ok=True)
                ar = [e.r8(sg.WS), e.r8(sg.WS + 1)]
                e.save_file(str(out / 'states' / f'{prefix}_{nseg + 1:03d}_{ar[0]}-{ar[1]}.ss'))
                init, rec = sg.init_of(e)
                rec.arm()
                nseg += 1
                seg = {'init': init, 'rec': rec, 'rows': [], 'n': nseg, 'f0': f}
                flags = e.read(sg.GD + 0x55, 0x80).hex()
                e.set_keys(k)
                e.run(1)
                row = rec.row(k)
                flags = sg.row_extras(e, row, flags)
        prev_lf = e.r32(0x03000F94)
        if seg is not None:
            seg['rows'].append(row)
        prev_row = row
        f += 1
    finish(seg)
    print('frames', f, 'segments', nseg, 'start area', area0)


if __name__ == '__main__':
    main()
