"""Decode boss on-death cutscenes (spawn objdef +0x14 = [0x080106b3, sequence*]) into key effects."""
from en_lib import *
from bs_vm import cutscene
import json, re
MAPS=json.load(open(ROOT+'data/log2/maps.json'))['maps']
def obj_class(mi, idx):
    objs=MAPS[mi]['objects']
    if idx<len(objs): return objs[idx].get('class_row'), objs[idx]['kind']
    return None, None
def effects(seq, mi):
    out=[]; texts=[]
    cur_map=mi
    for s in cutscene(seq):
        if 'text' in s: texts.append(s['text'])
        sc=s.get('script','')
        for m in re.finditer(r'\[(\d+),(\d+),(\d+),(\d+),(\d+)\] b0x2[67]\(\)',sc):
            z,a,ent,x,y=map(int,m.groups())
            idx=[k for k,mm in enumerate(MAPS) if mm['zone']==z and mm['area']==a]
            cur_map=idx[0] if idx else cur_map
            out.append(dict(op='change_map',zone=z,area=a,map_index=cur_map,entry=ent,x=x,y=y))
        for m in re.finditer(r'(\d+) spawnMapObject\(listB idx\)\(\)',sc):
            i=int(m.group(1)); c,k=obj_class(cur_map,i)
            out.append(dict(op='spawn_map_object',map_index=cur_map,object_index=i,class_row=c,kind=k))
        for m in re.finditer(r'\[(\d+),(\d+)\] replaceActorWithMapObject',sc):
            a,i=map(int,m.groups()); c,k=obj_class(cur_map,i)
            out.append(dict(op='actor_becomes_map_object',actor=a,map_index=cur_map,object_index=i,class_row=c))
        for m in re.finditer(r'(\d+) giveExp',sc): out.append(dict(op='give_exp',amount=int(m.group(1))))
        for m in re.finditer(r'(\d+) b0x46\(\)',sc): out.append(dict(op='set_active_character',index=int(m.group(1))))
        for m in re.finditer(r'(\d+) b0x1b\(\)',sc): out.append(dict(op='set_flag',flag=int(m.group(1))))
        if 'b0x58()' in sc: out.append(dict(op='broadcast_msg_0x10'))
        if 'player.maxHP() store[0xd]' in sc: out.append(dict(op='refill_player_hp'))
        if 'player.maxEP() store[0xf]' in sc: out.append(dict(op='refill_player_ep'))
    return out, texts
if __name__=='__main__':
    B={3,4,5,6,7,21,22,23,24,25,26,29,44,60,64,76,78,79,98,101,106,41,74}
    for m in MAPS:
        for k,o in enumerate(m['objects']):
            if o['kind']=='enemy' and o['class_row'] in B and o['death_event']['fn']=='0x80106b3':
                ef,tx=effects(int(o['death_event']['script'],16), m['index'])
                print(f"map {m['index']} obj {k} class {o['class_row']} death seq {o['death_event']['script']}:")
                for e in ef: print('    ',e)
                print('     text:', ' | '.join(t[:50] for t in tx[:4]))
