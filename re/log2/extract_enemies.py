"""Extract every LoG2 enemy class (class table 0x086d56ac, 109 rows x 24 bytes) + spawns, AI summary, drops,
names and graphics references -> data/log2/enemies.json.

Class row layout (VERIFIED: enemy init 0x0800e280/0x0800f7cc, takeHit 0x0800e485, exp award 0x0800e4d2,
scouter page 0x0801790e):
  +0x00 u32 exp           base exp reward (see exp_award)
  +0x04 u32 hp            max HP (copied to entity+0x188 at spawn)
  +0x08 u32 ai_state      initial AI state (index into state table 0x086d5194; see ai.json)
  +0x0c u8  STR           attacker stat for melee attacks (damage formula 0x0800f910)
  +0x0d u8  POW           attacker stat for energy attacks
  +0x0e u8  END           defence: incoming damage *= (128-END)/128
  +0x0f u8  melee_mult    incoming melee damage *= melee_mult/128
  +0x10 u8  energy_mult   incoming energy damage *= energy_mult/128
  +0x11 3 bytes padding
  +0x14 u32 death_node    node whose fn2 runs on death (0x086d2b6c explode+exp, 0x086ff3c4 exp only, ...)
Entity bytes +0x160..+0x164 receive STR,POW,END,melee_mult,energy_mult (0x0800f7cc)."""
from en_lib import *
from extract_ai import states as ai_states, summarize
from extract_maps import maps as map_list
from en_script import drop_rules
import json, os
OUTDIR=ROOT+'data/log2/'
GFX_PTR_TBL=0x083b4e74
def gfx_ref(g):
    if not (7<=g<=156): return dict(gfx_id=g, descriptor=None)
    d=u32(GFX_PTR_TBL+4*g)
    rects={}
    for k,dn in enumerate(('down','up','left','right')):
        rects[dn]=[s32(d+0xc+16*k+4*j) for j in range(4)]
    return dict(gfx_id=g, descriptor=hex(d), collision_box_wh=[u8(d+3),u8(d+4)],
                hurtbox_by_facing=rects,
                note='descriptor+3/+4 = collision box w,h (copied to entity+0x154/+0x155 by 0x080078e4); descriptor+0xc+16*facing = hurt rect [x0,y0,x1,y1] relative to entity position (0x080079b8)')
DEATH={0x086d2b6c:'DeathExplode: exp award + on-death event + explosion effect + death sfx',
       0x086ff3c4:'exp award + on-death event, no explosion (bosses/scripted)',
       0x086d5654:'DeathExplode variant (param 0x40) (Cell Junior)',
       0x086d5664:'special death node (class 36)',
       0x086d5694:'composite death node (Eggbot robots: two children then explode)'}
FIGHTER_AI={21,29,80,116,129,138,148,98}
def exp_award_note():
    return ('award = class.exp; if class.exp>5: L=active char level (0x08004214); if L==50 -> no exp; '
            'span = expTable[L]-expTable[L-1]; award = clamp(class.exp, span>>7, span>>1). '
            'Queued as world event (vtable 0x080245b0) and added by 0x08003d58. VERIFIED (snake exp 13 at Lv40 gave 4747 = 607735>>7).')
def main():
    st=ai_states()
    ms=map_list()
    db=db_entries()
    spawns={}; drops={}
    for m in ms:
        for s in m['enemy_spawns']:
            spawns.setdefault(s['class_row'],[]).append(dict(map_index=m['index'],zone=m['zone'],area=m['area'],
                map_name=m['name'],x=s['x'],y=s['y'],gfx_id=s['gfx_id'],spawn_condition=s['spawn_condition'],
                death_script=s['death_event']['script'],waypoints=len(s['waypoints']),
                patrol_speeds_px_per_frame=sorted(set(round(w['param']/256,4) for w in s['waypoints'] if w['kind']=='0x800d625'))))
            sc=s['death_event']['script']
            if sc not in drops: drops[sc]=s['death_event'].get('drops')
    out=[]
    for i in range(N_CLASS):
        r=class_row(i)
        names=[d for d in db if d['class_row']==i]
        sp=spawns.get(i,[])
        gfx=sorted(set([d['gfx_id'] for d in names]+[s['gfx_id'] for s in sp]))
        summ=summarize(st, r['ai_state'])
        dscripts={}
        for s in sp: dscripts[s['death_script']]=dscripts.get(s['death_script'],0)+1
        is_boss=(r['ai_state'] in FIGHTER_AI) or (sp and all(s['spawn_condition']==1 for s in sp))
        ent=dict(id=i, addr=r['addr'],
            names=sorted(set(d['name'] for d in names)) or None,
            scouter_entries=[dict(db_index=d['index'],name=d['name'],gfx_id=d['gfx_id'],portrait_id=d['portrait_id'],description=d['description']) for d in names],
            hp=r['hp'], exp=r['exp'], STR=r['STR'], POW=r['POW'], END=r['END'],
            melee_taken_mult=r['melee_taken_mult'], energy_taken_mult=r['energy_taken_mult'],
            damage_taken_note=f"incoming dmg *= (128-{r['END']})/128 then *= {r['melee_taken_mult']}/128 (melee) or {r['energy_taken_mult']}/128 (energy)",
            ai_state=r['ai_state'], ai_summary=summ,
            attacks=summ['attacks'],
            death_node=r['death_node'], death_behaviour=DEATH.get(int(r['death_node'],16),'unknown'),
            graphics=[gfx_ref(g) for g in gfx],
            spawn_count=len(sp), spawns=sp,
            drop_scripts=[dict(script=k,spawn_count=v,drops=drops.get(k)) for k,v in sorted(dscripts.items(),key=lambda kv:-kv[1])],
            is_boss_or_scripted=bool(is_boss),
            used_in_game=bool(sp))
        out.append(ent)
    atk=[attack(i) for i in range(N_ATK)]
    doc=dict(source='roms/log2.gba', class_table=hex(CLASS_TBL), n_classes=N_CLASS, row_size=24,
             layout=__doc__, exp_award=exp_award_note(),
             spawn_condition_semantics=('objdef+4 evaluated by 0x08003f06 at map load (0x0800c526): 0 = always; '
                 'small non-zero (e.g. 1) = never at load (spawned/activated by event scripts: bosses); hi16=1 flag set, '
                 'hi16=2 flag clear, hi16=3 test-and-clear (flag index = lo16 into the story-flag bitfield), larger = script ptr evaluated by the VM.'),
             enemy_attack_table=atk,
             enemy_attack_note=('Enemies use attack table 0x086db2cc ids 1 (energy 1024), 2 (melee 1024), 3 (energy 1024); damage to the player '
                 'uses the shared formula 0x0800f910 with S = enemy STR (melee) or POW (energy) and the player END/multipliers.'),
             scouter_db=hex(DB_TBL), graphics_ptr_table=hex(GFX_PTR_TBL),
             classes=out)
    os.makedirs(OUTDIR,exist_ok=True)
    json.dump(doc,open(OUTDIR+'enemies.json','w'),indent=1)
    print('classes',len(out),'used',sum(e['used_in_game'] for e in out),'boss/scripted',sum(e['is_boss_or_scripted'] for e in out))
if __name__=='__main__': main()
