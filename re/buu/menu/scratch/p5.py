p = '/Users/prof/source/buus-fury-decomp/engine/crates/wf-core/src/menu/mod.rs'
s = open(p).read()
s = s.replace('''    /// opened by a script (the script waits for `result`) rather than by START
    pub scripted: bool,
}''', '''    /// opened by a script (the script waits for `result`) rather than by START
    pub scripted: bool,
    /// the screen change around this menu (fade steps, black frames)
    pub tr: Transition,
}

/// How a menu screen comes and goes (all measured per screen kind): the field fades out by `fade_out` per frame
/// (palette fade level 0 -> 256), `enter` black frames (world save, mode enter, first draw: the page shows on the
/// last), the menu runs, `leave` frames after the closing iteration (the first still shows the page, then black:
/// the world restore), the field fades in by `fade_in` per frame. A step of 0 = no fade (the screen cuts).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Transition {
    pub fade_out: u16,
    pub enter: u16,
    pub leave: u16,
    pub fade_in: u16,
}

impl Default for Transition {
    fn default() -> Self {
        PAUSE
    }
}

/// the pause menu (START): see [`step`]
pub const PAUSE: Transition = Transition { fade_out: 16, enter: 5, leave: 25, fade_in: 16 };

/// Open a menu screen with a transition (`scripted`: a script waits for `World.menu.result`). The field fades out
/// from the next frame.
pub fn open(w: &mut World, screen: Screen, tr: Transition, scripted: bool) {
    w.menu = Menu { phase: Phase::FadeOut { level: 0 }, stack: vec![screen], scripted, tr, ..Menu::default() };
    if tr.fade_out == 0 {
        w.menu.phase = Phase::Enter { t: 0 };
    }
}''')
s = s.replace('''    w.sfx(SFX_MENU, 64);
    let page = status::Status::new(w);
    w.menu = Menu { phase: Phase::FadeOut { level: 0 }, stack: vec![Screen::Status(page)], ..Menu::default() };
    true''', '''    w.sfx(SFX_MENU, 64);
    let page = status::Status::new(w);
    open(w, Screen::Status(page), PAUSE, false);
    true''')
s = s.replace('''pub const FADE_STEP: u16 = 16;
pub const ENTER_FRAMES: u16 = 5;
pub const LEAVE_FRAMES: u16 = 25;
''', '')
s = s.replace('''            let l = level + FADE_STEP;
            w.menu.phase = if l >= 256 { Phase::Enter { t: 0 } } else { Phase::FadeOut { level: l } };''', '''            let l = level + w.menu.tr.fade_out.max(1);
            w.menu.phase = if l >= 256 { Phase::Enter { t: 0 } } else { Phase::FadeOut { level: l } };''')
s = s.replace('''            if t + 1 >= ENTER_FRAMES {''', '''            if t + 1 >= w.menu.tr.enter.max(1) {''')
s = s.replace('''            if t + 1 >= LEAVE_FRAMES {
                w.menu.phase = Phase::FadeIn { level: 256 - FADE_STEP };
                w.story.play_frames += 1;''', '''            if t + 1 >= w.menu.tr.leave.max(1) {
                let st = w.menu.tr.fade_in;
                if st == 0 {
                    w.menu.phase = Phase::Idle;
                    w.menu.shown = None;
                } else {
                    w.menu.phase = Phase::FadeIn { level: 256u16.saturating_sub(st) };
                }
                w.story.play_frames += 1;''')
s = s.replace('''            let l = level.saturating_sub(FADE_STEP);''', '''            let l = level.saturating_sub(w.menu.tr.fade_in.max(1));''')
open(p, 'w').write(s)
print('ok')
