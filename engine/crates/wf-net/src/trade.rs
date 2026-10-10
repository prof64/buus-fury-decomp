//! Link play: the **Trading Post** (the game's link-cable trade, title -> Multiplayer -> Trading Post; strings 80..89
//! "Your Trade:" / "Friend's Trade:" / "Zenie" / "How many to trade?" / "Propose Trade" / "Accept" / "Decline" /
//! "You have accepted trade" / "TRADE" "COMPLETE!") over online play, with a **two-phase commit** so a dropped
//! connection or a crashed app can neither duplicate nor lose a trade. `engine/docs/netcode.md` "Link play: Trading
//! Post" has the flow, the decoded rules and the recovery rule.
//!
//! * No realtime need, no rollback: every [`TICK_MS`] each side sends one idempotent **state snapshot** ([`Snap`]:
//!   its offer + revision, its acceptance, and the state of every trade record it holds with this peer) over the
//!   ordinary unreliable channel; loss / reordering / duplicates only delay things.
//! * Negotiation is soft state (memory): offers carry a revision; an acceptance names both revisions, so a changed
//!   offer voids it. When both acceptances match, both sides derive the same trade id (a hash of both peers' ids,
//!   offers, revisions and session nonces).
//! * Commit is durable state ([`TradeStore`], one per side, each side writes only its own save): `Prepared`
//!   (validated, recorded, nothing applied, the save locked) -> `Committed` (applied atomically together with the
//!   record) -> `Done`. **Rule:** a side commits iff it is `Prepared` and sees the peer `Prepared` / `Committed` /
//!   `Done`; a `Prepared` side aborts only when the peer reports `Aborted`; a side reports `Aborted` for a trade id
//!   only after durably recording it, and it records `Aborted` only for an id it never prepared (validation failed,
//!   or the id is unknown to it - a restart lost the negotiation). So both sides end committed or both aborted; a
//!   side that is `Prepared` while the peer is away stays in doubt (the slot locked) until the peer is back - it
//!   never guesses.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::transport::{PeerChange, Transport};

/// the trade channel's packet tag (the transport also carries the lobby / match traffic)
pub const TAG: &[u8; 4] = b"TRD1";
/// snapshot period
pub const TICK_MS: f64 = 200.0;
/// negotiation: the friend is gone after this much silence ("Disconnected"; a prepared trade stays in doubt)
pub const SILENCE_MS: f64 = 10_000.0;
/// a supply count's ceiling: the save keeps one byte per supply and the game's trade adds with saturation at 0xFF
/// (`0x080231AE`, MEASURED from the ROM) - here a trade that would pass it is refused instead (nothing lost)
pub const SUPPLY_MAX: u32 = 255;
/// the equipment list's capacity: `0x08004448` (add equipment) ignores an add at 250 entries (MEASURED from the ROM)
/// - a trade that would pass it is refused
pub const EQUIP_MAX: usize = 250;

/// what one side puts on the table: supplies by index -> count, equipment by item id (one entry per copy), zenie
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Offer {
    pub supplies: BTreeMap<u16, u32>,
    pub equipment: Vec<i32>,
    pub zenie: u32,
}

impl Offer {
    pub fn is_empty(&self) -> bool {
        self.supplies.values().all(|&n| n == 0) && self.equipment.is_empty() && self.zenie == 0
    }
}

/// the part of a save a trade can touch
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Inventory {
    /// supply counts by supply index
    pub supplies: Vec<u32>,
    /// story items (key items) by supply index: not tradable (MEASURED: the trade's item picker `0x0801F44E` lists a
    /// supply only when bit 0 of its record's first halfword (`0x083BD2F8` + 0x34 x id) is clear - the items table's
    /// `story_item`: ids 25..82; tradable are the supplies 0..24, the equipment list, and zenie when not 0)
    pub story: Vec<bool>,
    /// the equipment list: (item id, worn). Worn entries are traded too: the picker lists every entry, and remove
    /// equipment (`0x08004468`) takes the first unworn copy, else the first worn one - its wearer's slot goes back to
    /// "-None-" (MEASURED: link recordings data/link/trade_*.json, Gohan's worn Cotton Gi / Leather Moccasins traded)
    pub equipment: Vec<(i32, bool)>,
    pub zenie: u32,
}

