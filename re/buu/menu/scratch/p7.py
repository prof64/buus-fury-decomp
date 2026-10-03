p = '/Users/prof/source/legacy-of-goku-x/.claude/worktrees/re-dashboard/engine/crates/wf-app/tests/menu_parity.rs'
s = open(p).read()

s = s.replace('''//! Menu screens vs the emulator, frame by frame (headless, muted; skipped without the ROM or without the captures
//! of `re/buu/menu/capture.py` in `<repo>/work/buu/menu/<scenario>/`, `WF_MENU_DIR` overrides).
//!
//! Each scenario starts from a save state; the capture decodes the GameData block at the START frame
//! (`story.json`), the remake's story session gets that state, then both run the same keys from that frame on.
//! Compared per video frame: the sounds the menu plays and, while the remake shows a menu screen, the whole
//! 240x160 picture (wf-core menu view -> wf-render). `MENU_PARITY_OUT=<dir>` writes emulator | ours | diff strips
//! of differing frames; `MENU_PARITY_LOG=1` prints every frame.''', '''//! Menu screens vs the emulator, tick by tick and frame by frame (headless, muted; skipped without the ROM).
//!
//! Traces: `<repo>/data/menu/<scenario>.json` (`re/buu/menu/capture.py`: keys, the mode loop's frame counter,
//! sound calls and the sha1 of the emulator's screen per video frame, the GameData block decoded at frame 0 and how
//! the screen opens). The remake's story session gets that state, then both run the same keys. Compared per video
//! frame: the sounds and, while the remake shows a menu screen, the sha1 of the 240x160 picture (wf-core menu view
//! -> wf-render). CPU time is an oracle from the trace (see `run`). Pictures for diffs: `work/buu/menu/<scenario>/`
//! (git-ignored, ROM-derived; `WF_MENU_DIR` overrides): `MENU_PARITY_OUT=<dir>` writes emulator | ours | diff
//! strips of differing frames; `MENU_PARITY_LOG=1` prints every frame; `MENU_PARITY_ONLY=<substring>` filters.
//! Scenarios whose names start with a `STRICT` prefix must be exact (sounds and every compared frame).''')

s = s.replace('''use std::path::{Path, PathBuf};''', '''use sha1::{Digest, Sha1};
use std::path::{Path, PathBuf};''')

s = s.replace('''fn dir() -> PathBuf {
    std::env::var("WF_MENU_DIR").map(PathBuf::from).unwrap_or_else(|_| wf_rom::testutil::repo_root().join("work/buu/menu"))
}''', '''fn dir() -> PathBuf {
    std::env::var("WF_MENU_DIR").map(PathBuf::from).unwrap_or_else(|_| wf_rom::testutil::repo_root().join("work/buu/menu"))
}

fn traces() -> PathBuf {
    wf_rom::testutil::repo_root().join("data/menu")
}

/// scenarios that must be exact (prefixes)
const STRICT: &[&str] = &["status_"];

/// the capture's picture hash: sha1 of the RGB bytes, first 16 hex digits
fn hash(fb: &[u32]) -> String {
    let mut h = Sha1::new();
    let mut rgb = Vec::with_capacity(240 * 160 * 3);
    for &c in fb.iter().take(240 * 160) {
        rgb.extend_from_slice(&[(c >> 16) as u8, (c >> 8) as u8, c as u8]);
    }
    h.update(&rgb);
    h.finalize().iter().take(8).map(|b| format!("{b:02x}")).collect()
}''')

a = '''fn run(packs: &Packs, data: &Arc<wf_core::content::GameData>, name: &str) -> Option<(usize, usize, Vec<String>, Vec<usize>)> {
    let d = dir().join(name);
    let frames: Value = serde_json::from_slice(&std::fs::read(d.join("frames.json")).ok()?).ok()?;
    let sj: Value = serde_json::from_slice(&std::fs::read(d.join("story.json")).ok()?).ok()?;
    let story = story_from(data, &sj);'''
b = '''fn run(packs: &Packs, data: &Arc<wf_core::content::GameData>, name: &str) -> Option<(usize, usize, Vec<String>, Vec<usize>)> {
    let d = dir().join(name);
    let tr: Value = serde_json::from_slice(&std::fs::read(traces().join(format!("{name}.json"))).ok()?).ok()?;
    let frames = &tr;
    let sj = &tr["story"];
    let story = story_from(data, sj);'''
assert a in s
s = s.replace(a, b)
a = '''    // script-opened screens: the remake opens the screen the frame the emulator's script did
    let meta: Value = std::fs::read(d.join("meta.json")).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or(Value::Null);'''
