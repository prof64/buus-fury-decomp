D = '/Users/prof/source/buus-fury-decomp/engine/crates/wf-core/src/menu/'
p = D + 'credits.rs'
s = open(p).read()
def rep(a, b):
    global s
    assert a in s, a
    s = s.replace(a, b, 1)
rep('''    /// EVA written by this / the previous iteration (the screen shows the previous)
    pub eva: i32,
    pub eva_shown: i32,
}''', '''    /// EVA (BLDALPHA, written directly: on screen at once)
    pub eva: i32,
    /// iterations still to skip: the mode's enter (VRAM / palette clears, the first pair) takes 2 frames
    pub skip: u8,
}''')
rep('''        Credits { c: Container::default(), t: 0x90, quit: false, unskippable, idx: FIRST, page: FIRST, eva: 0, eva_shown: 0 }''',
    '''        Credits { c: Container::default(), t: 0x90, quit: false, unskippable, idx: FIRST, page: FIRST, eva: 0, skip: 1 }''')
rep('''    fn before_draw(&mut self, _w: &World) {
        self.eva_shown = self.eva;
    }
''', '')
rep('''        self.c.drawn = false;
        let (pressed, held)''', '''        self.c.drawn = false;
        if self.skip > 0 {
            // the enter's second frame: no input read, no step in the game
            self.skip -= 1;
            return Action::None;
        }
        let (pressed, held)''')
rep('''                if self.quit {
                    // the mode ends now: the screen keeps this iteration's registers
                    self.eva_shown = self.eva;
                    w.menu.shown = Some(Box::new(Screen::Credits(self.clone())));
                    return Action::Pop(1);
                }''', '''                if self.quit {
                    // the mode ends now: the screen keeps this iteration's registers
                    w.menu.shown = Some(Box::new(Screen::Credits(self.clone())));
                    return Action::Pop(1);
                }''')
rep('''            blend: Some(MenuBlend { first: 0b11, second: 1 << 30, eva: self.eva_shown.clamp(0, 16) as u8, evb: 0 }),''',
    '''            blend: Some(MenuBlend { first: 0b11, second: 1 << 30, eva: self.eva.clamp(0, 16) as u8, evb: 0 }),''')
rep('''    fn view(&self, _w: &World, d: &MenuData) -> MenuView {''', '''    /// a new pair drawn (two surfaces + 2 x 15 KB uploads) makes the iteration take 2 frames
    fn redraw_cost(&self) -> u8 {
        if self.t == 0x90 {
            2
        } else {
            1
        }
    }
    fn view(&self, _w: &World, d: &MenuData) -> MenuView {''')
rep('''        m.before_draw(w);
        m.c.drawn = true;
        w.menu.shown = Some(Box::new(Screen::Credits(m.clone())));''', '''        m.c.drawn = true;
        w.menu.shown = Some(Box::new(Screen::Credits(m.clone())));''')
s = s.replace('''t == 160 -> 0. The IO registers are shadowed (copied at VBlank): the screen shows the previous iteration's EVA;
//! the text tiles are uploaded at once.''', '''t == 160 -> 0. EVA is written straight to the register. The enter takes 2 frames (the first step in the 2nd
//! frame after the call), an iteration drawing a new pair 2 frames.''')
open(p, 'w').write(s)

p = D + 'minigame.rs'
s = open(p).read()
rep('''    pub watch: Option<Watch>,
}''', '''    pub watch: Option<Watch>,
    /// iterations still to skip (Watch This!: the construction's VRAM eviction + box decode take 2 frames)
    #[serde(default)]
    pub skip: u8,
}''')
rep('''counter: w.story.play_frames as u32, watch: None }''', '''counter: w.story.play_frames as u32, watch: None, skip: 0 }''')
rep('''        self.c.drawn = false;
        self.counter = self.counter.wrapping_add(1);''', '''        self.c.drawn = false;
        if self.skip > 0 {
            // no iteration in this frame: the screen stays (nothing redrawn)
            self.skip -= 1;
            self.c.drawn = true;
            return Action::None;
        }
        self.counter = self.counter.wrapping_add(1);''')
rep('''    s.watch = Some(Watch::new(level, seq));
    open_overlay(w, s, held);''', '''    s.watch = Some(Watch::new(level, seq));
    s.skip = 1;
    open_overlay(w, s, held);''')
rep('''            let mut m = Minigame::base(Kind::Watch, Gauge::default(), w);
            m.watch = Some(Watch::new(v["level"].as_i64().unwrap_or(0) as i32, seq));
            m''', '''            let mut m = Minigame::base(Kind::Watch, Gauge::default(), w);
            m.watch = Some(Watch::new(v["level"].as_i64().unwrap_or(0) as i32, seq));
            m.skip = 1;
            m''')
open(p, 'w').write(s)
print('ok')