impl Inventory {
    /// `give` may leave this inventory: every count / copy (worn or not) / zenie is there, no story item
    pub fn check_give(&self, give: &Offer) -> Result<(), String> {
        for (&i, &n) in &give.supplies {
            if n == 0 {
                continue;
            }
            if self.story.get(i as usize).copied().unwrap_or(false) {
                return Err(format!("supply {i} is a story item and cannot be traded"));
            }
            let have = self.supplies.get(i as usize).copied().unwrap_or(0);
            if have < n {
                return Err(format!("supply {i}: offering {n}, have {have}"));
            }
        }
        let mut free: BTreeMap<i32, usize> = BTreeMap::new();
        for &(id, _) in &self.equipment {
            *free.entry(id).or_default() += 1;
        }
        let mut want: BTreeMap<i32, usize> = BTreeMap::new();
        for &id in &give.equipment {
            *want.entry(id).or_default() += 1;
        }
        for (id, n) in want {
            let have = free.get(&id).copied().unwrap_or(0);
            if have < n {
                return Err(format!("equipment {id}: offering {n}, have {have}"));
            }
        }
        if self.zenie < give.zenie {
            return Err(format!("zenie: offering {}, have {}", give.zenie, self.zenie));
        }
        Ok(())
    }

    /// this inventory after giving `give` and getting `get` (checked: [`Inventory::check_give`] and the receiving
    /// limits - a supply over [`SUPPLY_MAX`], the list over [`EQUIP_MAX`], zenie over u32)
    pub fn after(&self, give: &Offer, get: &Offer) -> Result<Inventory, String> {
        self.check_give(give)?;
        let mut inv = self.clone();
        for (&i, &n) in &give.supplies {
            inv.supplies[i as usize] -= n;
        }
        for id in &give.equipment {
            let k = inv.equipment.iter().position(|&(x, worn)| x == *id && !worn).or_else(|| inv.equipment.iter().position(|&(x, _)| x == *id)).unwrap();
            inv.equipment.remove(k);
        }
        inv.zenie -= give.zenie;
        for (&i, &n) in &get.supplies {
            let i = i as usize;
            if inv.story.get(i).copied().unwrap_or(false) && n > 0 {
                return Err(format!("the friend's supply {i} is a story item"));
            }
            if inv.supplies.len() <= i {
                inv.supplies.resize(i + 1, 0);
                inv.story.resize(i + 1, false);
            }
            let total = inv.supplies[i] + n;
            if total > SUPPLY_MAX {
                return Err(format!("supply {i}: {total} would pass the limit of {SUPPLY_MAX}"));
            }
            inv.supplies[i] = total;
        }
        for &id in &get.equipment {
            if inv.equipment.len() >= EQUIP_MAX {
                return Err(format!("the equipment list is full ({EQUIP_MAX})"));
            }
            inv.equipment.push((id, false));
        }
        inv.zenie = inv.zenie.checked_add(get.zenie).ok_or("zenie would overflow")?;
        Ok(inv)
    }
}

