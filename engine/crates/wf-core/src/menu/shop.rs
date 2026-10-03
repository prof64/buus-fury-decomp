//! The shop screens: BUY (`open_shop_buy` 0xCE → mode ctor `0x080315B0`, vt `0x080499A8`, enter `0x080316D6`,
//! redraw `0x0803185C`, events `0x08031C4C`, done check `0x08031D94`, preview `0x08031644`) over the list built by
//! `shop_add_item` 0xCD (`0x08031808`: 8-byte entries {u8 category (0 supply, 1 equipment), u8 stock (0 = no
//! limit), u16 item, u32 price}, count at `0x03003174`) and SELL (`open_shop_sell(percent)` 0xCF → ctor
//! `0x08031E84`, vt `0x08049B88`, enter `0x08032070`, redraw `0x080321D0`, events `0x080325C0`, done check
//! `0x080326F4`): the list is built from what the player owns: every supply with a def price (+0x2C) and a count,
//! every equipment with a def price (+0x20) and unequipped copies; price = −price when the def price is negative,
//! else price · percent / 100 (`0x08053034`, truncating); stock = the owned count.
//!
//! Layout (both): background, panel (8, 8)-(232, 152), separators y 66 / 132 (x 16..224), x 168 (y 70..128), the
//! list scroll bar x 220 (y 27..49); 3 rows (16 px, from y 23): icon centred on x 37, name (truncated with "..")
//! at x 54, price right-aligned at x 212 ("%d Z"; buy: colour 0x75 when unaffordable, 6 selected, else 2); the
//! selected entry's description wrapped to 148 px from (16, 79), line height 14; STR / POW / END / SPD with the
//! entry equipped (preview) at x 175, the required level for equipment; "Buy x %d" / "Sell x %d" (colour 5) at
//! (28, 145) with the B prompt moved after it and "Cancel"; "%d ZENIE" right-aligned at (224, 145). Children: UP /
//! DOWN prompts at (213, 12) / (213, 49) (codes 2 / 3, auto-repeat), A (12, 134) code 0 buy / sell, B code 1 leave,
//! the blinking cursor at the selected row (point (20, row · 16 + 23)). LEFT / RIGHT (done check) change the count
//! (sound 39, or 38 when not allowed: count 1 / the money / the stock).

use super::kit::{self, Button, Container, Cursor, Widget, BTN_A, BTN_B, BTN_DOWN, BTN_UP};
use super::text::{decode_escaped, Arg, Canvas, Pen, TextData};
use super::{Action, MenuData, Page, Transition};
use crate::scene::ShopItem;
use crate::world::World;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use wf_view::{MenuFont, MenuView};

/// sound 38: not allowed
pub const SFX_NO: i32 = 38;

/// one item's menu data
#[derive(Clone, Debug, Default)]
pub struct ItemUi {
    pub name: Vec<u16>,
    pub desc: Vec<u16>,
    pub icon: String,
    /// def price (supply +0x2C, equipment +0x20): the sell price base
    pub price: i32,
    /// equipment: slot (+0x14: 0 body, 1 hands, 2 feet, 3 accessory), required level (+0x15)
    pub slot: u8,
    pub level: u8,
}

/// supplies (by supply index) and equipment (by item id) from `items/*.json` / `equipment/*.json`
#[derive(Clone, Debug, Default)]
pub struct ItemsUi {
    pub supplies: Vec<ItemUi>,
    pub equipment: Vec<ItemUi>,
    /// the bytecode of event #6 (header `0x0877CCA4`): the chapter string id a save slot shows (save.rs)
    pub chapter_code: Vec<u8>,
    /// the title menus' box remap (`ui/menu/title_shade.json`, title.rs)
    pub title_shade: Vec<u8>,
    /// the OBJ remap of party members that cannot be chosen (`ui/menu/silhouette.json`, save.rs)
    pub silhouette: Vec<u8>,
    /// the save slot corner markers' offsets (`ui/menu/slot_corner.json`)
    pub corners: Vec<[i32; 2]>,
}

fn put(v: &mut Vec<ItemUi>, i: usize, it: ItemUi) {
    if i >= v.len() {
        v.resize(i + 1, ItemUi::default());
    }
    v[i] = it;
}

