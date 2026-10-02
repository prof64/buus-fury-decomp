"""Find a word under any constant byte offset (and 16-bit) encoding."""
import sys
from common import rom
r = rom()
word = sys.argv[1].encode()
diffs = [word[i + 1] - word[i] for i in range(len(word) - 1)]
hits = {}
n = len(word)
import numpy as np
a = np.frombuffer(r, dtype=np.uint8).astype(np.int16)
d = a[1:] - a[:-1]
mask = np.ones(len(d) - n, dtype=bool)
for i, df in enumerate(diffs):
    mask &= d[i:len(d) - n + i] == df
idx = np.nonzero(mask)[0]
for i in idx[:50]:
    off = int(a[i]) - word[0]
    print(f'8bit {0x08000000 + i:08x} offset {off}  ctx={r[i-8:i+n+8].hex()}')
print(len(idx), 'hits 8bit')
# 16-bit
a16 = np.frombuffer(r[: len(r) // 2 * 2], dtype=np.uint16).astype(np.int32)
for par in (0,):
    d = a16[1:] - a16[:-1]
    mask = np.ones(len(d) - n, dtype=bool)
    for i, df in enumerate(diffs):
        mask &= d[i:len(d) - n + i] == df
    idx = np.nonzero(mask)[0]
    for i in idx[:20]:
        print(f'16bit {0x08000000 + 2*i:08x} first={a16[i]:04x}')
    print(len(idx), 'hits 16bit')
