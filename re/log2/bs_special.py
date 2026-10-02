"""Force boss special attacks in the arena by patching AI node params in the emulated ROM, then log hits/projectiles.
usage: bs_special.py NAME"""
import bs_arena, bs_watch
import sys
def patch(addrs_vals):
    def f(e):
        for a,v in addrs_vals: e.w32(a,v)
    return f
CFG={
 # Cell (imperfect, ai 80) grab: state 85 IfPlayerWithin(24) r2 @0x086d39f4+0xc -> 0 ; Chance(35%) @0x086d39c8+0xc -> 0xffff
 'cell_grab':(60,28,20,700,[(0x086d39f4+0xc,0),(0x086d39c8+0xc,0xffff)]),
 # Cell Nonperfect (ai 116) beam: state 117 Chance(5%) @0x086d4584+0xc ->0xffff ; state 118 Chance(50%) @0x086d45f0+0xc -> 0xffff
 'cellnp_beam':(76,29,100,700,[(0x086d4584+0xc,0xffff),(0x086d45f0+0xc,0xffff)]),
 'pcell_beam':(21,30,100,700,[(0x086d4d94+0xc,0xffff),(0x086d4e00+0xc,0xffff)]),
 # Tao (ai 129): state 130 Chance(5%) @0x086d4a30+0xc -> 0xffff ; Chance(40%) @0x086d49f4+0xc -> 0xffff
 'tao_special':(44,50,100,700,[(0x086d4a30+0xc,0xffff),(0x086d49f4+0xc,0xffff)]),
 'destroyer_spray':(31,37,60,700,[]),
 'ninja_throw':(75,96,70,500,[(0x086d2f10+0xc,0xffff)]),
 'ladybug_laser':(65,77,40,400,[]),
 'bandit_lob':(9,18,70,500,[(0x086d2c6c+0xc,0xffff)]),
 'hench_ball':(85,141,50,400,[(0x086d2060+0xc,0xffff)]),
 'snake_melee':(53,118,14,400,[(0x086d1f68+0xc,0xffff)]),
 'wolf_melee':(17,146,14,400,[]),
 'trex_melee':(99,122,20,600,[]),
 'hawk':(58,63,40,600,[]),
 'cellnp_grab':(76,29,20,700,[(0x086d47d4+0xc,0),(0x086d47a8+0xc,0xffff)]),
}
if __name__=='__main__':
    name=sys.argv[1]
    cls,gfx,dist,N,pv=CFG[name]
    orig=bs_arena.arena
    bs_watch.arena=lambda c,g,**kw: orig(c,g,extra_patch=patch(pv),**kw)
    r=bs_watch.watch(cls,gfx,dist,N,follow=('grab' in name))
    if r: print('hits',r['hits'])