impl ItemsUi {
    pub fn load(src: &dyn crate::content::ContentSource) -> ItemsUi {
        let mut out = ItemsUi::default();
        let s = |v: &Value, k: &str| decode_escaped(v[k].as_str().unwrap_or(""));
        for p in src.list() {
            let sup = p.starts_with("items/");
            let eq = p.starts_with("equipment/");
            if !(sup || eq) || !p.ends_with(".json") || p.ends_with("index.json") {
                continue;
            }
            let Some(b) = src.read(&p) else { continue };
            let Ok(v) = serde_json::from_slice::<Value>(&b) else { continue };
            let Some(i) = v["item"].as_u64() else { continue };
            let _ = &i;
            let icon = v["icon"].as_str().unwrap_or("").to_string();
            let price = v["price"].as_i64().unwrap_or(0) as i32;
            if sup {
                // supply def +4 = the short name, +8 = the description
                let name = if v["short_name"].is_string() { s(&v, "short_name") } else { s(&v, "name") };
                put(&mut out.supplies, i as usize, ItemUi { name, desc: s(&v, "description"), icon, price, slot: 0, level: 0 });
            } else {
                let raw: Vec<u8> = v["raw"].as_str().map(|h| (0..h.len() / 2).filter_map(|k| u8::from_str_radix(&h[2 * k..2 * k + 2], 16).ok()).collect()).unwrap_or_default();
                let slot = raw.get(0x14).copied().unwrap_or_else(|| match v["slot"].as_str() {
                    Some("hands") => 1,
                    Some("feet") => 2,
                    Some("accessory") => 3,
                    _ => 0,
                });
                let level = raw.get(0x15).copied().unwrap_or(v["required_level"].as_u64().unwrap_or(0) as u8);
                let price = if raw.len() >= 0x24 { i32::from_le_bytes([raw[0x20], raw[0x21], raw[0x22], raw[0x23]]) } else { price };
                put(&mut out.equipment, i as usize, ItemUi { name: s(&v, "name"), desc: s(&v, "description"), icon, price, slot, level });
            }
        }
        if let Some(b) = src.read("scripts/bytecode.json") {
            if let Ok(v) = serde_json::from_slice::<Value>(&b) {
                if let Some(h) = v["events"]["s006"]["nodes"][0]["code"]["code"].as_str() {
                    out.chapter_code = (0..h.len() / 2).filter_map(|k| u8::from_str_radix(&h[2 * k..2 * k + 2], 16).ok()).collect();
                }
            }
        }
        if let Some(b) = src.read("ui/menu/silhouette.json") {
            if let Ok(v) = serde_json::from_slice::<Value>(&b) {
                out.silhouette = v["table"].as_array().map(|a| a.iter().map(|x| x.as_u64().unwrap_or(0) as u8).collect()).unwrap_or_default();
            }
        }
        if let Some(b) = src.read("ui/menu/slot_corner.json") {
            if let Ok(v) = serde_json::from_slice::<Value>(&b) {
                out.corners = v["offsets"].as_array().map(|a| a.iter().map(|o| [o[0].as_i64().unwrap_or(0) as i32, o[1].as_i64().unwrap_or(0) as i32]).collect()).unwrap_or_default();
            }
        }
        if let Some(b) = src.read("ui/menu/title_shade.json") {
            if let Ok(v) = serde_json::from_slice::<Value>(&b) {
                out.title_shade = v["table"].as_array().map(|a| a.iter().map(|x| x.as_u64().unwrap_or(0) as u8).collect()).unwrap_or_default();
            }
        }
        out
    }
    pub fn get(&self, category: i32, item: i32) -> Option<&ItemUi> {
        let v = if category == 1 { &self.equipment } else { &self.supplies };
        v.get(item.max(0) as usize)
    }
}

// ---------------------------------------------------------------------------------------------------------------
// drawing helpers (shared routines of the game's text / primitive library)

