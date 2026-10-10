import sys
sys.path.insert(0, '/Users/prof/source/buus-fury-decomp/re/buu/menu')
from cap import Cap, program
c = Cap('buu_snakeway_start.ss')
for k in program(['-:2', 'SELECT:1', '-:60']):
    c.step(k, keep=False)
e = c.e
# ctor of the scouter: break there, then watch the list area of the new object
b = e.breakpoint(0x080261EA); e.watch(0x2024ba4 + 0x10, 0x2024ba4 + 0x10 + 40 * 8, "w")
e.set_keys(0x200)
seen = set()
for f in range(40):
    e.run(1)
    e.set_keys(0)
    for h in e.hits():
        if h['kind'] == 'break' and h['addr'] & ~1 == 0x080261EA:
            obj = h['regs'][4]
            print('ctor obj', hex(obj))
            e.watch(obj + 0x10, obj + 0x10 + 40 * 8, 'w')
        elif h['kind'] == 'watch':
            k = (h['pc'], h['addr'] - 0)
            if h['pc'] not in seen:
                seen.add(h['pc'])
                print('write pc', hex(h['pc']), 'lr', hex(h['lr']), hex(h['addr']), h['new'])
