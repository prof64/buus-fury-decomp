"""Extract LoG2 map table (0x08409368, 327 x 0x38) + every object spawn list -> data/log2/maps.json.
Map record layout (VERIFIED by map loader 0x0800c390/0x0800c664 and lookup 0x0800eb7c; name id VERIFIED against
in-game banners for maps 0 and 12):
  +0  u8 zone, +1 u8 area   (lookup key used by warps/debug Map Test: 0x0800eb7c scans for (zone,area))
  +2  u8 ?  (passed to 0x0800eb20/0x0800eb4c: variation/entry index)
  +3  u8 count of list@+0x10 (conditional objects: 8-byte [condition, objdef*], condition evaluated by 0x08003f06)
  +4  u8 count of list@+0x14 (object/spawn definitions: ptrs to objdefs)
  +5  u8 count of list@+0x18 ; +6 u8 count of list@+0x1c ; +7 u8 count of list@+0x20
  +8  u32 flag ; +0xc u32 location-name string id (index into 0x080e2840 string table) or ptr
  +0x24 u32 (music/ambience id? INFERRED) ; +0x28.. ptrs (scripts / tile data, INFERRED)
Enemy spawn objdef (factory 0x0800e77f; VERIFIED via 0x0800e280):
  +0 factory, +4 spawn condition word (0 = always), +8 class row index (0x086d56ac), +0xc s16 x,s16 y (px),
  +0x10 gfx/actor id (0x08009324; >=7 -> ptr table 0x083b4e74), +0x14 ptr on-death event node [fn,script]
  (fn 0x0801069b runs the event script with 0x080092e4; death position in 0x03001fd0), +0x18 waypoint count,
  +0x1c.. waypoint ptrs: [factory, s16 x, s16 y, param]; factory 0x0800d625 = walk to (x,y) at param/256 px/frame."""
from en_lib import *
from en_script import drop_rules
import json, os
OUTDIR=ROOT+'data/log2/'
WP_KIND={0x0800d625:'walk_to(x,y) at param/256 px/frame',0x0800d5ff:'waypoint kind 0x0800d5ff (INFERRED: idle/pose at (x,y))',
         0x0800d5b9:'waypoint kind 0x0800d5b9 (INFERRED)',0x0800d0af:'0x0800d0af',0x0800cb0f:'0x0800cb0f',0x08010099:'0x08010099',
         0x0800d0fd:'0x0800d0fd',0x080100bf:'0x080100bf'}
FACT={0x0800e77f:'enemy',0x0800b711:'npc',0x0800d6df:'walker_npc'}
def s16(v): return v-0x10000 if v&0x8000 else v
def loc_name(v):
    if v<0x400:
        p=u32(STR_TBL-0x40+4*v)
        if 0x08000000<=p<0x08800000: return cstr(p)
    return None
def objdef(p):
    w=[u32(p+4*j) for j in range(7)]
    d=dict(addr=hex(p),factory=hex(w[0]),kind=FACT.get(w[0],'other'))
    if w[0]==0x0800e77f:
        d.update(spawn_condition=w[1],class_row=w[2],x=s16(w[3]&0xffff),y=s16(w[3]>>16),gfx_id=w[4])
        dn=w[5]; d['death_event']=dict(node=hex(dn),fn=hex(u32(dn)),script=hex(u32(dn+4)))
        if u32(dn)==0x0801069b:
            try: d['death_event']['drops']=drop_rules(u32(dn+4))
            except Exception as ex: d['death_event']['drops']=None
        wps=[]
        for j in range(w[6]):
            q=u32(p+0x1c+4*j)
            f=u32(q); xy=u32(q+4)
            wps.append(dict(kind=hex(f),desc=WP_KIND.get(f,hex(f)),x=s16(xy&0xffff),y=s16(xy>>16),param=u32(q+8)))
        d['waypoints']=wps
    return d
def maps():
    out=[]
    for i in range(N_MAP):
        a=MAP_TBL+0x38*i; o=a&0x1ffffff
        h=ROM[o:o+8]; f=struct.unpack_from('<12I',ROM,o+8)
        m=dict(index=i,addr=hex(a),zone=h[0],area=h[1],hdr=list(h),flag=f[0],name_id=f[1],name=loc_name(f[1]),
               raw=[hex(x) for x in f])
        objs=[]
        if f[3]:
            for k in range(h[4]): objs.append(objdef(u32(f[3]+4*k)))
        m['objects']=objs
        cond=[]
        if f[2]:
            for k in range(h[3]):
                c=u32(f[2]+8*k); p=u32(f[2]+8*k+4)
                cond.append(dict(condition=hex(c),objdef=hex(p)))
        m['conditional_objects']=cond
        m['enemy_spawns']=[o_ for o_ in objs if o_['kind']=='enemy']
        out.append(m)
    return out
if __name__=='__main__':
    os.makedirs(OUTDIR,exist_ok=True)
    ms=maps()
    doc=dict(source='roms/log2.gba', table=hex(MAP_TBL), count=N_MAP, record_size=0x38,
             notes=__doc__, maps=ms)
    json.dump(doc,open(OUTDIR+'maps.json','w'),indent=1)
    print('maps',len(ms),'enemy spawns',sum(len(m['enemy_spawns']) for m in ms))