/// `0x08005154(ctx, x0, y, x1)`: the 3-row horizontal separator
pub fn hsep(cv: &mut Canvas, x0: i32, y: i32, x1: i32) {
    let w = x1 - x0;
    cv.fill(x0 + 1, y - 1, x0 + 1 + (w - 2), y, 0x2F);
    cv.pixel(x0, y, 0x2F);
    cv.fill(x0 + 1, y, x0 + 1 + (w - 2), y + 1, 0x30);
    cv.pixel(x0 + w - 1, y, 0x31);
    cv.fill(x0 + 1, y + 1, x0 + 1 + (w - 2), y + 2, 0x31);
}

/// `0x080051B0(ctx, x, y0, y1)`: the vertical separator
pub fn vsep(cv: &mut Canvas, x: i32, y0: i32, y1: i32) {
    cv.pixel(x, y0, 0x2F);
    let mut y = y0 + 1;
    for _ in 0..(y1 - y0 - 2).max(0) {
        cv.pixel(x - 1, y, 0x2F);
        cv.pixel(x, y, 0x30);
        cv.pixel(x + 1, y, 0x31);
        y += 1;
    }
    cv.pixel(x, y, 0x31);
}

fn unit_w(t: &TextData, f: MenuFont, c: u16) -> i32 {
    t.font(f).measure_n(&[c], 1)
}

/// `0x0803DCC4(ctx, maxw, s)`: draw `s` when it fits `maxw`, else the units that fit with ".." (whose width is
/// counted first) and ".."
pub fn text_trunc(cv: &mut Canvas, t: &TextData, pen: &mut Pen, maxw: i32, s: &[u16]) {
    let fm = t.font(pen.font);
    if fm.measure(s) <= maxw {
        cv.text(t, pen, s);
        return;
    }
    let mut w = 2 * unit_w(t, pen.font, b'.' as u16);
    let mut out = Vec::new();
    for &c in s.iter().take_while(|&&c| c != 0) {
        w += unit_w(t, pen.font, c);
        if w > maxw {
            break;
        }
        out.push(c);
    }
    out.push(b'.' as u16);
    out.push(b'.' as u16);
    cv.text(t, pen, &out);
}

/// `0x0803DD66(ctx, maxw, s)`: word-wrapped lines (leading spaces skipped, a line breaks at the last space before
/// it exceeds `maxw`, or at `\n`; colour codes take no width here), each drawn aligned like `0x0803DADA`, the pen y
/// moving by the line height; stops below the clip rect
pub fn text_wrap(cv: &mut Canvas, t: &TextData, pen: &mut Pen, maxw: i32, s: &[u16]) {
    let mut i = 0usize;
    let mut top = pen.y - 12;
    let at = |k: usize| s.get(k).copied().unwrap_or(0);
    while top < pen.clip[3] && at(i) != 0 {
        if at(i) == 0x20 {
            i += 1;
            continue;
        }
        let mut e = i;
        let mut w = 0;
        let mut brk: Option<usize> = None;
        loop {
            let c = at(e);
            if c == 0 {
                break;
            }
            if c == 0x0A {
                e += 1;
                break;
            }
            if c == 8 {
                e += 2;
                continue;
            }
            if c == 0x20 {
                brk = Some(e);
            }
            w += unit_w(t, pen.font, c);
            if w > maxw {
                if let Some(b) = brk {
                    e = b;
                }
                break;
            }
            e += 1;
        }
        let n = e - i;
        if n > 0 && top > pen.clip[1] - 16 {
            let line = &s[i..e];
            let lw = t.font(pen.font).measure_n(line, n);
            let x = match pen.align & 3 {
                1 => pen.x - ((lw + 1) >> 1),
                2 => pen.x - lw,
                _ => pen.x,
            };
            let mut p2 = pen.clone();
            p2.x = x;
            p2.align = 0;
            p2.single = true;
            // drawn unit by unit: `0x0A` has no glyph and takes no room
            let save_x = p2.x;
            cv.text(t, &mut p2, line);
            let _ = save_x;
            pen.colour = p2.colour;
        }
        top += pen.line_h;
        pen.y += pen.line_h;
        i = e;
    }
}

// ---------------------------------------------------------------------------------------------------------------

