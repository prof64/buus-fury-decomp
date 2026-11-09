//! Coverage recorder (diagnostics; off unless `WF_COVERAGE=<dir>` is set - then it only counts, as the RNG trace): what
//! a harness shows in a picture it compares with the game. The world's step notes what ran (`note`: script natives, AI
//! states, skills), `render_view` leaves what the world holds (`sample`), and a picture being rendered (`commit`, from
//! wf-render) writes both plus the view's own sheets / screen parts to `<dir>/<level>-<pid>.jsonl`, one line per new
//! (category, key, context): `WF_COVERAGE_LEVEL` names the kind of comparison (tools/coverage/run.sh), the context
//! is the recording's name (`context`). Report: tools/coverage/report.py, engine/docs/coverage.md.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::io::Write;
use std::sync::{Mutex, OnceLock};

struct Out {
    file: Mutex<(std::fs::File, HashSet<(String, String, String)>, BTreeMap<(String, String), u64>)>,
    level: String,
}

static OUT: OnceLock<Option<Out>> = OnceLock::new();

fn out() -> Option<&'static Out> {
    OUT.get_or_init(|| {
        let dir = std::env::var_os("WF_COVERAGE")?;
        let level = std::env::var("WF_COVERAGE_LEVEL").unwrap_or_else(|_| "gate".into());
        std::fs::create_dir_all(&dir).ok()?;
        let p = std::path::Path::new(&dir).join(format!("{level}-{}.jsonl", std::process::id()));
        let file = std::fs::OpenOptions::new().create(true).append(true).open(p).ok()?;
        Some(Out { file: Mutex::new((file, HashSet::new(), BTreeMap::new())), level })
    })
    .as_ref()
}

/// recording? (the callers build their keys only then)
pub fn on() -> bool {
    out().is_some()
}

/// the comparison's kind (`WF_COVERAGE_LEVEL`)
pub fn level() -> Option<&'static str> {
    out().map(|o| o.level.as_str())
}

#[derive(Default)]
struct Local {
    context: String,
    /// what ran since the last compared picture
    pending: BTreeSet<(&'static str, String)>,
    /// what the world held at the last `render_view` (its frame)
    sample: (u64, Vec<(&'static str, String)>),
    /// the harness's own position (a whole run's row), written instead of the world's frame
    at: Option<u64>,
}

thread_local! {
    static LOCAL: RefCell<Local> = RefCell::new(Local::default());
}

/// the recording being replayed on this thread (a new one drops what the last left uncompared)
pub fn context(name: &str) {
    if !on() {
        return;
    }
    LOCAL.with(|l| {
        let mut l = l.borrow_mut();
        if l.context != name {
            l.context = name.to_string();
            l.pending.clear();
            l.sample = (0, vec![]);
        }
    });
}

/// the harness's position in its recording (the `f` of the lines from here on)
pub fn at(k: u64) {
    if !on() {
        return;
    }
    LOCAL.with(|l| l.borrow_mut().at = Some(k));
}

/// something ran (shown by the next compared picture)
pub fn note(cat: &'static str, key: String) {
    if !on() {
        return;
    }
    LOCAL.with(|l| {
        l.borrow_mut().pending.insert((cat, key));
    });
}

/// what the world of `frame` holds
pub fn sample(frame: u64, items: Vec<(&'static str, String)>) {
    LOCAL.with(|l| l.borrow_mut().sample = (frame, items));
}

fn write(items: &mut dyn Iterator<Item = (&'static str, String)>, ctx: &str, frame: u64) {
    let Some(o) = out() else { return };
    let mut g = o.file.lock().unwrap();
    for (c, k) in items {
        *g.2.entry((c.to_string(), k.clone())).or_default() += 1;
        if g.1.insert((c.to_string(), k.clone(), ctx.to_string())) {
            let k = k.replace('\\', "/").replace('"', "'");
            let _ = writeln!(g.0, "{{\"l\":\"{}\",\"c\":\"{c}\",\"k\":\"{k}\",\"ctx\":\"{ctx}\",\"f\":{frame}}}", o.level);
        }
    }
}

/// a picture of `view` is compared: what ran before it, what its world held, and what it draws
pub fn commit(view: &crate::RenderView) {
    if !on() {
        return;
    }
    LOCAL.with(|l| {
        let mut l = l.borrow_mut();
        let ctx = if l.context.is_empty() { "?".to_string() } else { l.context.clone() };
        let mut items: Vec<(&'static str, String)> = std::mem::take(&mut l.pending).into_iter().collect();
        if l.sample.0 == view.frame {
            items.extend(l.sample.1.iter().cloned());
        }
        for s in &view.sprites {
            items.push(("sheet", s.sheet.clone()));
        }
        let mut part = |on: bool, name: &str| {
            if on {
                items.push(("part", name.to_string()));
            }
        };
        part(view.dialog.is_some(), "dialog");
        part(view.banner.is_some(), "banner");
        part(!view.hud.is_empty(), "hud");
        part(!view.boss_bars.is_empty(), "boss_bar");
        part(view.sphere.is_some(), "energy_sphere");
        part(view.timer.is_some(), "timer");
        part(!view.damage_numbers.is_empty(), "damage_number");
        part(view.boot.is_some(), "boot");
        part(!view.title.is_empty(), "title_text");
        part(!view.markers.is_empty(), "marker");
        part(view.palette_fade.is_some(), "palette_fade");
        part(view.palette_scale.is_some(), "script_fade");
        part(view.world_map.is_some(), "world_map");
        part(view.map.as_ref().is_some_and(|m| m.mode7.is_some()), "mode7");
        part(view.menu.is_some(), "menu");
        write(&mut items.into_iter(), &ctx, l.at.unwrap_or(view.frame));
    });
}

/// the counts so far (compared pictures / iterations each key was in), one line per key: a harness that wants the
/// frequencies calls this at its end (the whole-run replay: how often a player meets a thing)
pub fn flush_counts() {
    let Some(o) = out() else { return };
    let mut g = o.file.lock().unwrap();
    let lines: Vec<String> = g.2.iter().map(|((c, k), n)| format!("{{\"l\":\"{}\",\"c\":\"{c}\",\"k\":\"{}\",\"n\":{n}}}", o.level, k.replace('\\', "/").replace('"', "'"))).collect();
    for l in lines {
        let _ = writeln!(g.0, "{l}");
    }
}
