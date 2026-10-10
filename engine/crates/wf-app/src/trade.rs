//! The Trading Post's durable side ([`wf_net::trade::TradeStore`]) on a real save slot file (the native shell's
//! `<data dir>/saves/slotN.sav`, the bytes of [`wf_core::menu::save::Saves::slot_bytes`]) and a journal next to it
//! (`slotN.sav.trade.json`). Every write is a temp file + rename (atomic on one file system). The commit is a
//! write-ahead log over the two files: (1) the journal records the save's post-trade bytes (`wal`), (2) the slot file
//! is replaced with them, (3) the journal marks the trade Committed and drops the `wal`. Opening the store finishes a
//! commit a crash interrupted (the slot holds the pre- or the post-trade bytes; either way it ends post-trade,
//! once) - see engine/docs/netcode.md "Link play: Trading Post".

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use serde::{Deserialize, Serialize};
use wf_core::menu::save::{SaveData, Saves};
use wf_net::trade::{Inventory, Offer, Record, TState, TradeStore};

/// the "-None-" item of each equipment slot (as `wf_core::menu::equip::NONE_ITEM`)
const NONE_ITEM: [i32; 4] = [0, 44, 77, 112];

/// the save's equipment list with the wearers (character, slot): the game marks the first matching unworn entry per
/// worn item, as `wf_core::menu::equip::marks`
fn wearers(sd: &SaveData) -> Vec<Option<(usize, usize)>> {
    let st = &sd.story;
    let mut m = vec![None; st.equipment.len()];
    for (c, cs) in st.chars.iter().enumerate() {
        for s in 0..4 {
            let id = cs.equip[s];
            if id == NONE_ITEM[s] {
                continue;
            }
            if let Some(k) = (0..m.len()).find(|&k| m[k].is_none() && st.equipment[k] == id) {
                m[k] = Some((c, s));
            }
        }
    }
    m
}

fn worn(sd: &SaveData) -> Vec<bool> {
    wearers(sd).iter().map(|w| w.is_some()).collect()
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
    // (remove equipment `0x08004468`: the first unworn copy, else the first worn one - its wearer's slot back to
    // "-None-"; link recordings data/link/trade_*.json)
    for id in &give.equipment {
        let w = wearers(&out);
        let st = &mut out.story;
        let k = (0..st.equipment.len())
            .find(|&k| st.equipment[k] == *id && w[k].is_none())
            .or_else(|| (0..st.equipment.len()).find(|&k| st.equipment[k] == *id))
            .ok_or("equipment gone")?;
        if let Some((c, s)) = w[k] {
            st.chars[c].equip[s] = NONE_ITEM[s];
        }
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

/// Where the Trading Post's files live: the file system (native, the default) or the browser's storage (the web
/// build installs its own with [`set_disk`]). `write` must be durable and atomic per path when it returns - the
/// commit's write-ahead log is built from those single-path writes (see the module doc).
pub trait Disk {
    fn read(&self, path: &Path) -> Option<Vec<u8>>;
    fn write(&self, path: &Path, bytes: &[u8]) -> Result<(), String>;
}

thread_local! {
    static DISK: RefCell<Option<Rc<dyn Disk>>> = const { RefCell::new(None) };
}

/// install a [`Disk`] for this thread (None = the file system)
pub fn set_disk(d: Option<Rc<dyn Disk>>) {
    DISK.with(|x| *x.borrow_mut() = d);
}

/// read a file through the installed [`Disk`] (default: the file system)
pub fn disk_read(path: &Path) -> Option<Vec<u8>> {
    if let Some(d) = DISK.with(|x| x.borrow().clone()) {
        return d.read(path);
    }
    std::fs::read(path).ok()
}

/// write a file atomically through the installed [`Disk`] (default: a temp file + rename; the folder is made)
pub fn disk_write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if let Some(d) = DISK.with(|x| x.borrow().clone()) {
        return d.write(path, bytes);
    }
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    write_atomic(path, bytes).map_err(|e| e.to_string())
}

pub fn read_slot(path: &Path) -> Option<SaveData> {
    let b = disk_read(path)?;
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
        let j: Journal = match disk_read(&journal_path) {
            Some(b) => serde_json::from_slice(&b).map_err(|e| format!("{}: {e}", journal_path.display()))?,
            None => Journal::default(),
        };
        let mut s = FileStore { slot: slot.to_path_buf(), journal_path, story, j, crash_after: None };
        s.recover()?;
        Ok(s)
    }

    /// a commit a crash interrupted: the slot ends with the post-trade bytes and the record Committed
    fn recover(&mut self) -> Result<(), String> {
        let Some((id, post)) = self.j.wal.clone() else { return Ok(()) };
        let post = unhex(&post);
        let now = disk_read(&self.slot).unwrap_or_default();
        if now != post {
            disk_write(&self.slot, &post)?;
        }
        if let Some(r) = self.j.records.get_mut(&id) {
            r.state = TState::Committed;
        }
        self.j.wal = None;
        self.save_journal()
    }

    fn save_journal(&self) -> Result<(), String> {
        disk_write(&self.journal_path, &serde_json::to_vec_pretty(&self.j).unwrap_or_default())
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
        disk_write(&self.slot, &bytes)?;
        if self.crash_after == Some(2) {
            return Err("test crash after the slot".into());
        }
        // (3) done
        self.j.records.insert(r.id.clone(), Record { state: TState::Committed, ..r.clone() });
        self.j.wal = None;
        self.save_journal()
    }
}