/// the shop screen (buy or sell)
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Shop {
    pub c: Container,
    pub sell: bool,
    pub percent: i32,
    /// the entries (buy: the `shop_add_item` list; sell: the owned items)
    pub list: Vec<ShopItem>,
    /// +0x114 / +0x118 / +0x11C: selected entry, first shown row, count
    pub sel: i32,
    pub top: i32,
    pub count: i32,
    /// +0x120..: STR / POW / END / SPD of the current record (RecalcStats at the ctor)
    pub base: [i32; 4],
    /// +0x130..: the same with the selected equipment worn (`0x08031644`)
    pub preview: [i32; 4],
    pub up: usize,
    pub down: usize,
    pub btn_b: usize,
    /// sampled when drawn: the money, the current character's level
    pub zenie: u32,
    pub level: i32,
}

fn cur_rec(w: &World) -> crate::story::CharState {
    w.story.chars.get(w.story.current.max(0) as usize).cloned().unwrap_or_default()
}

fn eff_with(w: &World, cs: &crate::story::CharState, slot: Option<(usize, i32)>) -> [i32; 4] {
    let mut eq = cs.equip;
    if let Some((s, id)) = slot {
        if s < 4 {
            eq[s] = id;
        }
    }
    let eqs: Vec<Option<&crate::content::Equip>> = eq.iter().map(|&e| w.data.equipment.iter().find(|q| q.id == e)).collect();
    let e = crate::progression::recalc_stats(&cs.rec, &eqs, false);
    [e.str_, e.pow, e.end, e.spd]
}

/// unequipped copies of an equipment item (`0x080043EE`): entries without a wearer mark (the story's list holds
/// every copy; the marks come from the characters' slots, `equip::marks`)
pub fn owned_free(w: &World, item: i32) -> i32 {
    let m = super::equip::marks(w);
    w.story.equipment.iter().enumerate().filter(|&(k, &e)| e == item && m.get(k).copied().unwrap_or(0) == 0).count() as i32
}

impl Shop {
    /// `0x080315B0` (buy) / `0x08031E84` (sell) + the enter (`0x080316D6` / `0x08032070`)
    pub fn new(w: &World, sell: bool, percent: i32) -> Shop {
        let cs = cur_rec(w);
        let base = eff_with(w, &cs, None);
        let mut list = Vec::new();
        if sell {
            let d = w.data.menu.as_ref().map(|m| m.items.clone()).unwrap_or_default();
            let price = |p: i32| if p < 0 { -p } else { p.wrapping_mul(percent) / 100 };
            for i in 0..0x53usize {
                let p = d.supplies.get(i).map(|x| x.price).unwrap_or(0);
                let n = w.story.supplies.get(i).copied().unwrap_or(0) as i32;
                if p != 0 && n != 0 {
                    list.push(ShopItem { category: 0, item: i as i32, price: price(p), flag: n & 0xFF });
                }
            }
            for i in 0..0xA2usize {
                let p = d.equipment.get(i).map(|x| x.price).unwrap_or(0);
                if p == 0 {
                    continue;
                }
                let n = owned_free(w, i as i32);
                if n != 0 {
                    list.push(ShopItem { category: 1, item: i as i32, price: price(p), flag: n & 0xFF });
                }
            }
        } else {
            list = w.scene.shop.clone();
        }
        let mut c = Container::default();
        let up = c.add(Widget::Button(Button::new(BTN_UP, 0xD5, 0xC, 2, true, true)));
        let down = c.add(Widget::Button(Button::new(BTN_DOWN, 0xD5, 0x31, 3, true, true)));
        c.add(Widget::Button(Button::new(BTN_A, 0xC, 0x86, 0, true, false)));
        let btn_b = c.add(Widget::Button(Button::new(BTN_B, 0xC, 0x86, 1, true, false)));
        c.add(Widget::Cursor(Cursor { t: 0 }));
        let mut s = Shop { c, sell, percent, list, sel: 0, top: 0, count: 1, base, preview: base, up, down, btn_b, zenie: w.story.zenie, level: 0 };
        s.update_preview(w);
        s
    }

