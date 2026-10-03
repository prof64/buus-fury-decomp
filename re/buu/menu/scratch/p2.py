p = '/Users/prof/source/legacy-of-goku-x/.claude/worktrees/re-dashboard/engine/crates/wf-core/src/menu/kit.rs'
s = open(p).read()
a = s[s.index("/// an event a child sends to its page: (child index, code)"):s.index("    /// container vt +0x1C: the selected control's cursor point (first child answering)")]
b = '''/// how an event's sender treats the page's answer
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EvMode {
    /// the sender consumed the input whatever the page answers (a button released / repeating)
    Consume,
    /// the sender returns the page's answer (a control's A / START activation, `0x08006182`)
    Handler,
    /// the sender lets the input go on to the next children (the tab bar, `0x08016188`)
    Continue,
}

/// one step of the input dispatch
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Input {
    /// done: whether a child consumed the input
    Done(bool),
    /// child `src` sent `code`; the dispatch resumes at `next` unless it is consumed
    Event { src: usize, code: i32, mode: EvMode, next: usize },
}

impl Container {
    pub fn add(&mut self, w: Widget) -> usize {
        self.kids.push(w);
        self.kids.len() - 1
    }
    /// `0x0800666C`: remove a child (the slot stays as `Gone`: the order of the others is kept, like the game's
    /// compaction; new children go after them)
    pub fn remove(&mut self, i: usize) {
        if let Some(k) = self.kids.get_mut(i) {
            *k = Widget::Gone;
        }
        if self.focus == Some(i) {
            self.focus = None;
        }
    }
    pub fn dirty(&mut self) {
        self.drawn = false;
    }

    /// the children's step (vt +0x14): the cursor's blink counter (a redraw every 8)
    pub fn step_children(&mut self) {
        let mut redraw = false;
        for w in self.kids.iter_mut() {
            if let Widget::Cursor(c) = w {
                c.t = c.t.wrapping_add(1);
                if c.t & 7 == 0 {
                    redraw = true;
                }
            }
        }
        if redraw {
            self.drawn = false;
        }
    }

    /// the input part of the container step from child `start`: the pressed button alone if there is one, else
    /// the children in order until one consumes the input or sends an event; sounds go to `sfx`
    pub fn input_from(&mut self, k: Keys, start: usize, sfx: &mut Vec<i32>) -> Input {
        if let Some(f) = self.focus {
            if start > 0 {
                return Input::Done(true);
            }
            return match self.input(f, k, sfx) {
                (_, Some((code, mode))) => Input::Event { src: f, code, mode, next: usize::MAX },
                (c, None) => Input::Done(c),
            };
        }
        for i in start..self.kids.len() {
            match self.input(i, k, sfx) {
                (_, Some((code, mode))) => return Input::Event { src: i, code, mode, next: i + 1 },
                (true, None) => return Input::Done(true),
                _ => {}
            }
        }
        Input::Done(false)
    }

    /// one child's input (vt +0x10): (consumed, event)
    fn input(&mut self, i: usize, k: Keys, sfx: &mut Vec<i32>) -> (bool, Option<(i32, EvMode)>) {
        let sel = self.sel;
        let focus = self.focus;
        match &mut self.kids[i] {
            Widget::Button(b) => {
                let mask = BUTTONS[b.btn as usize].0;
                if focus == Some(i) {
                    // 0x08013F7C focused: release -> event; held with repeat -> events; always consumed
                    if k.released & mask != 0 {
                        self.focus = None;
                        if b.enabled {
                            let code = b.code;
                            self.drawn = false;
                            return (true, Some((code, EvMode::Consume)));
                        }
                    } else if b.repeat {
                        b.countdown -= 1;
                        if b.countdown == 0 {
                            if b.interval > 4 {
                                b.interval /= 2;
                            }
                            b.countdown = b.interval;
                            if b.enabled {
                                let code = b.code;
                                self.drawn = false;
                                return (true, Some((code, EvMode::Consume)));
                            }
                        }
                    }
                    (true, None)
                } else {
                    if k.pressed & mask != 0 && b.enabled {
                        b.countdown = 16;
                        b.interval = 16;
                        self.focus = Some(i);
                        self.drawn = false;
                    }
                    (false, None)
                }
            }
            Widget::Text(t) => {
                // 0x08005FD4: the selected control moves the selection; else its activate check (vt +0x18)
                if sel != t.id {
                    return (false, None);
                }
                if let Some(to) = nav_move(&t.nav, k.pressed) {
                    sfx.push(super::SFX_CURSOR);
                    self.sel = to;
                    self.drawn = false;
                    return (true, None);
                }
                // 0x08006182: A / START sends the code; consumed = the page's answer
                if k.pressed & (A | START) != 0 {
                    return (false, Some((t.code, EvMode::Handler)));
                }
                (false, None)
            }
            Widget::Tabs(t) => {
                if sel != t.id {
                    return (false, None);
                }
                if let Some(to) = nav_move(&t.nav, k.pressed) {
                    sfx.push(super::SFX_CURSOR);
                    self.sel = to;
                    self.drawn = false;
                    return (true, None);
                }
                let left = t.left;
                // 0x08016188: only when the arrow buttons are hidden (all tabs fit); never consumes
                let arrows = matches!(self.kids.get(left), Some(Widget::Button(b)) if b.visible);
                let Widget::Tabs(t) = &mut self.kids[i] else { unreachable!() };
                if arrows {
                    return (false, None);
                }
                if k.pressed & LEFT != 0 {
                    t.step_left();
                    let code = t.code;
                    self.drawn = false;
                    return (false, Some((code, EvMode::Continue)));
                } else if k.pressed & RIGHT != 0 {
                    t.step_right();
                    let code = t.code;
                    self.drawn = false;
                    return (false, Some((code, EvMode::Continue)));
                }
                (false, None)
            }
            _ => (false, None),
        }
    }

'''
s = s.replace(a, b)
# remove the old Dispatch struct
a2 = ""
s = s.replace(a2, "")
# generic runner + stub page step
s = s.replace('''/// `0x08005FD4`: UP / DOWN / LEFT / RIGHT (in that priority) to a neighbour id >= 0''', '''/// The container step with a page's event handler (`handle_event`, vt +0x20): children's steps, then the input
/// dispatch; each event goes to `h(page, world, src, code) -> (answer, action)`; an action ends the step.
/// Returns (consumed, action): when nothing consumed the input the page runs its own check (vt +0x28).
pub fn run_step<P>(
    p: &mut P,
    w: &mut crate::world::World,
    c: fn(&mut P) -> &mut Container,
    mut h: impl FnMut(&mut P, &mut crate::world::World, usize, i32) -> (bool, Option<super::Action>),
) -> (bool, Option<super::Action>) {
    let k = Keys { held: w.menu.held, pressed: w.menu.pressed, released: w.menu.released };
    c(p).step_children();
    let mut start = 0;
    loop {
        let mut sfx = Vec::new();
        let r = c(p).input_from(k, start, &mut sfx);
        for s in sfx {
            w.sfx(s, 64);
        }
        match r {
            Input::Done(consumed) => return (consumed, None),
            Input::Event { src, code, mode, next } => {
                let (answer, act) = h(p, w, src, code);
                if act.is_some() {
                    return (true, act);
                }
                match mode {
                    EvMode::Consume => return (true, None),
                    EvMode::Handler if answer => return (true, None),
                    _ => {}
                }
                if next == usize::MAX {
                    return (true, None);
                }
                start = next;
            }
        }
    }
}

/// `0x08005FD4`: UP / DOWN / LEFT / RIGHT (in that priority) to a neighbour id >= 0''')
s = s.replace('''    pub fn step(&mut self, w: &mut crate::world::World) -> super::Action {
        let k = Keys { held: w.menu.held, pressed: w.menu.pressed, released: w.menu.released };
        let d = self.c.dispatch(k);
        for s in &d.sfx {
            w.sfx(*s, 64);
        }
        for (_, code) in d.events {
            match code {
                1 => {
                    w.sfx(super::SFX_MENU, 64);
                    return super::Action::Close;
                }
                6 | 7 => {
                    w.sfx(super::SFX_MENU, 64);
                    return super::Action::Replace(super::page_screen(w, self.page, code == 7));
                }
                _ => {}
            }
        }
        if !d.consumed && w.menu.pressed & START != 0 {
            w.sfx(super::SFX_MENU, 64);
            return super::Action::Close;
        }
        super::Action::None
    }''', '''    pub fn step(&mut self, w: &mut crate::world::World) -> super::Action {
        let (consumed, act) = run_step(self, w, |p| &mut p.c, |p, w, _src, code| match code {
            1 => {
                w.sfx(super::SFX_MENU, 64);
                (true, Some(super::Action::Close))
            }
            6 | 7 => {
                w.sfx(super::SFX_MENU, 64);
                (true, Some(super::Action::Replace(super::page_screen(w, p.page, code == 7))))
            }
            _ => (false, None),
        });
        if let Some(a) = act {
            return a;
        }
        if !consumed && w.menu.pressed & START != 0 {
            w.sfx(super::SFX_MENU, 64);
            return super::Action::Close;
        }
        super::Action::None
    }''')
open(p, 'w').write(s)

p = '/Users/prof/source/legacy-of-goku-x/.claude/worktrees/re-dashboard/engine/crates/wf-core/src/menu/status.rs'
s = open(p).read()
a = s[s.index("    pub fn step(&mut self, w: &mut World) -> Action {"):s.index("        // done check 0x080157A4")]
b = '''    pub fn step(&mut self, w: &mut World) -> Action {
        let (consumed, act) = kit::run_step(self, w, |p| &mut p.c, |p, w, src, code| match p.event(w, src, code) {
            Some(a) => (true, Some(a)),
            None => (false, None),
        });
        if let Some(a) = act {
            return a;
        }
        if consumed {
            return Action::None;
        }
'''
s = s.replace(a, b)
s = s.replace("use super::kit::{self, Button, Container, Cursor, Font, Keys, Label, Tabs, TextCtl, Widget, BTN_DOWN, BTN_L, BTN_R};",
              "use super::kit::{self, Button, Container, Cursor, Font, Label, Tabs, TextCtl, Widget, BTN_DOWN, BTN_L, BTN_R};")
open(p, 'w').write(s)
print('ok')
