from common import *
e = emu('buu_snakeway_start.ss')
print('ptrs', hex(u32(0x080707e0)), hex(u32(0x080707e4)), hex(e.r32(u32(0x080707e0))), hex(e.r32(u32(0x080707e4))))
for a in (0x080126D4, 0x080127AE):
    e.breakpoint(a)
e.run(3)
seen = set()
for h in e.hits():
    k = (h['addr'], h['lr'])
    if k in seen:
        continue
    seen.add(k)
    print(hex(h['addr']), 'lr', hex(h['lr']), [hex(x) for x in h['regs'][:4]])
p = e.r32(0x03001c34)
vt = e.r32(p)
print('player', hex(p), 'vt', hex(vt))
