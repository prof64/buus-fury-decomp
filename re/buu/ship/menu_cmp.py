"""menu_cmp.py LOG TIMING.json [skip] -- the timing model's redraws (a WF_SND_DEBUG log of one segment: `menu redraw`
+ `snd redraw` lines) beside the emulator's measured iterations (menu_timing.py), in order; `skip` = model redraws
to drop first (the measurement window starts later). Prints page, the model's own cycles (t0 + base), the game's,
the difference, both ends and frame counts; '!!' marks a wrong frame count."""
import json
import re
import sys

log, tim = sys.argv[1], json.load(open(sys.argv[2]))
skip = int(sys.argv[3]) if len(sys.argv) > 3 else 0
ours = []
page = None
for ln in open(log, errors='replace'):
    m = re.match(r'menu redraw f\d+ (\w+) const (\d+) timed (\S+ ?\S*) pressed (\S+) released (\S+) objs \d+ \| (.*)', ln)
    if m:
        page = (m.group(1), int(m.group(2)), m.group(4), m.group(6)[:60])
        if 'None' in m.group(3):
            ours.append({'page': page[0], 'const': page[1], 'own': None, 'feat': page[3], 'pressed': page[2]})
        continue
    m = re.match(r'snd redraw f\d+: t0 (\d+) base (\d+) irq at (\d+) cost (\d+) post n (\d+) cost (\d+) end (\d+) -> (\d+) frames, margin (-?\d+)', ln)
    if m and page:
        t0, base, irqt, irqc, pn, pc, end, fr, mg = (int(x) for x in m.groups())
        ours.append({'page': page[0], 'own': t0 + base, 'irq': irqc, 'post': (pn, pc), 'end': end, 'frames': fr, 'margin': mg, 'feat': page[3], 'pressed': page[2]})
        page = None
ours = ours[skip:]
for o, g in zip(ours, tim):
    gi = sum(x[2] for x in g['irq'])
    if o['own'] is None:
        print(f"{g['f0']:5d} {o['page']:10s} (const {o['const']})          game own {g['own']:8d} irq {gi:6d} post {g['post']} end {g['end']} frames {g['frames']} {'!!' if o['const'] != g['frames'] else ''} | {o['feat']}")
        continue
    bad = '!!' if o['frames'] != g['frames'] else ''
    print(f"{g['f0']:5d} {o['page']:10s} own {o['own']:8d} game {g['own']:8d} d {o['own'] - g['own']:7d} | irq {o['irq']:6d} / {gi:6d} | post {o['post'][0]}:{o['post'][1]} / {g['post'][0]}:{g['post'][1]} | end {o['end']} / {g['end']} d {o['end'] - g['end']:6d} | frames {o['frames']} / {g['frames']} {bad} k{o['pressed']} | {o['feat']}")
print(len(ours), 'model redraws,', len(tim), 'measured')
