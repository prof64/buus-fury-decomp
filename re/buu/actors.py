"""Enumerate actor placement records of every area."""
from collections import Counter, defaultdict
from common import *
from strtab import S
T = 0x0808e2e0
N = 0x1c4


def areas():
    for i in range(N):
        a = T + 0x38 * i
        yield i, a


def actor_list(a):
    n = u32(a + 4)
    lst = u32(a + 0x14)
    out = []
    if lst and n < 400:
        for k in range(n):
            out.append(u32(lst + 4 * k))
    return out


if __name__ == '__main__':
    import sys
    ctors = Counter()
    data = defaultdict(set)
    for i, a in areas():
        for rec in actor_list(a):
            if not (0x08000000 <= rec < 0x08800000):
                continue
            c = u32(rec)
            ctors[c] += 1
            data[c].add(u32(rec + 0xc))
    for c, n in ctors.most_common():
        print(f'{c:08x} n={n} datas={len(data[c])} e.g. {[hex(x) for x in sorted(data[c])[:6]]}')
