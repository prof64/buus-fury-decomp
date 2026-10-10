p = '/Users/prof/source/buus-fury-decomp/engine/crates/wf-core/src/menu/kit.rs'
s = open(p).read()
a = """    Cursor(Cursor),
    /// a removed child slot (keeps indices stable for pages that refer to their children by index)
    Gone,
}"""
b = """    Cursor(Cursor),
    /// a removed child slot (keeps indices stable for pages that refer to their children by index)
    Gone,
    /// an option slider (ctor `0x08006880`, vt `0x0804BF3C`)
    Slider(Slider),
    /// a tab header box (ctor `0x08015C62`, vt `0x080493F4`; Journal's Goals / Completed Goals)
    TabBox(TabBox),
}

/// an option slider (`0x08006880`): id (selected = the page's selected control id), the bar at (x0, y), width
/// `w` (x1 - x0 - 2), value 0..=max; LEFT / RIGHT while selected change it by one (sound 39, redraw) and send
/// `code` (the page stores the value); draw `0x080068B8`
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Slider {
    pub id: i32,
    pub x0: i32,
    pub y: i32,
    pub w: i32,
    pub value: i32,
    pub max: i32,
    pub code: i32,
}

/// a tab header box (`0x08015C62`): box (x, y)-(x + width(small font) + 6, y + 10), text at (x + 3, y + 8) in the
/// small font, colour 2, drawn with the tab colours (`0x08015BB8`), `active` = the marked colours
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TabBox {
    pub rect: [i32; 4],
    pub text: Vec<u16>,
    pub active: bool,
}

impl TabBox {
    pub fn new(t: &TextData, x: i32, y: i32, text: &[u16], active: bool) -> TabBox {
        let w = t.font(MenuFont::Small).measure(text);
        TabBox { rect: [x, y, x + w + 6, y + 10], text: text.to_vec(), active }
    }
}

impl Slider {
    /// `0x080068B8`: fill = value * w / max; row 0 and 4 the top / bottom edges, rows 1..3 the bar
    pub fn draw(&self, cv: &mut Canvas) {
        let (x0, y, w) = (self.x0, self.y, self.w);
        let f = if self.max != 0 { self.value * w / self.max } else { 0 };
        let hl = |cv: &mut Canvas, x: i32, y: i32, n: i32, c: u8| {
            if n > 0 {
                cv.fill(x, y, x + n, y + 1, c);
            }
        };
        if f == 0 {
            hl(cv, x0 + 1, y, w, 1);
            for r in 1..4 {
                cv.pixel(x0, y + r, 0x73);
                hl(cv, x0 + 1, y + r, w, 0x70);
                cv.pixel(x0 + w + 1, y + r, 1);
            }
            hl(cv, x0 + 1, y + 4, w, 0x73);
        } else if f == w {
            hl(cv, x0 + 1, y, w, 0x0A);
            for r in 1..4 {
                cv.pixel(x0, y + r, 0x0A);
                hl(cv, x0 + 1, y + r, w, 3);
                cv.pixel(x0 + w + 1, y + r, 0x68);
            }
            hl(cv, x0 + 1, y + 4, w, 0x68);
        } else {
            hl(cv, x0 + 1, y, f, 0x0A);
            hl(cv, x0 + 1 + f, y, w - f, 1);
            for r in 1..4 {
                cv.pixel(x0, y + r, 0x0A);
                hl(cv, x0 + 1, y + r, f, 3);
                cv.pixel(x0 + f + 1, y + r, 0x68);
                hl(cv, x0 + f + 2, y + r, w - f - 1, 0x70);
                cv.pixel(x0 + w + 1, y + r, 1);
            }
            hl(cv, x0 + 1, y + 4, f, 0x68);
            hl(cv, x0 + 1 + f, y + 4, w - f, 0x73);
        }
    }
}"""
assert a in s
s = s.replace(a, b, 1)
a = """                (false, None)
            }
            _ => (false, None),
        }
    }"""
b = """                (false, None)
            }
            Widget::Slider(sl) => {
                // 0x080069E6: LEFT (if above 0) else RIGHT (if below max)
                if sel != sl.id {
                    return (false, None);
                }
                if k.pressed & LEFT != 0 && sl.value > 0 {
                    sfx.push(super::SFX_CURSOR);
                    sl.value -= 1;
                    self.drawn = false;
                    return (true, Some((sl.code, EvMode::Consume)));
                }
                if k.pressed & RIGHT != 0 && sl.value < sl.max {
                    sfx.push(super::SFX_CURSOR);
                    sl.value += 1;
                    self.drawn = false;
                    return (true, Some((sl.code, EvMode::Consume)));
                }
                (false, None)
            }
            _ => (false, None),
        }
    }"""
assert s.count(a) == 1, s.count(a)
s = s.replace(a, b, 1)
a = """                Widget::Gone => {}
            }
        }
    }
}"""
b = """                Widget::Gone => {}
                Widget::Slider(sl) => sl.draw(cv),
                Widget::TabBox(tb) => {
                    let c = TAB_COLOURS[tb.active as usize];
                    tab_box(cv, tb.rect[0], tb.rect[1], tb.rect[2], tb.rect[3], c);
                    let mut p = Pen::new(MenuFont::Small, tb.rect[0] + 3, tb.rect[1] + 8, 0, 2, None);
                    cv.text(t, &mut p, &tb.text);
                }
            }
        }
    }
}"""
assert s.count(a) == 1
s = s.replace(a, b, 1)
open(p, 'w').write(s)
print('ok')
