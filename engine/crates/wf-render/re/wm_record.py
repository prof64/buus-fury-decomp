"""Per-frame world-map recording for `cargo run -p wf-app --example mode7_check` (headless, muted emulator):
the map object 0x030019A4 (camera, rotation, matrix, far, bbox, ...), the BG2 scanline tables, the shadow OAM and
the flyer, over a scripted flight from work/buu/wm_start.ss.

  .venv/bin/python engine/crates/wf-render/re/wm_record.py OUT.json
"""
import json
import struct
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[4]
sys.path.insert(0, str(ROOT / 're' / 'buu'))
from common import emu, KEY  # noqa: E402

M = 0x030019A4
SO = 0x03003498


def s32(v):
    return v - (1 << 32) if v & 0x80000000 else v


e = emu(str(ROOT / 'work' / 'buu' / 'wm_start.ss'))
script = [(0, 3), (KEY.UP, 40), (KEY.UP | KEY.RIGHT, 90), (KEY.UP, 20), (KEY.UP | KEY.LEFT, 120), (KEY.B, 40),
          (KEY.UP | KEY.RIGHT, 60), (KEY.A, 60), (KEY.DOWN | KEY.LEFT, 50), (KEY.UP, 200), (KEY.UP | KEY.LEFT, 200)]
rows = []
for keys, n in script:
    for _ in range(n):
        e.set_keys(keys)
        e.run(1)
        m = [s32(e.r32(M + 4 * i)) for i in range(0x30)]
        t = e.r32(M + 0xA8)
        tab = []
        for v in range(160):
            pa, pb, pc, pd = struct.unpack('<hhhh', e.read(t + 16 * v, 8))
            x, y = struct.unpack('<ii', e.read(t + 16 * v + 8, 8))
            tab.append([pa, pb, pc, pd, x, y])
        p = e.r32(0x03001C34)
        fl = [s32(e.r32(p + o)) for o in (0x1A0, 0x1A4, 0x1A8, 0x1AC)] + [e.r8(p + 0x10), e.r8(p + 0x11)]
        rows.append({'m': m, 'tabs': [tab], 'oam': e.read(SO, 1024).hex(), 'fl': fl})
Path(sys.argv[1] if len(sys.argv) > 1 else '/tmp/wm_record.json').write_text(json.dumps(rows))
print(len(rows), 'frames')
