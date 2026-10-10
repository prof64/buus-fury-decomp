"""Shared decoding helpers for LoG2 enemies/AI/maps (owned by the enemies agent)."""
from common import *
CLASS_TBL=0x086d56ac; N_CLASS=109
STATE_TBL=0x086d5194; N_STATE=152
DB_TBL=0x086f8e40; N_DB=128
ATK_TBL=0x086db2cc; N_ATK=16
MAP_TBL=0x08409368; N_MAP=0x147
STR_TBL=0x080e2880
def cstr(a):
    o=a&0x1ffffff; e=ROM.index(b'\0',o); return ROM[o:e].decode('latin1')
def class_row(i):
    o=(CLASS_TBL&0x1ffffff)+24*i
    exp,hp,ai=struct.unpack_from('<3I',ROM,o)
    s,p,en,mm,em=ROM[o+12:o+17]
    death=u32(CLASS_TBL+24*i+20)
    return dict(id=i,addr=hex(CLASS_TBL+24*i),exp=exp,hp=hp,ai_state=ai,STR=s,POW=p,END=en,
                melee_taken_mult=mm,energy_taken_mult=em,pad=ROM[o+17:o+20].hex(),death_node=hex(death))
def db_entries():
    out=[]
    for i in range(N_DB):
        n,d,g,pt,c=struct.unpack_from('<5I',ROM,(DB_TBL&0x1ffffff)+20*i)
        out.append(dict(index=i,addr=hex(DB_TBL+20*i),name=cstr(n),description=cstr(d),gfx_id=g,portrait_id=pt,class_row=c))
    return out
def attack(i):
    a=ATK_TBL+8*i
    return dict(id=i,addr=hex(a),type='melee' if u8(a)==0 else 'energy',type_byte=u8(a),id_byte=u8(a+1),power=u32(a+4))
