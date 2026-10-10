import os, re
D = '/Users/prof/source/buus-fury-decomp/engine/crates/wf-core/src/menu/'
p = D + 'mod.rs'
s = open(p).read()
if 'pub fn open_from_json' not in s:
    s = s.replace('''/// the pause page L (`right` = false) / R of page `from`''', '''/// Open a screen described by JSON (test harnesses: `menu_parity.rs` opens script-started screens the frame the
/// emulator's script did): `{"kind": "...", ...}`, handled by the screen modules' `open_json`.
pub fn open_from_json(w: &mut World, v: &serde_json::Value) -> bool {
    let tries: [fn(&World, &serde_json::Value) -> Option<(Screen, Transition, bool)>; 13] = [
        inventory::open_json,
        equip::open_json,
        options::open_json,
        journal::open_json,
        skills::open_json,
        shop::open_json,
        save::open_json,
        title::open_json,
        charsel::open_json,
        gameover::open_json,
        credits::open_json,
        minigame::open_json,
        mapscreen::open_json,
    ];
    for f in tries {
        if let Some((sc, tr, scripted)) = f(w, v) {
            open(w, sc, tr, scripted);
            return true;
        }
    }
    false
}

/// the pause page L (`right` = false) / R of page `from`''')
    open(p, 'w').write(s)

for f in ['inventory', 'equip', 'options', 'journal', 'skills', 'shop', 'save', 'title', 'charsel', 'gameover', 'credits', 'minigame', 'mapscreen']:
    path = D + f + '.rs'
    s = open(path).read()
    if 'pub fn open_json' in s:
        continue
    s += '''
/// a harness opening this screen from JSON (`menu::open_from_json`); None = not this screen's kind
pub fn open_json(_w: &World, _v: &serde_json::Value) -> Option<(super::Screen, super::Transition, bool)> {
    None
}
'''
    open(path, 'w').write(s)
print('ok')
