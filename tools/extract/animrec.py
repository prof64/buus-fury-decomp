"""Dynamic animation recorder (LoG2 / Buu): hooks the engine's per-entity frame-draw routine and logs
(entity, def-or-anim, anim index, direction, frame index) every video frame. Used to measure frame
durations and to label animations semantically.

LoG2: entity in r0 at 0x08007A84; +0x48 def, +0x4C current direction-list table (def+0x4C+16*anim),
      +0x10 u8 direction, +0x14 u16 frame index.
Buu:  entity in r4 at 0x080101A0; +0x50 current anim struct, +0x10 u8 direction, +0x11 u8 frame index
      (frame pointer = anim + 4 + 16*frame + 4*dir).
"""
import sys
from pathlib import Path
ROOT = Path(__file__).resolve().parent.parent.parent
sys.path.insert(0, str(ROOT / 'tools' / 'emu'))
from emu import Emu, KEY  # noqa

HOOK = {
    'log2': {'pc': 0x8007A84, 'reg': 0, 'def': 0x48, 'anim': 0x4C, 'dir': 0x10, 'frame': 0x14, 'fw': 2,
             'anim_base': 0x4C, 'stride': 16},
    'buu': {'pc': 0x80101A0, 'reg': 4, 'def': None, 'anim': 0x50, 'dir': 0x10, 'frame': 0x11, 'fw': 1},
}


class Recorder:
    def __init__(self, game, state=None, emu=None):
        self.game = game
        self.e = emu or Emu(game)
        if state:
            self.e.load_file(state)
        self.H = HOOK.get(game)
        self.bp = self.e.breakpoint(self.H['pc']) if self.H else None
        self.log = []  # (frame, ent, def|anim, anim_idx, dir, fidx)

    def snap_log1(self):
        """LoG1 (re/log1/notes.md 1.3): live entity list at 0x03000610 (u32 count, pointers); entity +0x08 anim
        row (= def.anim + state*16), +0x12 frame index, +0x13 facing, +0x1C sprite def."""
        e = self.e
        n = e.r32(0x03000610)
        f = e.frame
        for i in range(min(n, 0x60)):
            ent = e.r32(0x03000614 + 4 * i)
            if not (0x02000000 <= ent < 0x02040000):
                continue
            d = e.r32(ent + 0x1C)
            row = e.r32(ent + 0x08)
            if not (0x08000000 <= d < 0x08800000):
                continue
            ai = (row - e.r32(d + 0x28)) // 16
            self.log.append((f, ent, d, ai, e.r8(ent + 0x13), e.r8(ent + 0x12)))

    def snap(self):
        if self.game == 'log1':
            return self.snap_log1()
        e, H = self.e, self.H
        seen = set()
        for h in e.hits():
            ent = h['regs'][H['reg']]
            if ent in seen or not (0x02000000 <= ent < 0x03008000):
                continue
            seen.add(ent)
            ap = e.r32(ent + H['anim'])
            fi = e.r16(ent + H['frame']) if H['fw'] == 2 else e.r8(ent + H['frame'])
            if H['def'] is not None:
                d = e.r32(ent + H['def'])
                ai = (ap - d - H['anim_base']) // H['stride'] if d else -1
            else:
                d, ai = ap, -1
            self.log.append((h['frame'], ent, d, ai, e.r8(ent + H['dir']), fi))

    def run(self, frames, keys=0):
        for _ in range(frames):
            self.e.set_keys(keys)
            self.e.run(1)
            self.snap()

    def press(self, keys, hold=3, after=30):
        self.run(hold, keys)
        self.run(after, 0)

    def clear(self):
        if self.bp is not None:
            self.e.clear(self.bp)


def segments(log):
    """Group log into per-entity runs of the same (def, anim, dir) with frame index durations."""
    by = {}
    for f, ent, d, ai, dr, fi in log:
        by.setdefault(ent, []).append((f, d, ai, dr, fi))
    out = []
    for ent, rows in by.items():
        cur = None
        for f, d, ai, dr, fi in rows:
            k = (d, ai, dr)
            if cur is None or cur['key'] != k or f != cur['last'] + 1:
                if cur:
                    out.append(cur)
                cur = {'ent': ent, 'key': k, 'start': f, 'last': f, 'seq': [[fi, 1]]}
            else:
                if cur['seq'][-1][0] == fi:
                    cur['seq'][-1][1] += 1
                else:
                    cur['seq'].append([fi, 1])
                cur['last'] = f
        if cur:
            out.append(cur)
    return out