/// a trade record's durable state
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TState {
    /// never prepared and never will be (validation failed, declined after agreement, or unknown to this side)
    Aborted,
    /// validated and recorded; nothing applied; the save is locked until it resolves
    Prepared,
    /// applied (atomically with this record)
    Committed,
    /// committed and the peer is known to have committed too (cleanup; still answers like Committed)
    Done,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Record {
    pub id: String,
    pub peer: String,
    pub give: Offer,
    pub get: Offer,
    pub state: TState,
    /// why it aborted (the UI's message)
    #[serde(default)]
    pub note: String,
}

/// One side's durable storage: its save's inventory and its trade journal. Implementations must make each call
/// durable before returning, and [`TradeStore::commit`] atomic (the inventory change and the record's `Committed`
/// state land together or not at all - a write-ahead log is fine).
pub trait TradeStore {
    fn inventory(&self) -> Inventory;
    fn record(&self, id: &str) -> Option<Record>;
    /// every record (recovery scans them)
    fn records(&self) -> Vec<Record>;
    /// write a record (any state but Committed, which only [`TradeStore::commit`] writes)
    fn put(&mut self, r: &Record);
    /// apply `r.give` / `r.get` to the save and mark `r` Committed, atomically
    fn commit(&mut self, r: &Record) -> Result<(), String>;
}

/// An in-memory store (tests, tools): `inv` + `journal` are the "disk" - a test keeps a clone to "restart" from
#[derive(Clone, Debug, Default)]
pub struct MemStore {
    pub inv: Inventory,
    pub journal: BTreeMap<String, Record>,
}

impl TradeStore for MemStore {
    fn inventory(&self) -> Inventory {
        self.inv.clone()
    }
    fn record(&self, id: &str) -> Option<Record> {
        self.journal.get(id).cloned()
    }
    fn records(&self) -> Vec<Record> {
        self.journal.values().cloned().collect()
    }
    fn put(&mut self, r: &Record) {
        assert!(r.state != TState::Committed, "Committed is written by commit()");
        self.journal.insert(r.id.clone(), r.clone());
    }
    fn commit(&mut self, r: &Record) -> Result<(), String> {
        let inv = self.inv.after(&r.give, &r.get)?;
        self.inv = inv;
        self.journal.insert(r.id.clone(), Record { state: TState::Committed, ..r.clone() });
        Ok(())
    }
}

/// the snapshot each side sends every [`TICK_MS`]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Snap {
    pub nonce: u64,
    pub rev: u32,
    pub offer: Offer,
    /// "I accept your offer `their` with mine `mine`"
    pub accept: Option<(u32, u32)>,
    /// bumped by each Decline (the friend sees it once)
    pub declines: u32,
    /// the offer is proposed ("Propose Trade", R on the game's picker) - an offer still being put together is not
    #[serde(default)]
    pub proposed: bool,
    /// trade records with the receiver: (id, state)
    pub records: Vec<(String, TState)>,
}

pub fn encode(s: &Snap) -> Vec<u8> {
    let mut v = TAG.to_vec();
    v.extend(serde_json::to_vec(s).unwrap_or_default());
    v
}

pub fn decode(b: &[u8]) -> Option<Snap> {
    if b.len() < 4 || &b[..4] != TAG {
        return None;
    }
    serde_json::from_slice(&b[4..]).ok()
}

/// what the UI hears
#[derive(Clone, Debug, PartialEq)]
pub enum TradeEvent {
    /// "Friend's Trade:" changed
    FriendOffer(Offer),
    /// the friend accepted the current pair
    FriendAccepted,
    /// the friend declined
    Declined,
    /// both accepted: the commit runs (id)
    Agreed(String),
    /// "TRADE COMPLETE!" - applied to this side's save
    Complete(String),
    /// the trade did not happen (id, why); nothing applied
    Aborted(String, String),
    /// "Disconnected": the friend is gone; a prepared trade waits for the friend to come back (`in_doubt`)
    Disconnected { in_doubt: bool },
    /// the friend is back (a pending trade resumes)
    Reconnected,
}

/// One side of a Trading Post session with one friend: offers, acceptance, the commit. No I/O and no clock of its
/// own: [`TradePeer::tick`] gets the time and the transport (as wf-net's sessions).
pub struct TradePeer {
    pub me: String,
    pub peer: String,
    nonce: u64,
    rev: u32,
    offer: Offer,
    /// my acceptance: (their rev, my rev)
    accepted: Option<(u32, u32)>,
    declines: u32,
    proposed: bool,
    their_proposed: bool,
    their: Option<(u64, u32, Offer)>,
    their_accept: Option<(u32, u32)>,
    their_declines: u32,
    agreed: Option<String>,
    last_send: f64,
    last_heard: f64,
    away: bool,
    events: Vec<TradeEvent>,
    /// the friend's last reported state of each trade (memory only: what this session heard)
    friend_states: BTreeMap<String, TState>,
}

