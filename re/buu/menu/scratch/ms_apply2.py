R = '/Users/prof/source/legacy-of-goku-x/.claude/worktrees/re-dashboard/'
src = open(R + 're/buu/menu/scratch/ms_export2.rs').read()
p = R + 'engine/crates/wf-ui/src/mapscreen.rs'
s = open(p).read()
if 'pub fn frame_bg0' not in s:
    s += src
    open(p, 'w').write(s)
p = R + 'engine/crates/wf-ui/src/lib.rs'
s = open(p).read()
a = "    mapscreen::area_maps(rom, out, &pal);"
assert a in s
if 'mapscreen::frame_variants' not in s:
    s = s.replace(a, a + "\n    mapscreen::frame_variants(rom, out, &pal);", 1)
    open(p, 'w').write(s)
print('ok')
