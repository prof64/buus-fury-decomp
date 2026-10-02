"""Survey area records -> map headers -> layer resources."""
import json
from collections import Counter
from r import R, u8, u16, u32
T = 0x0808E2E0
hdrs = {}
c_vt = Counter()
c_3c = Counter()
c_cnt = Counter()
areas = []
for i in range(0x1C4):
    a = T + 0x38 * i
    w, m = u8(a), u8(a + 1)
    nvar = u8(a + 2)
    lst = u32(a + 0x2C)
    hs = [u32(lst + 4 * k) for k in range(nvar)] if R.isptr(lst) else []
    areas.append(((w, m), nvar, hs))
    for h in hs:
        hdrs.setdefault(h, []).append((w, m))
print('areas', len(areas), 'distinct headers', len(hdrs))
print('nvar', Counter(a[1] for a in areas))
for h in hdrs:
    c_3c[u32(h + 0x3C)] += 1
    c_cnt[u32(h + 0xC)] += 1
    for bg in range(4):
        res = u32(h + 0x14 + 4 * bg)
        c_vt[u32(res) if R.isptr(res) else res] += 1
print('mode +3c', c_3c)
print('anim count +c', c_cnt)
print('res vt', c_vt)
print('+24 trig count', Counter(u32(h + 0x24) for h in hdrs))
print('+4c tile tables', Counter(hex(u32(h + 0x4C)) for h in hdrs))
print('+30', Counter(hex(u32(h + 0x30)) for h in hdrs).most_common(5))
print('+40', Counter(hex(u32(h + 0x40)) for h in hdrs).most_common(5))
print('+50', Counter(hex(u32(h + 0x50)) for h in hdrs).most_common(5))
dup = [(k, v) for k, v in hdrs.items() if len(v) > 1]
print('shared headers', len(dup), dup[:5])
json.dump({hex(k): v for k, v in hdrs.items()}, open('out/headers.json', 'w'))
