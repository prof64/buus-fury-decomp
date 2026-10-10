"""Extract LoG2 boss fights -> data/log2/bosses.json.

Sources (all from ROM unless noted):
 * class rows (0x086d56ac), AI state machines (0x086d5194, summarized with extract_ai.summarize),
 * spawn defs in the map table (extract_maps) incl. spawn condition (1 = only activated by a cutscene),
 * on-death cutscene sequences (spawn def +0x14 = [0x080106b3, sequence*]; engine 0x0800b4ae, decoded with bs_vm/bs_chain):
   they reveal the fight chain (next phase = spawnMapObject / replaceActorWithMapObject), exp rewards, active-character
   switches, HP/EP refills and story flags,
 * boss special-attack catalog (enemy_attacks from extract_enemies.ATTACKS),
 * emulator measurements in re/log2/bs_measure_results.json (produced by bs_measure.py: force-spawned boss on a quiet
   map, Lv40 Piccolo END 53 standing still at 30 / 100 px; 900 frames). Damage numbers depend on that test character.

Phase model (VERIFIED by disassembly): no AI node reads the boss's HP, so there are no mid-fight HP-threshold
phases inside one class row. A multi-phase boss is a chain of class rows/spawns: when a phase's HP reaches 0 the
death cutscene runs and spawns (or converts a cutscene actor into) the next spawn def."""
from en_lib import *
from extract_ai import states as ai_states, summarize
from extract_maps import maps as map_list
from en_attacks import ATTACKS, AI_SPECIALS
from bs_chain import effects
import json, os
OUTDIR=ROOT+'data/log2/'
MEAS=os.path.join(os.path.dirname(os.path.abspath(__file__)),'bs_measure_results.json')
CHARS=['Gohan','Piccolo','Vegeta','Trunks','Goku','Hercule']
FIGHTS=[
 dict(key='frieza_dream',name='Frieza (Gohan\'s nightmare, tutorial)',classes=[41],maps=[14],
      notes='Class 41 uses gfx 48 (Frieza). Very low stats (HP 130); death cutscene: "The phone is ringing..." -> Gohan wakes up (map 19).'),
 dict(key='general_tao',name='General Tao',classes=[44],maps=[46]),
 dict(key='triceratops_king',name='Triceratops King',classes=[101],maps=[105],notes='Takes half damage from melee and energy (64/128).'),
 dict(key='newman',name='Newman (Capsule Corp. intruder, uses wolf row 74 / gfx 146)',classes=[74],maps=[155]),
 dict(key='cooler',name='Cooler',classes=[29],maps=[172]),
 dict(key='sparring',name='Sparring Arena: Yamcha / Krillin / Tien',classes=[106,64,98],maps=[175],
      notes='Three separate spawn defs (one per opponent), rewards: +3 Power / Strength / Endurance capsule.'),
 dict(key='android_19',name='Android 19',classes=[5],maps=[191]),
 dict(key='android_20',name='Android 20 (Dr. Gero)',classes=[6,7],maps=[195,215,229,242]),
 dict(key='android_18',name='Android 18',classes=[4],maps=[251]),
 dict(key='cell_imperfect',name='Cell (Imperfect)',classes=[60],maps=[287]),
 dict(key='android_17',name='Android 17',classes=[3],maps=[296,297,298]),
 dict(key='cell_semiperfect',name='Cell (Semi-perfect / "Nonperfect") -> Perfect Cell (Tropical Islands)',classes=[76,78,79],maps=[302,304]),
 dict(key='cell_games',name='Cell Games',classes=[21,22,23,26,24,25],maps=[324,325]),
]
def main():
    st=ai_states(); ms=map_list(); db=db_entries()
    meas=json.load(open(MEAS)) if os.path.exists(MEAS) else {}
    out=[]
    for F in FIGHTS:
        phases=[]
        for mi in F['maps']:
            m=ms[mi]
            for k,o in enumerate(m['objects']):
                if o['kind']!='enemy' or o['class_row'] not in F['classes']: continue
                c=o['class_row']; r=class_row(c)
                summ=summarize(st, r['ai_state'])
                de=o['death_event']
                eff,texts=([],[])
                if de['fn']=='0x80106b3': eff,texts=effects(int(de['script'],16), mi)
                elif de['fn']=='0x801069b': eff=[dict(op='event_script',script=de['script'])]
                nxt=[e for e in eff if e['op'] in ('spawn_map_object','actor_becomes_map_object') and e.get('class_row') is not None]
                specials=[AI_SPECIALS[s] for s in summ['states'] if s in AI_SPECIALS]
                m_ = {kk:v for kk,v in meas.items() if v['cls']==c}
                phases.append(dict(
                    class_row=c, names=sorted(set(d['name'] for d in db if d['class_row']==c)) or None,
                    map_index=mi, map_name=m['name'], zone=m['zone'], area=m['area'], object_index=k, spawn_def=o['addr'],
                    spawn_condition=o['spawn_condition'],
                    activation=('spawned at map load' if o['spawn_condition']==0 else
                                'cutscene/script activated (condition 1)' if o['spawn_condition']==1 else
                                f'story-flag condition {o["spawn_condition"]:#x}'),
                    position=[o['x'],o['y']], gfx_id=o['gfx_id'],
                    stats=dict(hp=r['hp'],STR=r['STR'],POW=r['POW'],END=r['END'],melee_taken_mult=r['melee_taken_mult'],
                               energy_taken_mult=r['energy_taken_mult'],class_exp=r['exp']),
                    ai_state=r['ai_state'], ai_states_used=summ['states'],
                    detection_radius_px=summ['aggro_radius_px'], range_check_radii_px=summ['radii_px'],
                    movement_speeds_px_per_frame=summ['speeds'],
                    attacks=summ['attacks'], special_attacks=specials,
                    energy_absorb=summ['energy_absorb'], stun_on_attack_ids=summ['stun_on_attack_ids'],
                    on_death=dict(sequence=de['script'],engine=de['fn'],effects=eff,dialogue_excerpt=texts[:6]),
                    next_phase=[dict(class_row=e['class_row'],map_index=e['map_index'],object_index=e['object_index'],how=e['op']) for e in nxt],
                    active_character_after=[CHARS[e['index']] if e['index']<6 else e['index'] for e in eff if e['op']=='set_active_character'],
                    measured=m_))
        # player character for a phase reached through a previous phase's death cutscene = last active-character switch there
        for p in phases:
            p['player_character']=None
        for p in phases:
            for nx in p['next_phase']:
                for q in phases:
                    if q['map_index']==nx['map_index'] and q['object_index']==nx['object_index']:
                        q['player_character']=(p['active_character_after'][-1] if p['active_character_after'] else 'unchanged')
        out.append(dict(key=F['key'],name=F['name'],classes=F['classes'],notes=F.get('notes',''),phases=phases))
    rules=dict(
        phase_model=__doc__.split('Phase model')[1].strip(),
        hit_reactions=('Fighter bosses (ai 21/29/80/98/129/138) use on-hit state 18: damage, then attack id 9 -> stun 128+32 f '
                       '(state 20, with knockback 16), id 15 -> stun 256+64 f (state 19), else knockback: wall behind -> state 16 '
                       'KnockbackInvulnerable(48) = unhittable until the hurt anim ends; open ground -> state 17 Knockback(32). '
                       'Cell Nonperfect/Perfect (ai 116/148) use the twin handler 113 (Knockback 24 / KnockbackInvulnerable 40). VERIFIED(disasm).'),
        invulnerability=('Bosses have no scripted invulnerable windows inside a fight; they are unhittable only during '
                         'KnockbackInvulnerable (wall case) and while not spawned/in cutscenes. VERIFIED(disasm).'),
        heals=('Android 19/20 (ai 29): any energy attack is absorbed: no damage, heal 20% of raw damage (VERIFIED in emulator). '
               'Cell drain grab heals Cell by the damage it deals (VERIFIED(disasm); heal capped at max HP).'),
        teleports='No teleport node exists in the AI node set; bosses move only via Chase/Flee/Align/BackAway nodes (and cutscene actors).',
        summons=('Cell Juniors are not summoned by the AI: they are three separate spawn defs (map 325 objects 8, 9, 10) activated '
                 'one at a time by the previous fight\'s death cutscene, each followed by an active-character switch.'),
        measurement_setup='bs_measure.py: debug Map Test Lv40 party, active Piccolo (STR 40 POW 55 END 53), boss force-spawned on map 13 (objdef 0x083ee440 patched), player stands still after placement at 30 or 100 px.',
    )
    doc=dict(source='roms/log2.gba',rules=rules,attack_catalog_ref='enemies.json -> enemy_attacks',fights=out)
    os.makedirs(OUTDIR,exist_ok=True)
    json.dump(doc,open(OUTDIR+'bosses.json','w'),indent=1)
    print('fights',len(out),'phases',sum(len(f['phases']) for f in out))
if __name__=='__main__': main()
