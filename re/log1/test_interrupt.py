"""Does being hit interrupt the player's ki charge? Hold B next to a wolf and watch player entity state."""
from game import *

e = emu('log1_wolf.ss')
pc = player_char(e)
e.w32(pc + 0x4c, 5000); e.w32(pc + 0x50, 5000); e.w32(pc + 0x58, 1000); e.w32(pc + 0x54, 1000)
e.hold(KEY.UP, 18)
ent = e.r32(0x030007B8)
e.breakpoint(0x08007416)
prev = None
for f in range(400):
    e.set_keys(KEY.B)
    e.run(1)
    st = (e.r32(ent + 0x34) - e.r32(ent + 0x18)) // 20
    hits = [h for h in e.hits() if h['regs'][1] == pc]
    if st != prev or hits:
        print(e.frame, 'player entity state', st, 'ki', e.r32(pc + 0x54), 'hit' if hits else '', 'hp', e.r32(pc + 0x4c))
        prev = st
