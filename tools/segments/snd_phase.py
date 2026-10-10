"""snd_phase.py SEGMENT.json [...]: add `init.snd_phase` to segment files.

The CPU cycles from the segment's start (its savestate, `source.state`) to the next sound-DMA completion interrupt
(the play position 0x0300389C advances by 16 every 16,800 cycles). RAM alone does not carry this timer phase; the
core's sound-timing model (engine/crates/wf-core/src/sndsim.rs) needs it to place the mixer's work in the frame.
Headless, muted; rewrites the file's first line in place."""
import json, re, sys
from pathlib import Path
ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'tools' / 'story_playthrough')); sys.path.insert(0, str(ROOT / 'tools' / 'field'))
from route import start_emu

POS = 0x0300389C


def phase(state):
    e = start_emu(str(state))
    c0 = e.cycles()
    p0 = e.r32(POS)
    for _ in range(200000):
        e.step()
        if e.r32(POS) != p0:
            return e.cycles() - c0
    raise SystemExit(f'{state}: no DMA interrupt')


def main():
    starts = {}
    for f in sys.argv[1:]:
        p = Path(f)
        s = p.read_text()
        nl = s.index('\n')
        head = s[:nl]
        m = re.search(r'"state":"([^"]+)"', head)
        r = re.search(r'"kind":"route","run":"([^"]+)","frame":(\d+)', head)
        if m and (ROOT / m.group(1)).exists():
            ph = phase(ROOT / m.group(1))
        elif r and (ROOT / r.group(1) / 'start.ss').exists():
            # a cut of a route run: the interrupt cadence is pure arithmetic from the run's start (one frame =
            # 280,896 cycles = 16 periods + 12,096; verified on the 90 Earth cuts to +-45 cycles)
            run = r.group(1)
            if run not in starts:
                starts[run] = phase(ROOT / run / 'start.ss')
            ph = (starts[run] - int(r.group(2)) * 12096) % 16800
        else:
            print(f, 'no savestate (source.state / source.run)'); continue
        head = re.sub(r'"snd_phase":\d+,', '', head)
        head = head.replace('"init":{', '"init":{"snd_phase":%d,' % ph, 1)
        p.write_text(head + s[nl:])
        print(f, ph)


main()
