"""script_nat_b_probe.py NATIVE arg,arg,... [state] [frames] [name]
Run `push args; native N` through the game's runner from a save state and report: frames until the script
returned (blocking time), the VM stack after, IWRAM/GameData words that changed, player position change,
blend/IO registers, and a before/after screenshot strip (work/buu/script/probe_<name>.png).
Extra natives can be chained with ';' : "0x88:30;0x79:16" (native:args)."""
import sys
from common import *
from script_nat_b_lib import run_code, asm

PLAYER_PTR = 0x03001C34


def parse(spec):
    items = []
    for part in spec.split(';'):
        n, _, a = part.partition(':')
        for x in [v for v in a.split(',') if v != '']:
            items.append(int(x, 0))
        items.append(('n', int(n, 0)))
    return items


def player_xy(e):
    p = e.r32(PLAYER_PTR)
    if not p:
        return None
    return (e.r32(p + 0x1a0) / 256, e.r32(p + 0x1a4) / 256)


def main():
    spec = sys.argv[1]
    state = sys.argv[2] if len(sys.argv) > 2 else 'buu_snakeway_start.ss'
    frames = int(sys.argv[3]) if len(sys.argv) > 3 else 600
    name = sys.argv[4] if len(sys.argv) > 4 else spec.replace(':', '_').replace(',', '_').replace(';', '__')
    e = emu(state)
    e.run(3)
    from PIL import Image
    shots = [Image.frombytes('RGB', (240, 160), e.pixels())]
    iw0 = e.iwram()
    io0 = e.read(0x04000000, 0x60)
    p0 = player_xy(e)
    marks = []

    def pf(e, f):
        if f in (5, 20, 60, 120, 240):
            shots.append(Image.frombytes('RGB', (240, 160), e.pixels()))
    r = run_code(e, asm(parse(spec)), max_frames=frames, per_frame=pf)
    print('result (frames, sp, stack):', r)
    e.run(2)
    shots.append(Image.frombytes('RGB', (240, 160), e.pixels()))
    iw1 = e.iwram()
    io1 = e.read(0x04000000, 0x60)
    print('player', p0, '->', player_xy(e))
    ch = []
    for i in range(0, 0x8000, 4):
        a, b = iw0[i:i + 4], iw1[i:i + 4]
        if a != b and not (0x7E00 <= i):
            ch.append((0x03000000 + i, int.from_bytes(a, 'little'), int.from_bytes(b, 'little')))
    gd = [c for c in ch if 0x03001000 <= c[0] < 0x03001700]
    print('IWRAM words changed:', len(ch), ' in 0x03001000..0x03001700:')
    for a, x, y in gd[:60]:
        print(f'   {a:08x}: {x:08x} -> {y:08x}')
    for off in (0x0, 0x8, 0xa, 0xc, 0xe, 0x48, 0x4a, 0x50, 0x52, 0x54):
        x = int.from_bytes(io0[off:off + 2], 'little')
        y = int.from_bytes(io1[off:off + 2], 'little')
        if x != y:
            print(f'   IO 0x{off:02x}: {x:04x} -> {y:04x}')
    W = Image.new('RGB', (240 * len(shots), 160))
    for i, im in enumerate(shots):
        W.paste(im, (240 * i, 0))
    p = WORK / 'script' / f'probe_{name}.png'
    W.save(p)
    print('shots', p)


if __name__ == '__main__':
    main()