    /// `0x08031644`: the preview stats (the selected equipment worn), then a redraw
    fn update_preview(&mut self, w: &World) {
        self.preview = self.base;
        if let Some(e) = self.list.get(self.sel.max(0) as usize) {
            if e.category == 1 {
                let slot = w.data.menu.as_ref().and_then(|m| m.items.get(1, e.item)).map(|x| x.slot).unwrap_or(0);
                self.preview = eff_with(w, &cur_rec(w), Some((slot as usize, e.item)));
            }
        }
        self.c.drawn = false;
    }

    /// handle_event (`0x08031C4C` / `0x080325C0`)
    fn event(&mut self, w: &mut World, code: i32) -> Option<Action> {
        match code {
            0 => {
                if self.list.is_empty() {
                    return None;
                }
                let k = self.sel.max(0) as usize;
                let e = self.list[k].clone();
                if !self.sell {
                    if (w.story.zenie as i64) < e.price as i64 {
                        w.sfx(SFX_NO, 64);
                        return None;
                    }
                    w.story.zenie = (w.story.zenie as i64 - (e.price as i64 * self.count as i64)) as u32;
                    if e.category == 0 {
                        if let Some(n) = w.story.supplies.get_mut(e.item.max(0) as usize) {
                            *n = (*n as i32 + self.count).min(255) as u8;
                        }
                    } else {
                        for _ in 0..self.count {
                            if w.story.equipment.len() < crate::story::EQUIPMENT_MAX {
                                w.story.equipment.push(e.item);
                            }
                        }
                    }
                    if e.flag != 0 {
                        let left = (e.flag - self.count) & 0xFF;
                        self.list[k].flag = left;
                        if left == 0 {
                            self.remove_entry(k);
                        }
                    }
                } else {
                    w.story.zenie = (w.story.zenie as i64 + e.price as i64 * self.count as i64) as u32;
                    if e.category == 0 {
                        if let Some(n) = w.story.supplies.get_mut(e.item.max(0) as usize) {
                            *n = (*n as i32 - self.count) as u8;
                        }
                    } else {
                        for _ in 0..self.count {
                            // the first unequipped copy (take_equipment 0x08004468 skips marked entries)
                            let m = super::equip::marks(w);
                            if let Some(p) = w.story.equipment.iter().enumerate().position(|(k, &x)| x == e.item && m.get(k).copied().unwrap_or(0) == 0) {
                                w.story.equipment.remove(p);
                            }
                        }
                    }
                    let left = (e.flag - self.count) & 0xFF;
                    self.list[k].flag = left;
                    if left == 0 {
                        self.remove_entry(k);
                    }
                }
                self.count = 1;
                self.c.drawn = false;
                w.sfx(super::SFX_ACCEPT, 64);
                None
            }
            1 => Some(Action::Pop(1)),
            2 => {
                self.count = 1;
                self.sel -= 1;
                w.sfx(super::SFX_CURSOR, 64);
                self.update_preview(w);
                None
            }
            3 => {
                self.count = 1;
                self.sel += 1;
                w.sfx(super::SFX_CURSOR, 64);
                self.update_preview(w);
                None
            }
            _ => None,
        }
    }

    fn remove_entry(&mut self, k: usize) {
        self.list.remove(k);
        let n = self.list.len() as i32;
        if n <= self.sel {
            self.sel = (n - 1).max(0);
        }
    }

    /// done check (`0x08031D94` / `0x080326F4`): LEFT / RIGHT change the count
    fn done_check(&mut self, w: &mut World) {
        let p = w.menu.pressed;
        if p & crate::input::LEFT != 0 {
            if self.count > 1 {
                self.count -= 1;
                w.sfx(super::SFX_CURSOR, 64);
                self.c.drawn = false;
            } else {
                w.sfx(SFX_NO, 64);
            }
        }
        if p & crate::input::RIGHT != 0 {
            let ok = match self.list.get(self.sel.max(0) as usize) {
                Some(e) if self.sell => self.count < e.flag,
                Some(e) => {
                    (w.story.zenie as i64) >= e.price as i64 * (self.count as i64 + 1) && (e.flag == 0 || self.count < e.flag)
                }
                None => false,
            };
            if ok {
                self.count += 1;
                w.sfx(super::SFX_CURSOR, 64);
                self.c.drawn = false;
            } else {
                w.sfx(SFX_NO, 64);
            }
        }
    }

