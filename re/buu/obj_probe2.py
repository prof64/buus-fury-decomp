from common import *
e = emu('buu_snakeway_start.ss')
T = 0x0203DFA4
print([hex(e.r32(T + 4 * i)) for i in range(8)])
e.watch(T + 4, T + 0xc, 'r')
e.run(2)
seen = set()
for h in e.hits():
    k = (h['pc'])
    if k in seen:
        continue
    seen.add(k)
    print(hex(h['addr']), 'pc', hex(h['pc']), 'lr', hex(h['lr']), [hex(x) for x in h['regs'][:8]])
