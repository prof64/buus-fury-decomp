R = '/Users/prof/source/legacy-of-goku-x/.claude/worktrees/re-dashboard/'
src = open(R + 're/buu/menu/scratch/ms_export.rs').read()
p = R + 'engine/crates/wf-ui/src/mapscreen.rs'
s = open(p).read()
if 'pub fn area_maps' not in s:
    s += src
    open(p, 'w').write(s)
p = R + 'engine/crates/wf-ui/src/lib.rs'
s = open(p).read()
a = '    out.put_json("ui/mapscreen.json", &mapscreen::json());'
assert a in s
if 'mapscreen::area_maps' not in s:
    s = s.replace(a, a + "\n    mapscreen::area_maps(rom, out, &pal);", 1)
    open(p, 'w').write(s)
print('ok')
