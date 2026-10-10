"""cycles_fit.py LOG:CYCLES.json[:skip] ... -- per page: the measured own cycles of the long iterations (cycles.py /
ship/menu_timing.py) against the timing model's draw-list features (the `menu redraw` lines of a WF_SND_DEBUG log
of the same scenario, in order; `skip` = measured iterations before the first logged redraw, default 1: the
screen's opening). Prints one row per redraw: page, own, the generic part (3.17 x glyph instructions + pictures +
plain text / fills / pixels, as sndsim::layer_cycles with line = 0), the rest, text lines, and per page the least
squares fit rest = K + c x lines with its residuals."""
import json
import re
import sys
from collections import defaultdict

import numpy as np


def generic(feat):
    """sndsim::layer_cycles(line = 0) of one layer's log text '(gn, small, extra) tc(g, c, b, l) ops...'"""
    m = re.match(r'\((\d+), (\d+), (\d+)\) tc\((\d+), (\d+), (\d+), (\d+)\) (.*)', feat.strip())
    if not m:
        return 0, 0, 0, 0, feat.strip()[:40]
    gn, small, extra, g, c, b, ln = (int(x) for x in m.groups()[:7])
    cyc = gn * 31700 // 10000 + small * 7284 + extra
    for w, h, name in re.findall(r'img(\d+)x(\d+):(\S+)', m.group(8)):
        w, h = int(w), int(h)
        if (w, h) != (16, 16):
            cyc += w * h * 2845 // 100
        elif name.startswith('cursor'):
            cyc -= 688
    return cyc, ln, gn, g, m.group(8)


rows = defaultdict(list)
args = [x for x in sys.argv[1:] if x not in ('--rows', '--layers')]
if args and args[0] == '--dir':
    # --dir LOGDIR CYCLESDIR [scenario ...]: LOGDIR/<scenario>.log + CYCLESDIR/<scenario>.json, aligned from the end
    # (the last measured iteration is the screen's close)
    import os
    names = args[3:] or sorted(f[:-4] for f in os.listdir(args[1]) if f.endswith('.log') and os.path.exists(f'{args[2]}/{f[:-4]}.json'))
    args = [f'{args[1]}/{n}.log:{args[2]}/{n}.json:auto' for n in names]
for arg in args:
    parts = arg.split(':')
    log, cyc = parts[0], json.load(open(parts[1]))
    skip = parts[2] if len(parts) > 2 else 1
    ours = []
    for ln in open(log, errors='replace'):
        m = re.match(r'menu redraw f\d+ (?:mf(\d+) )?(\w+) const (\d+) timed (.*?) pressed (\S+) released (\S+) objs (\d+) \| (.*)', ln)
        if m:
            layers = m.group(8).split(' || ')
            tm = re.match(r'Some\(\((\d+), (-?\d+)\)\)', m.group(4))
            ours.append((m.group(2), int(m.group(3)), m.group(5), m.group(6), layers, int(tm.group(1)) + int(tm.group(2)) if tm else None, int(m.group(1) or 0)))
    ours = [o for o in ours if o[1] > 1]
    if skip == 'auto':
        skip = max(len(cyc) - len(ours) - 1, 0)
    skip = int(skip)
    # align by video frame: the model's menu frame counter (mf) + a constant offset = the measured f0
    by_f = {g['f0']: g for g in cyc}
    best = (0, 0)
    from collections import Counter
    offs = Counter(f - o[6] for o in ours for f in by_f)
    if offs:
        off, n = offs.most_common(1)[0]
        best = (n, off)
    pairs = [(o, by_f[o[6] + best[1]]) for o in ours if o[6] + best[1] in by_f]
    if best[0] < len(ours):
        print(f'# {parts[1].split("/")[-1]}: {len(ours)} model redraws, {len(cyc)} measured, {best[0]} aligned (offset {best[1]})')
    prev = None
    for o, g in pairs:
        fresh = 1 if prev != o[0] else 0
        prev = o[0]
        gen = [generic(x) for x in o[4]]
        tot = sum(x[0] for x in gen)
        lines = sum(x[1] for x in gen)
        key = o[0] + ('+' + str(len(gen) - 1) if len(gen) > 1 else '')
        rows[key].append(dict(lay=[(x[0], x[1], x[2]) for x in gen], model=o[5], fresh=fresh, own=g['own'], gen=tot, rest=g['own'] - tot, lines=lines, frames=g['frames'], const=o[1], end=g['end'], f0=g['f0'],
                              pressed=o[2], released=o[3], gn=sum(x[2] for x in gen), g=sum(x[3] for x in gen), ops=gen[-1][4][:70], src=parts[1].split('/')[-1][:-5]))
for page, rs in sorted(rows.items()):
    cols = [[1, r['lines'], r['gn'] / 1000, r['fresh']] for r in rs]
    use = [k for k in range(4) if len(set(c[k] for c in cols)) > 1 or k == 0]
    import os
    if os.environ.get('FIT'):
        use = [k for k in use if str(k) in os.environ['FIT'].split(',')]
    if os.environ.get('GROUPS'):
        gr = defaultdict(list)
        for r in rs:
            gr[(r['lines'], r['gn'], r['fresh'], r['ops'][:int(os.environ['GROUPS'])])].append(r['rest'])
        for kk, v in sorted(gr.items()):
            print(f'   group lines {kk[0]:2d} gn {kk[1]:6d} fresh {kk[2]} n {len(v):2d} rest mean {np.mean(v):8.0f} spread {max(v) - min(v):6.0f} | {kk[3]}')
    A = np.array([[c[k] for k in use] for c in cols], dtype=float)
    y = np.array([r['rest'] for r in rs], dtype=float)
    k, *_ = np.linalg.lstsq(A, y, rcond=None)
    res = y - A @ k
    names = ['K', 'per line', 'per 1000 gn', 'fresh']
    print(f'== {page}: n {len(rs)} fit rest = ' + ', '.join(f'{names[u]} {v:.0f}' for u, v in zip(use, k)) + f'; residual std {res.std():.0f} max {np.abs(res).max():.0f}')
    errs = [r['model'] - r['own'] for r in rs if r['model'] is not None]
    if errs:
        print(f'   the model (t0 + base) - measured: n {len(errs)} mean {np.mean(errs):.0f} std {np.std(errs):.0f} min {min(errs)} max {max(errs)}')
    if '--layers' in sys.argv:
        for r in rs:
            print(f"  L {r['src'][:14]:14s} f{r['f0']:4d} own {r['own']:8d} frames {r['frames']} fresh {r['fresh']} k{r['pressed']}/{r['released']} layers " + ' '.join(f'[cyc {a} lines {b} gn {c}]' for a, b, c in r['lay']))
    if '--rows' not in sys.argv:
        continue
    for r, e in zip(rs, res):
        print(f"  {r['src'][:18]:18s} f{r['f0']:4d} err {(r['model'] - r['own']) if r['model'] is not None else 0:6d} own {r['own']:8d} gen {r['gen']:7d} rest {r['rest']:7d} lines {r['lines']:2d} gn {r['gn']:6d} g {r['g']:3d} res {e:7.0f} frames {r['frames']} (const {r['const']}) end {r['end']} k{r['pressed']}/{r['released']} | {r['ops']}")