b = '''    // script-opened screens: the remake opens the screen the frame the emulator's script did
    let meta = &tr["meta"];'''
assert a in s
s = s.replace(a, b)
a = '''        let Some(emu) = load_png(&d.join(format!("f{k:04}.png"))) else { continue };
        if k > 0 && tear(&hs, k) {'''
b = '''        if k > 0 && tear(&hs, k) {'''
assert a in s
s = s.replace(a, b)
a = '''        let ours = r.render(packs, &v, 0, 240, 160).to_vec();
        let diff = emu.iter().zip(&ours).filter(|(a, b)| a != b).count();
        compared += 1;
        if diff == 0 {
            exact += 1;
        } else {
            bad_frames.push(k);
            if let Some(o) = &out {
                std::fs::create_dir_all(o).ok();
                let dm: Vec<u32> = emu.iter().zip(&ours).map(|(a, b)| if a == b { 0xFF00_0000 } else { 0xFFFF_00FF }).collect();
                save(&o.join(format!("{name}_{k:04}.png")), &[&emu, &ours, &dm]);
            }
        }'''
b = '''        let ours = r.render(packs, &v, 0, 240, 160).to_vec();
        compared += 1;
        let same = hash(&ours) == hs[k];
        let mut diff = 0;
        if same {
            exact += 1;
        } else {
            bad_frames.push(k);
            if let Some(emu) = load_png(&d.join(format!("f{k:04}.png"))) {
                diff = emu.iter().zip(&ours).filter(|(a, b)| a != b).count();
                if let Some(o) = &out {
                    std::fs::create_dir_all(o).ok();
                    let dm: Vec<u32> = emu.iter().zip(&ours).map(|(a, b)| if a == b { 0xFF00_0000 } else { 0xFFFF_00FF }).collect();
                    save(&o.join(format!("{name}_{k:04}.png")), &[&emu, &ours, &dm]);
                }
            }
        }'''
assert a in s
s = s.replace(a, b)
a = '''    let Some(rom) = wf_rom::testutil::rom() else { return };
    if !dir().exists() {
        return;
    }'''
b = '''    let Some(rom) = wf_rom::testutil::rom() else { return };
    if !traces().exists() {
        return;
    }'''
assert a in s
s = s.replace(a, b)
a = '''    let mut names: Vec<String> = std::fs::read_dir(dir()).unwrap().filter_map(|e| e.ok()).filter(|e| e.path().join("frames.json").exists()).map(|e| e.file_name().to_string_lossy().to_string()).collect();'''
b = '''    let mut names: Vec<String> = std::fs::read_dir(traces())
        .unwrap()
        .filter_map(|e| e.ok())
        .filter_map(|e| e.file_name().to_string_lossy().strip_suffix(".json").map(String::from))
        .collect();'''
assert a in s
s = s.replace(a, b)
a = '''    let mut all_ok = true;
    for n in &names {
        let Some((cmp, ex, snd, bad)) = run(&packs, &data, n) else { continue };
        println!("menu {n}: {ex} / {cmp} frames exact; sound mismatches {}; differing frames {:?}", snd.len(), &bad[..bad.len().min(40)]);
        for s in snd.iter().take(10) {
            println!("  {s}");
        }
        if ex != cmp || !snd.is_empty() {
            all_ok = false;
        }
    }
    if std::env::var("MENU_PARITY_STRICT").is_ok() {
        assert!(all_ok);
    }'''
b = '''    let mut failed = Vec::new();
    let strict_all = std::env::var("MENU_PARITY_STRICT").is_ok();
    for n in &names {
        let Some((cmp, ex, snd, bad)) = run(&packs, &data, n) else { continue };
        println!("menu {n}: {ex} / {cmp} frames exact; sound mismatches {}; differing frames {:?}", snd.len(), &bad[..bad.len().min(40)]);
        for s in snd.iter().take(10) {
            println!("  {s}");
        }
        let strict = strict_all || STRICT.iter().any(|p| n.starts_with(p));
        if strict && (ex != cmp || !snd.is_empty()) {
            failed.push(n.clone());
        }
    }
    assert!(failed.is_empty(), "menu scenarios not exact: {failed:?}");'''
assert a in s
s = s.replace(a, b)
open(p, 'w').write(s)

p = '/Users/prof/source/legacy-of-goku-x/.claude/worktrees/re-dashboard/engine/crates/wf-app/Cargo.toml'
s = open(p).read()
if 'sha1' not in s.split('[dev-dependencies]')[1]:
    s = s.replace('[dev-dependencies]\npng = { workspace = true }', '[dev-dependencies]\npng = { workspace = true }\nsha1 = { workspace = true }')
open(p, 'w').write(s)
print('ok')
