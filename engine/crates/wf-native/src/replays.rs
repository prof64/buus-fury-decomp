//! Desktop storage for recordings and save states, and the host side of `wf_app::control` requests.
//!
//! `<data>/replays/<id>.wfr` automatic recordings (newest `keep_replays` kept), `<data>/replays/kept/<id>.wfr`
//! kept / imported ones (never pruned), `<data>/states/slot<N>.wfs` save state slots, `<data>/hotkeys.json`.

use std::path::{Path, PathBuf};

use wf_app::control::{Control, Request};
use wf_app::replay_menu::{ReplayCmd, ReplayEntry, ReplayMenu};
use wf_app::wf_replay::Replay;
use wf_app::App;

pub struct Store {
    pub dir: PathBuf,
}

impl Store {
    pub fn new(data: &Path) -> Store {
        Store { dir: data.to_path_buf() }
    }
    fn auto(&self) -> PathBuf {
        self.dir.join("replays")
    }
    fn kept(&self) -> PathBuf {
        self.dir.join("replays/kept")
    }
    pub fn slot(&self, n: u8) -> PathBuf {
        self.dir.join(format!("states/slot{n}.wfs"))
    }
    /// Path of recording `id` (kept copy wins).
    pub fn path(&self, id: &str) -> PathBuf {
        let k = self.kept().join(format!("{id}.wfr"));
        if k.exists() {
            k
        } else {
            self.auto().join(format!("{id}.wfr"))
        }
    }

    pub fn list(&self) -> Vec<ReplayEntry> {
        let mut out = Vec::new();
        for (dir, kept) in [(self.kept(), true), (self.auto(), false)] {
            let Ok(rd) = std::fs::read_dir(&dir) else { continue };
            for e in rd.flatten() {
                let p = e.path();
                if p.extension().is_none_or(|x| x != "wfr") {
                    continue;
                }
                let Ok(b) = std::fs::read(&p) else { continue };
                let Ok(header) = Replay::peek_header(&b) else { continue };
                let id = p.file_stem().unwrap_or_default().to_string_lossy().into_owned();
                if out.iter().any(|x: &ReplayEntry| x.id == id) {
                    continue;
                }
                out.push(ReplayEntry { id, header, kept, size: b.len() });
            }
        }
        out.sort_by(|a, b| b.id.cmp(&a.id));
        out
    }

    pub fn save_replay(&self, id: &str, bytes: &[u8], keep: usize) {
        let k = self.kept().join(format!("{id}.wfr"));
        let p = if k.exists() { k } else { self.auto().join(format!("{id}.wfr")) };
        let _ = std::fs::create_dir_all(p.parent().unwrap());
        let tmp = p.with_extension("tmp");
        if std::fs::write(&tmp, bytes).is_ok() {
            let _ = std::fs::rename(&tmp, &p);
        }
        // prune the automatic ones
        if let Ok(rd) = std::fs::read_dir(self.auto()) {
            let mut v: Vec<PathBuf> = rd.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "wfr")).collect();
            v.sort();
            while v.len() > keep.max(1) {
                let _ = std::fs::remove_file(v.remove(0));
            }
        }
    }

    pub fn toggle_keep(&self, id: &str) {
        let (a, k) = (self.auto().join(format!("{id}.wfr")), self.kept().join(format!("{id}.wfr")));
        let _ = std::fs::create_dir_all(self.kept());
        if k.exists() {
            let _ = std::fs::rename(&k, &a);
        } else {
            let _ = std::fs::rename(&a, &k);
        }
    }

    pub fn import(&self, src: &Path) -> Result<String, String> {
        let b = std::fs::read(src).map_err(|e| e.to_string())?;
        let r = wf_app::control::parse_replay(&b)?;
        let id = format!("{}-import", src.file_stem().unwrap_or_default().to_string_lossy());
        let _ = std::fs::create_dir_all(self.kept());
        std::fs::write(self.kept().join(format!("{id}.wfr")), r.to_bytes()).map_err(|e| e.to_string())?;
        Ok(id)
    }
}

