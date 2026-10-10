p = '/Users/prof/source/buus-fury-decomp/engine/crates/wf-core/src/menu/kit.rs'
s = open(p).read()
if 'Item(ItemCtl)' not in s:
    a = """    /// a tab header box (ctor `0x08015C62`, vt `0x080493F4`; Journal's Goals / Completed Goals)
    TabBox(TabBox),
}"""
    b = """    /// a tab header box (ctor `0x08015C62`, vt `0x080493F4`; Journal's Goals / Completed Goals)
    TabBox(TabBox),
    /// a title-menu item (`0x080060A8` with vt `0x0804D82C`, `0x0804E358`): small font, outlined glyphs
    Item(ItemCtl),
}

/// a title-menu item (START / OPTIONS, Single Player / Multiplayer): a selectable control like [`TextCtl`] drawn
/// centred in the small font with the outlined glyph routine `0x0803F70C` (outline colour 1), colour 6 selected,
/// else 2 (0x75 when disabled, `0x0801E6FC`); cursor point (x − width / 2, y − 3) (`0x08006248`)
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ItemCtl {
    pub id: i32,
    pub nav: [i32; 4],
    pub text: Vec<u16>,
    pub x: i32,
    pub y: i32,
    pub code: i32,
    pub enabled: bool,
}"""
    assert a in s
    s = s.replace(a, b, 1)
    a = """            Widget::Tabs(t) => {
                if sel != t.id {
                    return (false, None);
                }"""
    b = """            Widget::Item(t) => {
                if sel != t.id {
                    return (false, None);
                }
                if let Some(to) = nav_move(&t.nav, k.pressed) {
                    sfx.push(super::SFX_CURSOR);
                    self.sel = to;
                    self.drawn = false;
                    return (true, None);
                }
                if k.pressed & (A | START) != 0 {
                    return (false, Some((t.code, EvMode::Handler)));
                }
                (false, None)
            }
            Widget::Tabs(t) => {
                if sel != t.id {
                    return (false, None);
                }"""
    assert a in s
    s = s.replace(a, b, 1)
    a = """                Widget::Tabs(tb) if tb.id == self.sel => {
                    let mut x = tb.x0;"""
    b = """                Widget::Item(c) if c.id == self.sel => {
                    let m = t.font(MenuFont::Small).measure(&c.text);
                    return Some((c.x - (m >> 1), c.y - 3));
                }
                Widget::Tabs(tb) if tb.id == self.sel => {
                    let mut x = tb.x0;"""
    assert a in s
    s = s.replace(a, b, 1)
    a = """                Widget::Slider(sl) => sl.draw(cv),"""
    b = """                Widget::Slider(sl) => sl.draw(cv),
                Widget::Item(c) => {
                    let col = if self.sel == c.id {
                        6
                    } else if c.enabled {
                        2
                    } else {
                        0x75
                    };
                    let w = t.font(MenuFont::Small).measure(&c.text);
                    cv.ops.push(wf_view::MenuOp::TextOutline {
                        font: MenuFont::Small,
                        x: c.x - ((w + 1) >> 1),
                        y: c.y,
                        colour: col,
                        outline: 1,
                        clip: [0, 0, 240, 160],
                        text: c.text.clone(),
                    });
                }"""
    assert a in s
    s = s.replace(a, b, 1)
    open(p, 'w').write(s)
print('ok')
