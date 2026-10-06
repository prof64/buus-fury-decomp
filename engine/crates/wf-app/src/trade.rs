//! The Trading Post's durable side ([`wf_net::trade::TradeStore`]) on a real save slot file (the native shell's
//! `<data dir>/saves/slotN.sav`, the bytes of [`wf_core::menu::save::Saves::slot_bytes`]) and a journal next to it
//! (`slotN.sav.trade.json`). Every write is a temp file + rename (atomic on one file system). The commit is a
//! write-ahead log over the two files: (1) the journal records the save's post-trade bytes (`wal`), (2) the slot file
//! is replaced with them, (3) the journal marks the trade Committed and drops the `wal`. Opening the store finishes a
//! commit a crash interrupted (the slot holds the pre- or the post-trade bytes; either way it ends post-trade,
//! once) - see engine/docs/netcode.md "Link play: Trading Post".

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use wf_core::menu::save::{SaveData, Saves};
use wf_net::trade::{Inventory, Offer, Record, TState, TradeStore};

/// the "-None-" item of each equipment slot (as `wf_core::menu::equip::NONE_ITEM`)
const NONE_ITEM: [i32; 4] = [0, 44, 77, 112];

/// the save's equipment list with the worn marks (the game marks the first matching unworn entry per worn item, as
/// `wf_core::menu::equip::marks`)
fn worn(sd: &SaveData) -> Vec<bool> {
    let st = &sd.story;
    let mut m = vec![false; st.equipment.len()];
    for cs in &st.chars {
        for s in 0..4 {
            let id = cs.equip[s];
            if id == NONE_ITEM[s] {
                continue;
            }
            if let Some(k) = (0..m.len()).find(|&k| !m[k] && st.equipment[k] == id) {
                m[k] = true;
            }
        }
    }
    m
}

/// the part of a save a trade can touch; `story`: the content's story-item flags by supply index
pub fn inventory_of(sd: &SaveData, story: &[bool]) -> Inventory {
    let st = &sd.story;
    let w = worn(sd);
    Inventory {
        supplies: st.supplies.iter().map(|&n| n as u32).collect(),
        story: (0..st.supplies.len().max(story.len())).map(|i| story.get(i).copied().unwrap_or(false)).collect(),
        equipment: st.equipment.iter().zip(w).map(|(&id, w)| (id, w)).collect(),
        zenie: st.zenie,
    }
}

/// the save after giving `give` and getting `get` (checked by [`Inventory::after`] first)
pub fn apply(sd: &SaveData, story: &[bool], give: &Offer, get: &Offer) -> Result<SaveData, String> {
    inventory_of(sd, story).after(give, get)?;
    let mut out = sd.clone();
    let st = &mut out.story;
    for (&i, &n) in &give.supplies {
        st.supplies[i as usize] -= n as u8;
    }
    for id in &give.equipment {
        let w = worn(&out);
        let st = &mut out.story;
        let k = (0..st.equipment.len()).find(|&k| st.equipment[k] == *id && !w[k]).ok_or("equipment gone")?;
        st.equipment.remove(k);
    }
    let st = &mut out.story;
    st.zenie -= give.zenie;
    for (&i, &n) in &get.supplies {
        let i = i as usize;
        if st.supplies.len() <= i {
            st.supplies.resize(i + 1, 0);
        }
        st.supplies[i] += n as u8;
    }
    st.equipment.extend(get.equipment.iter().copied());
    st.zenie += get.zenie;
    Ok(out)
}

pub fn read_slot(path: &Path) -> Option<SaveData> {
    let b = std::fs::read(path).ok()?;
    let mut s = Saves::default();
    s.set_slot_bytes(0, &b);
    s.slot(0).cloned()
}

pub fn slot_bytes(sd: &SaveData) -> Vec<u8> {
    let mut s = Saves::default();
    s.set(0, Some(sd.clone()));
    s.slot_bytes(0)
}

fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let tmp = path.with_extension("tmp-trade");
    std::fs::write(&tmp, bytes)?;
    if let Ok(f) = std::fs::File::open(&tmp) {
        let _ = f.sync_all();
    }
    std::fs::rename(&tmp, path)
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct Journal {
    records: BTreeMap<String, Record>,
    /// a commit in progress: (trade id, the slot's post-trade bytes, hex)
    #[serde(default)]
    wal: Option<(String, String)>,
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}
fn unhex(s: &str) -> Vec<u8> {
    (0..s.len() / 2).filter_map(|i| u8::from_str_radix(&s[2 * i..2 * i + 2], 16).ok()).collect()
}

/// A slot file + its trade journal as a [`TradeStore`]
pub struct FileStore {
    pub slot: PathBuf,
    pub journal_path: PathBuf,
    story: Vec<bool>,
    j: Journal,
    /// tests: stop a commit after its WAL step `n` (1: journal written, 2: slot replaced) - a crash there
    pub crash_after: Option<u8>,
}

impl FileStore {
    /// open (and recover): `story` = the content's story-item flags by supply index
    pub fn open(slot: &Path, story: Vec<bool>) -> Result<FileStore, String> {
        let journal_path = PathBuf::from(format!("{}.trade.json", slot.display()));
        let j: Journal = match std::fs::read(&journal_path) {
            Ok(b) => serde_json::from_slice(&b).map_err(|e| format!("{}: {e}", journal_path.display()))?,
            Err(_) => Journal::default(),
        };
        let mut s = FileStore { slot: slot.to_path_buf(), journal_path, story, j, crash_after: None };
        s.recover()?;
        Ok(s)
    }

    /// a commit a crash interrupted: the slot ends with the post-trade bytes and the record Committed
    fn recover(&mut self) -> Result<(), String> {
        let Some((id, post)) = self.j.wal.clone() else { return Ok(()) };
        let post = unhex(&post);
        let now = std::fs::read(&self.slot).unwrap_or_default();
        if now != post {
            write_atomic(&self.slot, &post).map_err(|e| e.to_string())?;
        }
        if let Some(r) = self.j.records.get_mut(&id) {
            r.state = TState::Committed;
        }
        self.j.wal = None;
        self.save_journal()
    }

    fn save_journal(&self) -> Result<(), String> {
        write_atomic(&self.journal_path, &serde_json::to_vec_pretty(&self.j).unwrap_or_default()).map_err(|e| e.to_string())
    }

    pub fn save(&self) -> Option<SaveData> {
        read_slot(&self.slot)
    }
}

impl TradeStore for FileStore {
    fn inventory(&self) -> Inventory {
        self.save().map(|sd| inventory_of(&sd, &self.story)).unwrap_or_default()
    }
    fn record(&self, id: &str) -> Option<Record> {
        self.j.records.get(id).cloned()
    }
    fn records(&self) -> Vec<Record> {
        self.j.records.values().cloned().collect()
    }
    fn put(&mut self, r: &Record) {
        self.j.records.insert(r.id.clone(), r.clone());
        if let Err(e) = self.save_journal() {
            eprintln!("trade journal: {e}");
        }
    }
    fn commit(&mut self, r: &Record) -> Result<(), String> {
        let sd = self.save().ok_or("the save slot is gone")?;
        let post = apply(&sd, &self.story, &r.give, &r.get)?;
        let bytes = slot_bytes(&post);
        // (1) the WAL
        self.j.wal = Some((r.id.clone(), hex(&bytes)));
        self.save_journal()?;
        if self.crash_after == Some(1) {
            return Err("test crash after the WAL".into());
        }
        // (2) the slot
        write_atomic(&self.slot, &bytes).map_err(|e| e.to_string())?;
        if self.crash_after == Some(2) {
            return Err("test crash after the slot".into());
        }
        // (3) done
        self.j.records.insert(r.id.clone(), Record { state: TState::Committed, ..r.clone() });
        self.j.wal = None;
        self.save_journal()
    }
}
