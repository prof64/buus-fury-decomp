"""Warp to a map, compare palette RAM with candidate ROM palettes."""
import sys, struct
sys.path.insert(0, '/Users/prof/source/buus-fury-decomp/tools/extract')
from emu_states import buu_warp
from r import R
world, area = int(sys.argv[1]), int(sys.argv[2])
e, ok = buu_warp(world, area)
pr = e.palram()
pal = [struct.unpack_from('<H', pr, 2 * i)[0] for i in range(512)]
print('ok', ok)
print('bg', ' '.join(f'{c:04x}' for c in pal[:32]))
print('bg240', ' '.join(f'{c:04x}' for c in pal[240:256]))
print('obj', ' '.join(f'{c:04x}' for c in pal[256:272]))
rp = R.bytes(0x080701b0, 48)
print('rom', rp.hex())
# search ROM for the BG palette 16..32 as BGR555
needle = pr[2:64]
i = R.d.find(needle)
print('bgr555 match at', hex(i + 0x08000000) if i >= 0 else None)
open('out/pal_%d_%d.bin' % (world, area), 'wb').write(pr)