/// the trade id both sides derive: both (peer id, nonce, rev, offer), ordered by peer id
pub fn trade_id(a: (&str, u64, u32, &Offer), b: (&str, u64, u32, &Offer)) -> String {
    use sha1::{Digest, Sha1};
    let (x, y) = if a.0 <= b.0 { (a, b) } else { (b, a) };
    let mut h = Sha1::new();
    for p in [x, y] {
        h.update(p.0.as_bytes());
        h.update([0]);
        h.update(p.1.to_le_bytes());
        h.update(p.2.to_le_bytes());
        h.update(serde_json::to_vec(p.3).unwrap_or_default());
        h.update([0]);
    }
    h.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

impl TradePeer {
    /// a new session with `peer` (`nonce`: any per-run random number - a restarted app is a new negotiation).
    /// Records the store holds with this peer that are not settled resume at once (recovery).
    pub fn new(me: &str, peer: &str, nonce: u64, store: &dyn TradeStore) -> TradePeer {
        let mut p = TradePeer {
            me: me.to_string(),
            peer: peer.to_string(),
            nonce,
            rev: 0,
            offer: Offer::default(),
            accepted: None,
            declines: 0,
            proposed: false,
            their_proposed: false,
            their: None,
            their_accept: None,
            their_declines: 0,
            agreed: None,
            last_send: f64::NEG_INFINITY,
            last_heard: 0.0,
            away: false,
            events: Vec::new(),
            friend_states: BTreeMap::new(),
        };
        if p.pending(store).is_some() {
            p.events.push(TradeEvent::Disconnected { in_doubt: true });
        }
        p
    }

    /// a trade with this peer still to settle (Prepared, or Committed and not known Done) - the save stays locked
    pub fn pending(&self, store: &dyn TradeStore) -> Option<Record> {
        store
            .records()
            .into_iter()
            .find(|r| r.peer == self.peer && matches!(r.state, TState::Prepared | TState::Committed))
    }

    /// "Your Trade:" - a new offer (voids both acceptances)
    pub fn set_offer(&mut self, o: Offer) {
        if o != self.offer {
            self.offer = o;
            self.rev += 1;
            self.accepted = None;
            self.agreed = None;
        }
    }
    pub fn offer(&self) -> &Offer {
        &self.offer
    }
    /// "Propose Trade": `o` on the table as a proposal (a new revision even when the same offer: the friend sees a
    /// new proposal - an empty one too)
    pub fn propose(&mut self, o: Offer) {
        self.offer = o;
        self.rev += 1;
        self.proposed = true;
        self.accepted = None;
        self.agreed = None;
    }
    /// the proposal withdrawn (back to the picker): not proposed, acceptances void
    pub fn withdraw(&mut self) {
        self.proposed = false;
        self.rev += 1;
        self.accepted = None;
        self.agreed = None;
    }
    /// my acceptance withdrawn without declining (the game's B in the box after ACCEPT)
    pub fn unaccept(&mut self) {
        self.accepted = None;
        self.agreed = None;
    }
    /// the friend's offer is a proposal (its Propose Trade)
    pub fn friend_proposed(&self) -> bool {
        self.their.is_some() && self.their_proposed
    }
    /// the friend's offer revision (a new proposal = a new revision)
    pub fn friend_rev(&self) -> Option<u32> {
        self.their.as_ref().map(|t| t.1)
    }
    /// "Friend's Trade:"
    pub fn friend_offer(&self) -> Option<&Offer> {
        self.their.as_ref().map(|t| &t.2)
    }
    /// "Accept" (or "Propose Trade" before the friend's acceptance): accept the friend's current offer with mine.
    /// Refused when my side cannot do it (the offer is not in the save, or the result passes a limit).
    pub fn accept(&mut self, store: &dyn TradeStore) -> Result<(), String> {
        let Some((_, trev, tof)) = &self.their else { return Err("no offer from your friend yet".into()) };
        if self.pending(store).is_some() {
            return Err("a trade with this friend is still being completed".into());
        }
        store.inventory().after(&self.offer, tof)?;
        self.accepted = Some((*trev, self.rev));
        Ok(())
    }
    /// "Decline": withdraw my acceptance and tell the friend
    pub fn decline(&mut self) {
        self.accepted = None;
        self.agreed = None;
        self.declines += 1;
    }

    /// the friend's last reported state of trade `id` in this session (None: not heard)
    pub fn friend_state(&self, id: &str) -> Option<TState> {
        self.friend_states.get(id).copied()
    }

    /// trade `id` is settled on both sides as far as this side can know: mine Done **and** the friend has reported
    /// Done. Leave the session only then (or when the friend is gone): a side that leaves as soon as its own record is
    /// Done can take the last snapshot the friend needs with it - the friend, still Committed (it has not heard this
    /// side's commit yet), then waits for the next link (1 in 60 sessions at 31 % loss, 1 in 14 two-process
    /// runs). My Done snapshots keep going out until the friend's Done comes back, and the friend's Done requires my
    /// Committed / Done, so both reach Done in the same session unless the connection itself goes.
    pub fn settled(&self, id: &str, store: &dyn TradeStore) -> bool {
        store.record(id).is_some_and(|r| r.state == TState::Done) && self.friend_state(id) == Some(TState::Done)
    }

    pub fn events(&mut self) -> Vec<TradeEvent> {
        std::mem::take(&mut self.events)
    }

    fn snap(&self, store: &dyn TradeStore) -> Snap {
        let records = store
            .records()
            .into_iter()
            .filter(|r| r.peer == self.peer)
            .map(|r| (r.id, r.state))
            .collect();
        Snap { nonce: self.nonce, rev: self.rev, offer: self.offer.clone(), accept: self.accepted, declines: self.declines, proposed: self.proposed, records }
    }

    /// one step: read the transport, react, send the snapshot when due
    pub fn tick(&mut self, now: f64, net: &mut dyn Transport, store: &mut dyn TradeStore) {
        for c in net.changes() {
            match c {
                PeerChange::Disconnected(p) if p == self.peer => self.gone(store),
                PeerChange::Connected(p) if p == self.peer && self.away => {
                    self.away = false;
                    self.events.push(TradeEvent::Reconnected);
                }
                _ => {}
            }
        }
        for (from, data) in net.recv(now) {
            if from != self.peer {
                continue;
            }
            let Some(s) = decode(&data) else { continue };
            self.last_heard = now;
            if self.away {
                self.away = false;
                self.events.push(TradeEvent::Reconnected);
            }
            self.on_snap(s, store);
        }
        if !self.away && now - self.last_heard > SILENCE_MS && self.their.is_some() {
            self.gone(store);
        }
        self.advance(store);
        if now - self.last_send >= TICK_MS {
            self.last_send = now;
            net.send(&self.peer, encode(&self.snap(store)), now);
        }
    }

    fn gone(&mut self, store: &dyn TradeStore) {
        if self.away {
            return;
        }
        self.away = true;
        // the negotiation is memory: it ends; a prepared / committed trade stays (journal) and resumes
        self.their = None;
        self.their_accept = None;
        self.accepted = None;
        self.agreed = None;
        self.events.push(TradeEvent::Disconnected { in_doubt: self.pending(store).is_some() });
    }

    fn on_snap(&mut self, s: Snap, store: &mut dyn TradeStore) {
        // the negotiation (soft state)
        self.their_proposed = s.proposed;
        let changed = self.their.as_ref().is_none_or(|t| t.0 != s.nonce || t.1 != s.rev);
        if changed {
            self.their = Some((s.nonce, s.rev, s.offer.clone()));
            self.accepted = None;
            self.agreed = None;
            self.events.push(TradeEvent::FriendOffer(s.offer.clone()));
        }
        if s.declines != self.their_declines {
            self.their_declines = s.declines;
            if s.declines > 0 {
                self.accepted = None;
                self.agreed = None;
                self.events.push(TradeEvent::Declined);
            }
        }
        if s.accept != self.their_accept {
            self.their_accept = s.accept;
            if s.accept == Some((self.rev, s.rev)) {
                self.events.push(TradeEvent::FriendAccepted);
            }
        }
        // both accepted the same pair: agreed
        if let (Some((trev, mrev)), Some((their_mine, their_theirs))) = (self.accepted, self.their_accept) {
            if trev == s.rev && mrev == self.rev && their_mine == self.rev && their_theirs == s.rev && self.agreed.is_none() {
                let id = trade_id((&self.me, self.nonce, self.rev, &self.offer), (&self.peer, s.nonce, s.rev, &s.offer));
                self.agreed = Some(id.clone());
                self.prepare(id, store);
            }
        }
        // the commit (durable state)
        for (id, st) in s.records {
            self.friend_states.insert(id.clone(), st);
            match store.record(&id) {
                None => {
                    if self.agreed.as_deref() == Some(id.as_str()) {
                        // (the agreement above prepared it: cannot be None here)
                        continue;
                    }
                    if matches!(st, TState::Prepared) {
                        // a trade I never prepared (a restart lost the negotiation, or my prepare failed to be
                        // reached): durably refuse it, so the friend aborts too
                        store.put(&Record { id: id.clone(), peer: self.peer.clone(), give: Offer::default(), get: Offer::default(), state: TState::Aborted, note: "unknown to this side".into() });
                    }
                }
                Some(r) => match (r.state, st) {
                    (TState::Prepared, TState::Prepared | TState::Committed | TState::Done) => {
                        match store.commit(&r) {
                            Ok(()) => self.events.push(TradeEvent::Complete(id.clone())),
                            // cannot happen while the save is locked (validated at prepare); never abort a prepared
                            // trade on our own - stay in doubt and report
                            Err(e) => self.events.push(TradeEvent::Aborted(id.clone(), format!("commit failed, still pending: {e}"))),
                        }
                    }
                    (TState::Prepared, TState::Aborted) => {
                        store.put(&Record { state: TState::Aborted, note: "the friend aborted".into(), ..r });
                        self.events.push(TradeEvent::Aborted(id.clone(), "the friend could not complete the trade".into()));
                    }
                    (TState::Committed, TState::Committed | TState::Done) => {
                        store.put(&Record { state: TState::Done, ..r });
                    }
                    _ => {}
                },
            }
        }
    }

    /// agreed: validate and record Prepared (or Aborted)
    fn prepare(&mut self, id: String, store: &mut dyn TradeStore) {
        if store.record(&id).is_some() {
            return;
        }
        let Some((_, _, get)) = self.their.clone() else { return };
        self.events.push(TradeEvent::Agreed(id.clone()));
        let r = Record { id: id.clone(), peer: self.peer.clone(), give: self.offer.clone(), get, state: TState::Prepared, note: String::new() };
        let ok = if self.pending(store).is_some() { Err("another trade is pending".to_string()) } else { store.inventory().after(&r.give, &r.get).map(|_| ()) };
        match ok {
            Ok(()) => store.put(&r),
            Err(e) => {
                store.put(&Record { state: TState::Aborted, note: e.clone(), ..r });
                self.events.push(TradeEvent::Aborted(id, e));
            }
        }
    }

    /// nothing time-driven beyond the snapshot: the commit only moves on what the friend reports (in doubt until then)
    fn advance(&mut self, _store: &mut dyn TradeStore) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    fn inv(supplies: &[u32], equip: &[(i32, bool)], zenie: u32) -> Inventory {
        let mut story = vec![false; 83];
        story[60] = true;
        let mut s = supplies.to_vec();
        s.resize(83, 0);
        Inventory { supplies: s, story, equipment: equip.to_vec(), zenie }
    }

    #[test]
    fn rules() {
        let a = inv(&[5, 0, 254], &[(7, true), (7, false), (9, false)], 100);
        let mut o = Offer::default();
        o.supplies.insert(0, 5);
        assert!(a.check_give(&o).is_ok());
        o.supplies.insert(0, 6);
        assert!(a.check_give(&o).is_err());
        // (worn copies trade too - the unworn one goes first)
        let two = Offer { equipment: vec![7], ..Default::default() };
        assert_eq!(a.after(&two, &Offer::default()).unwrap().equipment, vec![(7, true), (9, false)]);
        assert!(a.check_give(&Offer { equipment: vec![7, 7], ..Default::default() }).is_ok());
        assert!(a.check_give(&Offer { equipment: vec![7, 7, 7], ..Default::default() }).is_err(), "two copies only");
        let mut story = Offer::default();
        story.supplies.insert(60, 1);
        assert!(inv(&[], &[], 0).check_give(&story).is_err());
        let mut get = Offer::default();
        get.supplies.insert(2, 2);
        assert!(a.after(&Offer::default(), &get).is_err(), "254 + 2 passes 255");
        get.supplies.insert(2, 1);
        assert_eq!(a.after(&Offer::default(), &get).unwrap().supplies[2], 255);
        let full = inv(&[], &vec![(1, false); 250], 0);
        assert!(full.after(&Offer::default(), &Offer { equipment: vec![3], ..Default::default() }).is_err());
        assert!(full.after(&Offer { equipment: vec![1], ..Default::default() }, &Offer { equipment: vec![3], ..Default::default() }).is_ok());
    }
}
