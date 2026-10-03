import json,re,collections,os,glob
R=str(__import__('pathlib').Path(__file__).resolve().parents[2]) + '/'
c=json.load(open(R+'engine/annotations/script_catalog.json'))
D=R+'work/buu/script/rhai/scripts/'
go=collections.defaultdict(set); par=collections.defaultdict(set)
for s in c['scripts']:
    sid=s['id']
    for n in s.get('nodes',[]):
        if n.get('goto_event') is not None:
            t=f"s{n['goto_event']:03d}"; go[sid].add(t); par[t].add(sid)
        for t in n.get('runs_event',[]):
            t=f"s{t:03d}" if isinstance(t,int) else t; go[sid].add(t); par[t].add(sid)
def info(sid):
    p=D+('events/' if sid.startswith('s') else 'inline/')+sid+'.rhai'
    t=open(p).read() if os.path.exists(p) else ''
    sets=sorted(set(map(int,re.findall(r'set_flag\((\d+)\)',t))))
    tst=sorted(set(map(int,re.findall(r'(?:get_flag|not_flag)\((\d+)\)',t))))
    ops=re.findall(r'((?:level_up_to|join_party|leave_party|set_character|fuse_characters|set_char_form|learn_skill|forget_skill|replace_skill|set_char_flag2|clear_char_flag2|give_item|give_equipment|add_zenie|give_zenie|give_xp|set_skill_level|add_stat_points)\([^;]*)\)',t)
    return sets,tst,ops
if __name__=='__main__':
    import sys
    for sid in sys.argv[1:]:
        s,t,o=info(sid)
        print(sid,'set',s,'tst',t,'par',sorted(par[sid]),'go',sorted(go[sid]))
        print('   ',o)
