"""script_text_codes.py -- survey non-printable UTF-16 units in the dialog banks (0x087B5B64, 7 LZ banks) and the
string table (0x0806BCE8): counts and example contexts per code. Output work/buu/script/text_codes.txt."""
import struct
from collections import Counter, defaultdict
from common import *
from script_nat_c_text import lz
from strtab import table, ustr

BANKS = 0x087B5B64
cnt = Counter()
ex = defaultdict(list)


def scan(units, src):
    for i, c in enumerate(units):
        if 0x20 <= c < 0x7f or 0xa0 <= c < 0x100:
            continue
        key = c if c < 0xE000 else c & 0xFF00
        cnt[(src, c)] += 1
        if len(ex[c]) < 4:
            ctx = ''.join(chr(u) if 0x20 <= u < 0x7f else '{%04x}' % u for u in units[max(0, i - 25):i + 25])
            ex[c].append(f'{src}: ...{ctx}...')


lines = []
for b in range(7):
    d = lz(u32(BANKS + 4 * b))
    units = list(struct.unpack_from(f'<{len(d) // 2}H', d))
    lines.append(f'bank {b}: {len(d)} bytes, {len(units)} units, zero-terminators {units.count(0)}')
    scan(units, f'bank{b}')
for i, p in enumerate(table()):
    units = []
    o = ra(p)
    while True:
        c = struct.unpack_from('<H', rom(), o)[0]
        if c == 0:
            break
        units.append(c)
        o += 2
    scan(units, 'strtab')
codes = sorted({c for (_, c) in cnt})
for c in codes:
    tot = {s: n for (s, cc), n in cnt.items() if cc == c}
    lines.append(f'code {c:04x}: {tot}')
    for x in ex[c]:
        lines.append('    ' + x)
(WORK / 'script' / 'text_codes.txt').write_text('\n'.join(lines))
print('\n'.join(lines[:400]))
