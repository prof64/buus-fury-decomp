"""Boss/enemy arena harness: force-spawn any enemy class on a quiet map via the debug Map Test.
Patches (in the emulated cartridge) map 13's first enemy objdef (0x083ee440) to the wanted class/gfx with spawn
condition 0, disables the other 7 spawns (condition 1), warps to map 13, and provides per-frame monitoring:
AI state stack, positions, player HP, live enemy-projectile objects (found by vtable in EWRAM)."""
from common import *
from warp import find_menu
from ents import enemies
import math
DEFS=[0x083ee440,0x083ee470,0x083ee4a0,0x083ee4e0,0x083ee524,0x083ee568,0x083ee5ac,0x083ee5f0]
ST_TBL=0x086d5194
PROJ_VT={0x08024cbc:'enemy_ki_blast',0x08024258:'energy_ball',0x08024f0c:'thrown_star',0x08024204:'lob_rock',
         0x080250a8:'boss_proj_dcfc',0x08025124:'boss_proj_ea8c',0x080259c4:'proj_11bc4',0x08024acc:'beam_80143a8',0x08024e5c:'laser_801e224'}
PARTY=0x03000e90
def arena(cls, gfx, x=248, y=240, frames=300, extra_patch=None):
    e=new('log2_debugmenu.ss')
    for d in DEFS[1:]: e.w32(d+4,1)
    d=DEFS[0]
    e.w32(d+4,0); e.w32(d+8,cls); e.w32(d+0xc,(y<<16)|x); e.w32(d+0x10,gfx)
    if extra_patch: extra_patch(e)
    m=find_menu(e)
    e.w32(m+8,2); e.w32(m+0x14,13); e.w32(m+0x18,0)
    e.press(KEY.A,3,3); e.run(frames)
    return e
def boss(e, cls):
    for b,c in enemies(e):
        if c==cls and e.r32(b)==0x080252a4: return b
    return None
def center(e,ent):
    return e.r32(ent+0x158)/256+e.r8(ent+0x154)/2, e.r32(ent+0x15c)/256+e.r8(ent+0x155)/2
def stack(e,ent):
    c=ent+0x19c; n=e.r32(c+4)
    return [(e.r32(c+8+8*i+4)-ST_TBL)//8 for i in range(min(n,5))]
def place(e,P,T,ox,oy):
    tx,ty=center(e,T); px,py=center(e,P)
    e.w32(P+0x158, e.r32(P+0x158)+int(round((tx+ox-px)*256))); e.w32(P+0x15c, e.r32(P+0x15c)+int(round((ty+oy-py)*256)))
def player_rec(e):
    i=e.r8(PARTY); return PARTY+0xc+28*i
def godmode(e,hp=30000):
    r=player_rec(e); e.w16(r,hp); e.w16(r+2,hp)
def projectiles(e):
    d=e.ewram(); out=[]
    for vt,name in PROJ_VT.items():
        pat=struct.pack('<I',vt); i=d.find(pat)
        while i!=-1:
            if i%4==0:
                a=0x02000000+i
                out.append((a,name,e.r32(a+0x20)/256,e.r32(a+0x24)/256,
                            [ (lambda v: v-(1<<32) if v>>31 else v)(e.r32(a+0x28+4*k)) for k in range(4)]))
            i=d.find(pat,i+1)
    return out
