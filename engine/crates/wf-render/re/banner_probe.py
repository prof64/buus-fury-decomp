"""Area-name banner probe (headless, muted): find the EnterArea name-id word (ws+8) and the banner's lifetime.

  .venv/bin/python engine/crates/wf-render/re/banner_probe.py
"""
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[4]
sys.path.insert(0, str(ROOT / 're' / 'buu'))
from common import emu, KEY  # noqa: E402

PLAYER = 0x03001C34
e = emu('buu_swforest3.ss')
bp = e.breakpoint(0x08008E4E)
p = e.r32(PLAYER)
e.w32(p + 0x1A0, 430 << 8)
e.w32(p + 0x1A4, 515 << 8)
for f in range(200):
    e.hold(KEY.DOWN, 1)
    h = e.hits()
    if h:
        regs = h[0]['regs']
        r4 = regs[4] & 0xFFFFFFFF
        print('frame', f, 'r4', hex(r4), 'ws+8', e.r16(r4 + 8), 'r6 (new name id)', regs[6])
        break
p = e.r32(PLAYER)
print('after', e.r32(p + 0x1A0) >> 8, e.r32(p + 0x1A4) >> 8, 'frame', e.frame)
e.screenshot('/tmp/bprobe.png')
