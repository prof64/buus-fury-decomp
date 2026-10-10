p = '/Users/prof/source/buus-fury-decomp/engine/crates/wf-app/tests/menu_parity.rs'
s = open(p).read()
a = """            if oracle {
                let m = &mut sim.session.world.menu;
                if ticks {
                    if let Some(p) = m.pending.take() {
                        m.shown = Some(p);
                    }
                    m.lag = 0;
                } else {
                    m.lag = quiet.max(1);
                }
            }"""
b = """            if oracle {
                let m = &mut sim.session.world.menu;
                // the display: a finished redraw shows from the frame the emulator's picture changes (a frame whose
                // picture differs from both neighbours is a VRAM copy racing the scanout: shown from the next one)
                let changed = hs[k] != hs[k - 1] && !tear(&hs, k);
                if changed {
                    if let Some(p) = m.pending.take() {
                        m.shown = Some(p);
                    }
                }
                // the logic: the emulator's mode loop iterations
                m.lag = if ticks { 0 } else { quiet.saturating_add(1) };
            }"""
assert a in s
s = s.replace(a, b)
a = """    let lf: Vec<i64> = rows.iter().map(|r| i(&r["lf"])).collect();"""
b = """    let lf: Vec<i64> = rows.iter().map(|r| i(&r["lf"])).collect();
    let hs: Vec<String> = rows.iter().map(|r| r["h"].as_str().unwrap_or("").to_string()).collect();
    let mut tears = 0;"""
s = s.replace(a, b)
a = """        let Some(emu) = load_png(&d.join(format!("f{k:04}.png"))) else { continue };"""
b = """        let Some(emu) = load_png(&d.join(format!("f{k:04}.png"))) else { continue };
        if k > 0 && tear(&hs, k) {
            tears += 1;
            if log {
                eprintln!("{name} {k:4} keys {keys:03x} tear frame (VRAM copy during the scanout), not compared");
            }
            continue;
        }"""
s = s.replace(a, b)
a = """    println!("menu {name}: lag model agrees on {} / {} menu frames", lag_pred.0, lag_pred.1);"""
b = """    println!("menu {name}: lag model agrees on {} / {} menu frames; {tears} tear frames skipped", lag_pred.0, lag_pred.1);"""
s = s.replace(a, b)
a = """fn dir() -> PathBuf {"""
b = """/// a picture differing from both neighbours: the VRAM copy raced the scanout (sub-frame CPU timing)
fn tear(hs: &[String], k: usize) -> bool {
    k > 0 && k + 1 < hs.len() && hs[k] != hs[k - 1] && hs[k] != hs[k + 1] && hs[k - 1] != hs[k + 1]
}

fn dir() -> PathBuf {"""
s = s.replace(a, b)
open(p, 'w').write(s)
print('ok')
