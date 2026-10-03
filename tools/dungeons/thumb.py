"""thumb.py ADDR [N]: Thumb disassembly of N instructions at a ROM address (re/buu/common.dis);
thumb.py -w ADDR N: N u32 words at a ROM address."""
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / 're' / 'buu'))
from common import dis, u32  # noqa: E402

if sys.argv[1] == '-w':
    a = int(sys.argv[2], 16)
    for i in range(int(sys.argv[3])):
        print(f'{a + 4 * i:08x}: {u32(a + 4 * i):08x}')
else:
    print(dis(int(sys.argv[1], 16), int(sys.argv[2]) if len(sys.argv) > 2 else 40))