/// Carry out what the control layer asked for.
pub fn service(store: &Store, ctl: &mut Control, app: &mut App, menu: &mut ReplayMenu) {
    for q in ctl.take_requests() {
        match q {
            Request::SaveReplay { id, bytes, .. } => store.save_replay(&id, &bytes, ctl.hotkeys.keep_replays),
            Request::SaveState { slot, bytes } => {
                let p = store.slot(slot);
                let _ = std::fs::create_dir_all(p.parent().unwrap());
                if let Err(e) = std::fs::write(&p, bytes) {
                    ctl.toast(format!("Cannot save state: {e}"));
                }
            }
            Request::LoadState { slot } => match std::fs::read(store.slot(slot)) {
                Ok(b) => {
                    if let Err(e) = ctl.load_state(app, &b) {
                        ctl.toast(e);
                    }
                }
                Err(_) => ctl.toast(format!("State slot {slot} is empty")),
            },
            Request::OpenReplays => {
                ctl.finalize(app);
                // the finalize request is serviced on the next call; write it now so the list shows it
                for q in ctl.take_requests() {
                    if let Request::SaveReplay { id, bytes, .. } = q {
                        store.save_replay(&id, &bytes, ctl.hotkeys.keep_replays);
                    }
                }
                menu.info.clear();
                menu.show(store.list());
                app.paused = true;
            }
            // (the battle's end screen: the window loop goes back to the launcher, `Control::take_battle_select`)
            Request::BattleSelect => {}
        }
    }
}

/// A replay browser command (file dialogs via rfd).
pub fn command(store: &Store, ctl: &mut Control, app: &mut App, menu: &mut ReplayMenu, cmd: ReplayCmd) {
    let read = |id: &str| std::fs::read(store.path(id)).map_err(|e| e.to_string());
    let res: Result<(), String> = match cmd {
        ReplayCmd::None => Ok(()),
        ReplayCmd::Close => {
            app.paused = false;
            Ok(())
        }
        ReplayCmd::Play(id) => read(&id).and_then(|b| ctl.play_replay(app, &id, &b)).inspect(|_| app.paused = false),
        ReplayCmd::Ghost(id) => read(&id).and_then(|b| ctl.set_ghost(app, &id, &b)).inspect(|_| app.paused = false),
        ReplayCmd::Verify(id) => read(&id).and_then(|b| ctl.verify(app, &b)).map(|r| {
            menu.info = match r.divergence {
                None if r.ok => format!("{id}: OK, {} frames, {} checkpoints match", r.frames, r.checkpoints),
                None => format!("{id}: could not start"),
                Some(d) => format!("{id}: {d}"),
            };
        }),
        ReplayCmd::Keep(id) => {
            store.toggle_keep(&id);
            menu.show(store.list());
            Ok(())
        }
        ReplayCmd::Delete(id) => {
            let _ = std::fs::remove_file(store.path(&id));
            menu.show(store.list());
            Ok(())
        }
        ReplayCmd::Export(id) => {
            if let Some(p) = rfd::FileDialog::new().set_file_name(format!("{id}.wfr")).add_filter("Buu's Fury replay", &["wfr"]).save_file() {
                read(&id).and_then(|b| std::fs::write(&p, b).map_err(|e| e.to_string())).map(|_| menu.info = format!("exported {}", p.display()))
            } else {
                Ok(())
            }
        }
        ReplayCmd::ExportCurrent => match ctl.current_replay(app) {
            Some((id, r)) => {
                if let Some(p) = rfd::FileDialog::new().set_file_name(format!("{id}.wfr")).add_filter("Buu's Fury replay", &["wfr"]).save_file() {
                    std::fs::write(&p, r.to_bytes()).map_err(|e| e.to_string()).map(|_| menu.info = format!("exported {}", p.display()))
                } else {
                    Ok(())
                }
            }
            None => Err("nothing is being recorded (playback)".into()),
        },
        ReplayCmd::Import => {
            if let Some(p) = rfd::FileDialog::new().add_filter("replay", &["wfr", "json"]).pick_file() {
                store.import(&p).map(|id| {
                    menu.show(store.list());
                    menu.info = format!("imported as {id}");
                })
            } else {
                Ok(())
            }
        }
    };
    if let Err(e) = res {
        menu.info = e;
    }
}
