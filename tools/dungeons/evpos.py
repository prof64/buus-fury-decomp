"""evpos.py STATE.ss: where the running event stands at a state (input for importing area entries inside an event).

* the scene objects (`0x0801246E`: +0 header, +8 u16 state, +0x14 param block ptr) found on the IWRAM stack as words
  equal to a scene-table header (0x0879B704) whose next word is a heap result pointer;
* the bytecode interpreters: the context stack 0x03001034 ([0] depth, [1..] interpreter objects); the VM
  0x08004038 pushes {r0 = interpreter, r1 = code pointer, ...} and keeps its PC in that r1 slot ([sp + 4], advanced
  per opcode at 0x08004064), so the PC is the word after a stack word equal to the interpreter pointer;
* for each interpreter: the scene step whose bytecode holds the PC (kind 0: code at record + 3) -> (scene, state,
  offset of the PC in the step's code)."""
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 're' / 'buu' / 'ship'))
import ship_driver  # noqa: E402,F401
from route import start_emu  # noqa: E402

TABLE = 0x0879B704


def lz77(e, src):
    """GBA BIOS LZ77 (type 0x10) at ROM `src` (a pointer to the header when src points at a pointer)"""
    if e.r8(src) != 0x10:
        src = e.r32(src)
    n = e.r32(src) >> 8
    out = bytearray()
    i = src + 4
    while len(out) < n:
        flags = e.r8(i)
        i += 1
        for b in range(8):
            if len(out) >= n:
                break
            if flags & (0x80 >> b):
                x, y = e.r8(i), e.r8(i + 1)
                i += 2
                ln = (x >> 4) + 3
                d = ((x & 0xF) << 8 | y) + 1
                for _ in range(ln):
                    out.append(out[-d])
            else:
                out.append(e.r8(i))
                i += 1
    return bytes(out[:n])


def scan(e):
    hdr = {}
    for k in range(1200):
        h = e.r32(TABLE + 4 * k)
        if 0x08000000 <= h < 0x0A000000:
            hdr.setdefault(h, k)
    scenes = []
    for a in range(0x03007000, 0x03008000, 4):
        v = e.r32(a)
        if v in hdr and e.r32(a + 0x14) >> 24 in (2, 3):
            scenes.append((a, hdr[v], v, e.r16(a + 8)))
    interps = [e.r32(0x03001034 + 4 * i) for i in range(1, 1 + min(e.r32(0x03001034), 8))]
    pcs = []
    for it in interps:
        for a in range(0x03007000, 0x03008000, 4):
            if e.r32(a) == it and (0x08000000 <= e.r32(a + 4) < 0x0A000000 or 0x02000000 <= e.r32(a + 4) < 0x02040000):
                pcs.append((it, a, e.r32(a + 4)))
    out = []
    for it, a, pc in pcs:
        best = None
        for sa, sid, h, st in scenes:
            for s in range(64):
                r = e.r32(h + 4 * s)
                if r < 0x10000:
                    continue
                kind = e.r8(r + 2)
                if kind == 0 and r + 3 <= pc < r + 3 + 0x800 and (best is None or r > best[3]):
                    best = (sid, s, st, r + 3)
                elif kind == 4 and pc >> 24 == 2:
                    try:
                        code = lz77(e, r + 4)
                    except IndexError:
                        continue
                    # the 0x808-byte heap buffer: the decompressed code just before the PC
                    for base in range(pc, pc - len(code) - 1, -1):
                        if bytes(e.r8(base + i) for i in range(min(16, len(code)))) == code[:16]:
                            best = (sid, s, st, base)
                            break
        out.append((hex(it), hex(a), hex(pc), best and {'scene': best[0], 'step': best[1], 'scene_state': best[2], 'off': pc - best[3]}))
    return scenes, out


if __name__ == '__main__':
    e = start_emu(str(Path(sys.argv[1]).resolve()))
    scenes, out = scan(e)
    print('depth', e.r32(0x03002A5C), 'scenes', [(hex(a), sid, st) for a, sid, _, st in scenes])
    for o in out:
        print('interp', o)
