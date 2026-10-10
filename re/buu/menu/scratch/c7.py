R = '/Users/prof/source/buus-fury-decomp/engine/crates/wf-core/src/'
p = R + 'menu/mod.rs'
s = open(p).read()
if 'pub overlay: Option<Box<Screen>>' not in s:
    a = """    #[serde(default)]
    pub exit_stub: Option<i32>,
}"""
    b = """    #[serde(default)]
    pub exit_stub: Option<i32>,
    /// a screen picture left on screen after the menu is gone, while the world runs below it until `World.frame`
    /// reaches `overlay_until` (New Game: the prologue script fades the title's Select Game picture out; title.rs)
    #[serde(default)]
    pub overlay: Option<Box<Screen>>,
    #[serde(default)]
    pub overlay_until: u32,
}"""
    assert a in s
    s = s.replace(a, b, 1)
    a = """        Phase::Enter { .. } | Phase::Leave { .. } => return Some(MenuView { backdrop: 0, palette: vec![(0, 0)], ..Default::default() }),
        _ => return None,
    }
    let d = w.data.menu.as_ref()?;"""
    b = """        Phase::Enter { .. } | Phase::Leave { .. } => return Some(MenuView { backdrop: 0, palette: vec![(0, 0)], ..Default::default() }),
        Phase::Idle => {
            let o = w.menu.overlay.as_deref().filter(|_| w.frame < w.menu.overlay_until)?;
            return Some(o.view(w, w.data.menu.as_ref()?));
        }
        _ => return None,
    }
    let d = w.data.menu.as_ref()?;"""
    assert a in s
    s = s.replace(a, b, 1)
    open(p, 'w').write(s)

p = R + 'session.rs'
s = open(p).read()
if 'title::boot' not in s:
    a = """        if crate::menu::active(w) {
            crate::menu::step(w);
            if !std::mem::take(&mut w.menu.resume) {"""
    b = """        if crate::menu::active(w) {
            crate::menu::step(w);
            // the title's New Game / Continue: a new field world (menu/title.rs); its load events stay for the host
            if let Some(nw) = crate::menu::title::boot(&self.data, &mut self.world) {
                self.world = nw;
                self.presenter.reset();
                return;
            }
            let w = &mut self.world;
            if !std::mem::take(&mut w.menu.resume) {"""
    assert a in s
    s = s.replace(a, b, 1)
    open(p, 'w').write(s)
print('ok')
