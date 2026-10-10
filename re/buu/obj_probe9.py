"""enter the world map 0-50 via the 8-1 capsule trigger (A) and look at the player object / altitude"""
from common import *
from warp import warp
import obj_list
e = emu('buu_snakeway_start.ss')
ok = warp(e, 8, 1, 280, 290)
P = e.r32(0x03001c34)
print('warp', ok, 'area', hex(e.r32(0x03001980)), 'pos', e.r32(P + 0x1a0) >> 8, e.r32(P + 0x1a4) >> 8)
e.save_file(WORK / 'obj_8_1.ss')
obj_list.dump(e)