    /// draw-time side effects of the redraw: the first shown row, the arrows' state, the cursor point, the B prompt
    /// after the "Buy x %d" text
    fn prepare(&mut self, w: &World, t: &TextData) {
        let n = self.list.len() as i32;
        self.top = (self.sel - 1).min(n - 3).max(0);
        if let Some(Widget::Button(b)) = self.c.kids.get_mut(self.up) {
            b.enabled = self.sel > 0;
        }
        if let Some(Widget::Button(b)) = self.c.kids.get_mut(self.down) {
            b.enabled = n - 1 > self.sel;
        }
        self.c.point = Some(if n == 0 { None } else { Some((0x14, (self.sel - self.top) * 16 + 0x17)) });
        let s = super::text::sprintf(t.s(if self.sell { 156 } else { 155 }), &[Arg::I(self.count)]);
        let bw = t.font(MenuFont::Menu).measure(&s);
        if let Some(Widget::Button(b)) = self.c.kids.get_mut(self.btn_b) {
            b.x = bw + 0x1E;
            b.y = 0x86;
        }
        self.zenie = w.story.zenie;
        self.level = crate::progression::level_of(cur_rec(w).rec.xp, &w.data.xp_table);
    }

    fn draw(&self, d: &MenuData) -> MenuView {
        let t = &d.text;
        let mut cv = Canvas::new();
        cv.background(0, 0, 240, 160);
        cv.panel(8, 8, 232, 152);
        let mut p = Pen { line_h: 14, shadow: Some(1), colour: 2, font: MenuFont::Menu, ..Pen::default() };
        hsep(&mut cv, 0x10, 0x42, 0xE0);
        hsep(&mut cv, 0x10, 0x84, 0xE0);
        vsep(&mut cv, 0xA8, 0x46, 0x80);
        vsep(&mut cv, 0xDC, 0x1B, 0x31);
        for row in 0..3 {
            let k = self.top + row;
            if k >= self.list.len() as i32 {
                break;
            }
            let e = &self.list[k as usize];
            let it = d.items.get(e.category, e.item).cloned().unwrap_or_default();
            p.colour = if !self.sell && (self.zenie as i64) < e.price as i64 {
                0x75
            } else if self.sel == k {
                6
            } else {
                2
            };
            if !it.icon.is_empty() {
                cv.image(&it.icon, 0, 16, 16, 0x25 - 8, row * 16 + 0x17 - 8);
            }
            let y = row * 16 + 0x19;
            p.x = 0xD4;
            p.y = y;
            p.align = 2;
            cv.textf(t, &mut p, t.s(154), &[Arg::I(e.price)]);
            p.x = 0x36;
            p.y = y;
            p.align = 0;
            text_trunc(&mut cv, t, &mut p, if self.sell { 0x58 } else { 0x6E }, &it.name);
            if self.sell {
                cv.textf(t, &mut p, t.s(157), &[Arg::I(e.flag)]);
            }
            if self.sel == k {
                p.x = 0x10;
                p.y = 0x4F;
                p.colour = 2;
                text_wrap(&mut cv, t, &mut p, 0x94, &it.desc);
            }
        }
        p.shadow = Some(1);
        let stat = |i: usize| -> usize {
            let (b, v) = (self.base[i], self.preview[i]);
            if v == b {
                0
            } else if v > b {
                1
            } else {
                2
            }
        };
        for (i, (y, s0)) in [(0x4E, 55usize), (0x5A, 58), (0x66, 61), (0x72, 64)].iter().enumerate() {
            p.x = 0xAF;
            p.y = *y;
            p.align = 0;
            cv.textf(t, &mut p, t.s(s0 + stat(i)), &[Arg::I(self.preview[i])]);
        }
        if let Some(e) = self.list.get(self.sel.max(0) as usize) {
            if e.category == 1 {
                let lv = d.items.get(1, e.item).map(|x| x.level as i32).unwrap_or(0);
                p.x = 0xAF;
                p.y = 0x7E;
                cv.textf(t, &mut p, t.s(if self.level >= lv { 67 } else { 68 }), &[Arg::I(lv)]);
            }
        }
        p.colour = 5;
        let s = super::text::sprintf(t.s(if self.sell { 156 } else { 155 }), &[Arg::I(self.count)]);
        p.x = 0x1C;
        p.y = 0x91;
        p.align = 0;
        cv.text(t, &mut p, &s);
        let bw = t.font(MenuFont::Menu).measure(&s);
        p.x = bw + 0x2E;
        p.y = 0x91;
        cv.text(t, &mut p, t.s(26));
        p.colour = 2;
        p.align = 2;
        p.x = 0xE0;
        p.y = 0x91;
        cv.textf(t, &mut p, t.s(153), &[Arg::I(self.zenie as i32)]);
        self.c.draw(t, &mut cv);
        super::surface_view(cv)
    }
}

