"""menu_timing.py SEGDIR NAME a b OUT.json [--prof] [--all] -- cycle measurements of the main loop's long iterations
(menu page redraws, any page) in video frames a..b of a segment (instruction-stepped, ~30 frames / s; headless,
muted). The main loop: [VBlank] step + redraw, the sound update 0x0803F296, wait for the VBlank. Per iteration that
does not fit its frame (--all: every iteration):
  f0        video frame it starts in (the frame after the previous iteration's sound update)
  pre       cycles from f0's start (the emulator's frame boundary = the VBlank) to the main loop's sound update
  irq       the sound updates the DMA interrupt ran meanwhile: [cycles after f0's start, samples, cycles]
  own       pre - the interrupt updates' cycles (the step's and the redraw's own time, DMA interrupts included)
  post      the main loop's update: [samples, cycles]
  end       pre + post cycles: the iteration is over, the loop waits for the VBlank
  frames    video frames the iteration took
  keys      the keys held in f0
--prof adds `prof`: user-mode cycles per 0x40-byte code bucket of the iteration (interrupt code excluded) and
`calls`: executions of the first instruction of some routines (glyph renderers, blits, decompressor).
Segment states: SEGDIR/states/NAME.ss (rec_segments.py) or work/segments/NAME.ss."""
import collections
import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import ship_driver  # noqa: E402,F401
from route import start_emu  # noqa: E402

UPDATE, OBJ = 0x0803F296, 0x03003898


def main():
    a = [x for x in sys.argv[1:] if not x.startswith('--')]
    prof = '--prof' in sys.argv
    every = '--all' in sys.argv
    d, name, fa, fb, out = Path(a[0]).resolve(), a[1], int(a[2]), int(a[3]), a[4]
    lines = (d / f'{name}.json').read_text().split('\n')
    keys = [json.loads(ln.rstrip(','))['keys'] for ln in lines[1:] if ln.startswith('{')]
    st = d / 'states' / f'{name}.ss'
    if not st.exists():
        st = Path(__file__).resolve().parents[3] / 'work' / 'segments' / f'{name}.ss'
    e = start_emu(str(st))
    for k in range(fa):
        e.set_keys(keys[k])
        e.run(1)
    f0 = e.frame
    cur = fa
    e.set_keys(keys[cur])
    fstart = {cur: e.cycles()}
    res = []
    upd = None
    prev_hit = False
    it = None            # the running iteration: {'f0', 'irq', 'prof'}
    start_next = None    # the frame the next iteration starts in
    while e.frame - f0 < fb - fa:
        raw = e.reg(15) & ~1
        mode = e.reg(16) & 0x1f
        c = e.cycles()
        hit = UPDATE in (raw, raw - 2, raw - 4)
        new = hit and not prev_hit
        prev_hit = hit
        if it is None and start_next is not None and cur >= start_next:
            it = {'f0': start_next, 'irq': [], 'keys': keys[start_next] if start_next < len(keys) else 0}
            if prof:
                it['prof'] = collections.Counter()
        if new and upd is None:
            n = (e.r32(OBJ + 4) - e.r32(OBJ + 8)) & 1023
            upd = [c, n, mode]
        e.step()
        c2 = e.cycles()
        if prof and it is not None and mode == 0x10 and upd is None:
            it['prof'][raw >> 6 << 6] += c2 - c
        if upd is not None and e.r32(OBJ) == 0 and c2 - upd[0] > 40:
            if upd[2] == 0x10:
                if it is not None:
                    base = fstart[it['f0']]
                    it['pre'] = upd[0] - base
                    it['own'] = it['pre'] - sum(x[2] for x in it['irq'])
                    it['post'] = [upd[1], c2 - upd[0]]
                    it['end'] = c2 - base
                    it['frames'] = cur - it['f0'] + 1
                    if prof:
                        it['prof'] = {hex(k): v for k, v in sorted(it['prof'].items())}
                    if every or it['frames'] > 1:
                        res.append(it)
                    it = None
                start_next = cur + 1
            elif it is not None:
                it['irq'].append([upd[0] - fstart[it['f0']], upd[1], c2 - upd[0]])
            upd = None
        fr = e.frame - f0 + fa
        if fr != cur:
            cur = fr
            fstart[cur] = c2
            if cur < len(keys):
                e.set_keys(keys[cur])
    json.dump(res, open(out, 'w'))
    for r in res:
        print(r['f0'], 'k%x' % r['keys'], 'pre', r['pre'], 'own', r['own'], 'irq', r['irq'], 'post', r['post'], 'end', r['end'], 'frames', r['frames'])


if __name__ == '__main__':
    main()
