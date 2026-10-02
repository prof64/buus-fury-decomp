from common import *
import obj_list
e = emu(WORK / 'obj_capsule.ss')
P = e.r32(0x03001c34)


def st():
    return [e.r32(P + 0x1a0) / 256, e.r32(P + 0x1a4) / 256, (e.r32(P + 0x1a8) - (1 << 32) if e.r32(P + 0x1a8) & 0x80000000 else e.r32(P + 0x1a8)) / 1024, e.r8(P + 0x10)]


print('start', st())
for name, k in (('A', KEY.A), ('B', KEY.B), ('R', KEY.R), ('L', KEY.L), ('UP', KEY.UP), ('LEFT', KEY.LEFT), ('DOWN', KEY.DOWN)):
    s = e.save()
    out = []
    for f in range(90):
        e.set_keys(k); e.run(1)
        if f % 15 == 14:
            out.append(st())
    e.set_keys(0)
    for f in range(60):
        e.run(1)
    out.append(('rel', st()))
    print(name, out)
    e.load(s)
obj_list.dump(e)
