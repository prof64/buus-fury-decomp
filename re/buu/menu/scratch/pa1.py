D = '/Users/prof/source/legacy-of-goku-x/.claude/worktrees/re-dashboard/engine/crates/wf-core/src/'
p = D + 'menu/mod.rs'
s = open(p).read()

def rep(a, b, cnt=1):
    global s
    assert a in s, a[:80]
    s = s.replace(a, b, cnt)

rep('''    /// the skill table's and the quest table's menu data (`ui/menu/opts.json`): options.rs
    pub opts: options::OptsData,
}''', '''    /// the skill table's and the quest table's menu data (`ui/menu/opts.json`): options.rs
    pub opts: options::OptsData,
    /// supplies' inventory data (def +4 name, +6 plural, +8 description, icon, story flag, use script): inventory.rs
    pub inv: inventory::InvData,
}''')
rep('''        Some(Arc::new(MenuData { text, portraits, forms, items, opts }))''', '''        let inv = inventory::InvData::load(src);
        Some(Arc::new(MenuData { text, portraits, forms, items, opts, inv }))''')
rep('''    /// a redraw in progress (on screen when its lag frames end)
    pub pending: Option<Box<Screen>>,''', '''    /// a redraw in progress (on screen when its lag frames end)
    pub pending: Option<Box<Screen>>,
    /// a picture copied before `pending` within the same iteration (a page redrawn by an event handler before a
    /// nested mode draws: the Inventory's Use dialog); shown first, dropped when `pending` shows
    #[serde(default)]
    pub pre: Option<Box<Screen>>,
    /// the top screen ran scripts through the host this frame (item use): `post` refreshes its redraw after them
    #[serde(default)]
    pub after_scripts: bool,''')
rep('''                    if let Some(p) = w.menu.pending.take() {
                        w.menu.shown = Some(p);
                    }
                }
                return;''', '''                    if let Some(p) = w.menu.pending.take() {
                        w.menu.shown = Some(p);
                    }
                    w.menu.pre = None;
                }
                return;''')
open(p, 'w').write(s)

# trait method + Screen helper + post()
s = open(p).read()
rep('''    /// a nested mode this one ran returned `r`
    fn child_result(&mut self, _w: &mut World, _r: i32) -> Action {
        Action::None
    }''', '''    /// a nested mode this one ran returned `r`
    fn child_result(&mut self, _w: &mut World, _r: i32) -> Action {
        Action::None
    }
    /// the scripts this screen started in its step (`FieldEvent::Run`, run by the host after the step: item use) are
    /// done: apply what the game does after them in the same iteration (see [`post`])
    fn after_scripts(&mut self, _w: &World) {}''')
s += '''
/// The rest of an active menu's frame (`Session::step_post`): when the top screen ran scripts through the host this
/// frame (they run after `step`, the game runs them inside its event handler), let it apply their results and redo
/// the iteration's redraw snapshot.
pub fn post(w: &mut World) {
    if !w.menu.after_scripts {
        return;
    }
    w.menu.after_scripts = false;
    let Some(mut top) = w.menu.stack.pop() else { return };
    top.page_mut().after_scripts(w);
    if top.container_ref().drawn {
        top.before_draw(w);
        let snap = Some(Box::new(top.clone()));
        if w.menu.pending.is_some() {
            w.menu.pending = snap;
        } else if w.menu.shown.is_some() {
            w.menu.shown = snap;
        }
    }
    w.menu.stack.push(top);
}
'''
open(p, 'w').write(s)

p = D + 'session.rs'
s = open(p).read()
rep('''        if crate::menu::active(w) {
            crate::menu::step(w);
            return;
        }''', '''        if crate::menu::active(w) {
            crate::menu::step(w);
            // scripts a menu starts (item use) run where the game runs them: right after its step
            Self::run_hook(&mut self.hook, &mut self.world, 0);
            return;
        }''')
rep('''        if crate::menu::active(w) && !crate::menu::opened_this_frame(w) {
            return &self.world.events;
        }''', '''        if crate::menu::active(w) && !crate::menu::opened_this_frame(w) {
            crate::menu::post(w);
            return &self.world.events;
        }''')
open(p, 'w').write(s)

p = D + 'field.rs'
s = open(p).read()
rep('''    LevelUp { entity: EntityId },
}''', '''    LevelUp { entity: EntityId },
    /// a supply used in the Inventory (its use script, supply def +0x30; `menu/inventory.rs`)
    ItemUse { item: i32 },
}''')
open(p, 'w').write(s)
print('ok')
