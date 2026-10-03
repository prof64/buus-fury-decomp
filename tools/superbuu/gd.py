"""GameData dump helpers"""
import sys
sys.path.insert(0,str(__import__('pathlib').Path(__file__).resolve().parents[2] / 're' / 'buu'))
from common import emu
GD=0x03001068
def dump(e):
    g=e.read(GD,0x8A8)
    flags=g[0x55:0x55+143]
    fl=[i for i in range(143*8) if flags[i>>3]>>(i&7)&1]
    print('cur',g[0],'b1',hex(g[1]),'zenie',int.from_bytes(g[0x4F4:0x4F8],'little'),'loc',list(g[0x8A0:0x8A3]),int.from_bytes(g[0x8A4:0x8A6],'little'),int.from_bytes(g[0x8A6:0x8A8],'little'))
    print('flags',fl)
    print('supplies',{i:g[2+i] for i in range(83) if g[2+i]})
    n=int.from_bytes(g[0xE4:0xE8],'little'); print('equip n',n,[hex(int.from_bytes(g[0xE8+4*i:0xEC+4*i],'little')) for i in range(min(n,40))])
    for c in range(9):
        r=g[0x4F8+0x68*c:0x4F8+0x68*(c+1)]
        u16=lambda o:int.from_bytes(r[o:o+2],'little')
        print(c,'b0',hex(r[0]),'b1',hex(r[1]),'fus',r[2:4].hex(),'nsk',r[0xC],'sk',list(r[0xD:0x14]),'form',r[0x14],'sel',r[0x15],'hp',u16(0x16),u16(0x18),'ep',u16(0x1A),u16(0x1C),'xp',int.from_bytes(r[0x24:0x28],'little'),'base',list(r[0x28:0x2C]),'pts',r[0x2C],'sklv',list(r[0x2E:0x3A]),'eq',[hex(int.from_bytes(r[o:o+4],'little')) for o in (0x3C,0x40,0x44,0x48)])
if __name__=='__main__':
    for s in sys.argv[1:]:
        print('=====',s); dump(emu(s))