/// redraw frames of the shop (CPU time; the test takes the real ones from the capture)
pub const REDRAW_FRAMES: u8 = 4;

impl Page for Shop {
    fn container(&mut self) -> &mut Container {
        &mut self.c
    }
    fn container_ref(&self) -> &Container {
        &self.c
    }
    fn before_draw(&mut self, w: &World) {
        let d = w.data.menu.clone();
        if let Some(d) = d {
            self.prepare(w, &d.text);
        }
    }
    fn step(&mut self, w: &mut World) -> Action {
        let (consumed, act) = kit::run_step(self, w, |p| &mut p.c, |p, w, _src, code| match p.event(w, code) {
            Some(a) => (true, Some(a)),
            None => (false, None),
        });
        if let Some(a) = act {
            if a == Action::Pop(1) {
                // the shop list as the screen left it (stock taken)
                if !self.sell {
                    w.scene.shop = self.list.clone();
                }
            }
            return a;
        }
        if !consumed {
            self.done_check(w);
        }
        Action::None
    }
    fn view(&self, _w: &World, d: &MenuData) -> MenuView {
        self.draw(d)
    }
    fn redraw_cost(&self) -> u8 {
        REDRAW_FRAMES
    }
}

/// the transition of the script-opened screens (quick fade `0x0805058C` / `0x08050568`, world save / restore)
pub const SCRIPTED: Transition = Transition { fade_out: 16 | super::FADE_HOLD, enter: 3, leave: 25, fade_in: 16 };

/// the sell screen builds its list in the ctor: one more black frame
pub const SCRIPTED_SELL: Transition = Transition { fade_out: 16 | super::FADE_HOLD, enter: 4, leave: 25, fade_in: 16 };

/// native 0xCE `open_shop_buy`: the buy screen over `Scene.shop`; blocks until the player leaves (no result)
pub fn open_shop_buy(w: &mut World) {
    let s = Shop::new(w, false, 0);
    super::open(w, super::Screen::Shop(s), SCRIPTED, true);
}

/// native 0xCF `open_shop_sell(percent)`
pub fn open_shop_sell(w: &mut World, percent: i32) {
    let s = Shop::new(w, true, percent);
    super::open(w, super::Screen::Shop(s), SCRIPTED_SELL, true);
}

/// a harness opening this screen from JSON (`menu::open_from_json`): `{"kind": "shop_buy", "items": [[category,
/// item, price, stock], ...]}` / `{"kind": "shop_sell", "percent": 50}`
pub fn open_json(w: &World, v: &serde_json::Value) -> Option<(super::Screen, super::Transition, bool)> {
    match v["kind"].as_str()? {
        "shop_buy" => {
            let mut w2 = w.clone();
            w2.scene.shop = v["items"]
                .as_array()
                .map(|a| a.iter().map(|e| ShopItem { category: e[0].as_i64().unwrap_or(0) as i32, item: e[1].as_i64().unwrap_or(0) as i32, price: e[2].as_i64().unwrap_or(0) as i32, flag: e[3].as_i64().unwrap_or(0) as i32 }).collect())
                .unwrap_or_default();
            Some((super::Screen::Shop(Shop::new(&w2, false, 0)), SCRIPTED, true))
        }
        "shop_sell" => Some((super::Screen::Shop(Shop::new(w, true, v["percent"].as_i64().unwrap_or(50) as i32)), SCRIPTED_SELL, true)),
        _ => None,
    }
}
