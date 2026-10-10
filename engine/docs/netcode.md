# Netcode: online multiplayer over the deterministic core

`engine/crates/wf-net` adds online play to the remake: GGPO-style **rollback** over `wf-core`'s battle `Session`,
peer-to-peer over **WebRTC data channels**, rooms by code through a **Matchbox** signaling server, co-op (2–4 players
vs any enemies / bosses) and **PvP**, spectators, per-frame desync detection with reports, a local **sync test**, and
every match saved as a **wf-replay** recording. Native (`buus-fury --net-*`) and browser (`/app/?net`) peers play in the
same room. Across the internet: STUN / **TURN** (coturn or the bundled `wf-turn`, time-limited credentials handed out
by the signaling server), **wss://** signaling, room tokens and a server key ([§6](#6-internet-play), deploy recipe in
`engine/deploy/netplay/`). Also an **in-game lobby** (game font, native + web), a menu that does not stall the match
(or a shared pause), **rollback-aware sounds**, **rejoin** mid-match (snapshot + hash check) and PvP homing.

```
cargo build --release -p wf-native -p wf-net --features wf-net/server,wf-net/turn --bin buus-fury --bin wf-signal --bin wf-turn
target/release/wf-signal                                   # signaling server, ws://127.0.0.1:3536
target/release/buus-fury --online                          # the in-game lobby (F12 any time)
target/release/buus-fury --net-host --net-players 2        # or straight into a room: prints the code, e.g. KQZTA
target/release/buus-fury --net-join KQZTA --fighter vegeta --form 26
cargo test -p wf-net                                       # session logic, wf-core over a bad virtual network, rejoin, sounds
engine/crates/wf-net/scripts/two_native.sh 6000 100 0.05   # two headless clients over real WebRTC, 100 ms + 5 % loss
engine/crates/wf-net/scripts/internet_sim.sh all           # TURN relay-only, wss + tokens, crash + rejoin, shared pause
```

## 1. Choice: own rollback session + Matchbox (not GGRS)

The brief suggested GGRS + Matchbox. Matchbox is used as suggested (signaling protocol, `matchbox_signaling` server,
`matchbox_socket` WebRTC natively). The rollback session itself is a small GGPO-model implementation in `wf-net`
instead of GGRS, because GGRS does not fit the browser build:

* **The web shell is plain `extern "C"` wasm without wasm-bindgen** (`wf-web`, loaded with a hand-written import object
  by `app.js`). On `wasm32` GGRS requires `js-sys` (`js_sys::Date` in its protocol), `instant` with `wasm-bindgen` and
  `getrandom/js`, i.e. wasm-bindgen glue and its CLI in the build. Adopting it would mean converting the web shell
  (shared with other work) to wasm-bindgen, or shipping a second bindgen module and bridging the two in JS.
* **No clock and no I/O inside.** GGRS reads wall-clock time and owns a socket; wf-net's session takes `now` and packets
  as arguments, so the identical code runs natively, in the browser (packets moved by JS) and on a **virtual network**
  in tests (deterministic latency / jitter / loss, thousands of frames in milliseconds, exact reproduction).
* **wf-core already provides everything rollback needs** (inputs as `u16` per slot, `snapshot` = `World` clone
  ~1.5 µs, `state_hash` ~12 µs), so the session is ~700 lines: input queues with prediction, saved states, rollback,
  time sync, hash exchange, disconnects, spectators.
* **Interop**: native and browser speak one wire format (`proto.rs`) and one signaling / data-channel setup.

What GGRS would have added (its sync handshake, network stats API) is covered: the lobby replaces the handshake,
`NetStats` the stats. A `GgrsSession` adapter would be easy natively but would not interoperate with browsers.

## 2. Architecture

```
wf-core  Session (deterministic battle)  + PvP rule set (battle::set_pvp, player::on_message)
   │
wf-net   game.rs        Game trait: save / load / advance(inputs, first) / checksum
         core_game.rs   CoreGame (wf-core Session as a Game), MatchConfig {battle, pvp}
         session.rs     P2PSession<G>: rollback, prediction, time sync, hashes, disconnects, spectator feed,
                        seat epochs + rejoin grant / resume, catch-up, acting host
         spectator.rs   SpectatorSession<G>: confirmed inputs from the (acting) host, catch-up
         synctest.rs    SyncTestSession<G>: roll back N frames every frame, compare checksums (+ byte snapshots)
         proto.rs       wire format v2 (LE binary; lobby messages are JSON; blob chunks)
         lobby.rs       hello (ready, seat key) / start / full / rejoin / grant; room codes + tokens; tickets
         blob.rs        snapshot transfer over the unreliable channel (chunks + bitmap acks)
         ice.rs         STUN / TURN config, TURN REST credentials (HMAC-SHA1), relay-only SDP filter
         transport.rs   Transport trait; SimNet (virtual network), Conditioner (latency/jitter/loss), QueueTransport
         matchbox.rs    MatchboxTransport (native WebRTC, feature `webrtc`), ConnectOptions (ICE, CA, key)
         wsclient.rs    our Matchbox signaling client: wss:// (rustls + extra CA), relay-only filter, GET /ice
         netsim.rs      NetLobby, NetSim (wf_view::Sim for the frontends), SyncSim, NetStatus, desync report, replay
         cli.rs         --net-* options, blocking lobby, headless runner
         bin/wf-signal  Matchbox full-mesh signaling server: room tokens, key, /ice, /healthz, optional TLS (`server`)
         bin/wf-turn    TURN relay (webrtc-rs `turn`, REST credentials; feature `turn`)
wf-app     netmenu.rs: the in-game lobby menu (game font), shared by both shells; App::set_sim
wf-native  online.rs: menu ↔ non-blocking lobby ↔ NetSim swap; --net-* / --online; overlay; local slot's camera
wf-web     net.rs: wf_menu_* (lobby menu) + wf_net_* exports (lobby, ready / fighter / start, packet queues,
           status, reconnect, report, replay); wf_app_init {"net": true}
web/app    net.js: Matchbox signaling over WebSocket + RTCPeerConnection / RTCDataChannel (ICE from /ice, relay
           policy), the menu loop on the game canvas, reconnect, ticket in localStorage; overlay
```

### The session (per tick, `P2PSession::tick`)

1. Peers silent for `disconnect_ms` (5 s) are dropped.
2. If a received input differs from the one a frame was simulated with: load the saved state of that frame and
   re-simulate up to the present (re-simulated frames drop their sounds: `first = false`).
3. Hash newly **globally confirmed** states (every connected peer has all inputs before them; known from the status
   in every packet), extend the hash chain, compare with the peers' hashes.
4. Unless the prediction window is full (`frame - confirmed >= max_prediction`: **stall**) or time sync asks to wait
   (**skip**): record the local input for `frame + input_delay`, simulate the frame with known or predicted inputs
   (prediction = the slot's last known input), save the state.
5. Send each peer one packet: all local inputs it has not acknowledged (redundancy instead of retransmission),
   ack, frame + frame advantage (time sync), ping / pong (RTT), per-slot status, the last 4 confirmed hashes and the
   newest chain value. The host additionally sends spectators the confirmed inputs of all slots.

Defaults: input delay 2 frames, prediction window 12 frames (covers ~230 ms RTT without stalls), disconnect after
5 s, time sync skips at most one tick every 12. Inputs are the 10 GBA buttons (`u16`) per slot; packets are ~130
bytes at 60 Hz per peer.

### Desync detection

Each peer hashes every globally confirmed state with `World::state_hash` and chains the hashes
(`chain[k] = mix(chain[k-1], hash[k])`); packets carry the last 4 `(k, hash)` and `(k, chain)`. Equal chain values mean
every earlier frame matched too. On a mismatch: `NetEvent::Desync`, a red **DESYNC** line on the overlay (native:
drawn into the frame; browser: status box) and a **desync report**: JSON with the frame, both hashes, the match
config, the full input log, the last 600 local hashes and observations, plus a `.wfr` replay cut at the divergence
(`--net-report DIR` natively; "download report" in the browser). Re-simulating the report's inputs on both sides
(`netsim::replay_hashes`, or `buus-fury --headless --replay desync-….wfr`, which verifies checkpoints) gives the first
divergent frame.

### Disconnects

A dropped peer's slots get neutral input (0) from the frame after its last input. Peers that received different
amounts of it reconcile to the minimum through the status every packet carries (and roll back if needed); hashes are
only taken of states every peer confirmed, so a disconnect cannot raise a false desync. The match continues; the
overlay names who left.

### PvP rule set (wf-core)

`battle::set_pvp(world, true)` (`World::opts.pvp`, off by default): a player's attack message (`team == 1`: melee,
skills, its shots and effects, attacker = that player) is accepted by **other players** in `player::on_message`
through the same path as enemy attacks — box overlap, hittable / dead / eaten checks, blocking, `ComputeDamage`, HP
accumulator, flash, hurt / KO — and the attacker gets the `Hit` cue and its on-hit hooks (`on_dealt`), as when it
hits an enemy. A player never hits itself; enemy-thrown hits are not PvP. Outcome: last fighter standing (200 frames
after the last KO, like the loss), `battle::pvp_winner`. `MatchConfig::pvp(battle, false)` removes the enemies
(`pvp + enemies` keeps them: everyone fights everyone, enemies still target players per slot). With the flag off
nothing changes: `Options` hashes `pvp` only when set, so single-player / co-op state hashes are as before, and the
lockstep / cross-check suites are unaffected.

Co-op uses the core's existing multiplayer rules (each enemy targets a player slot, round-robin at spawn; the
projectile camera test is the union of all players' cameras).

### Lobby

Room URL `ws://<signal>/wf-<CODE>` (Matchbox full mesh: every pair of peers gets a data channel). Peers send `hello`
(role host / player / spectator, name, fighter + form + level, content fingerprint, protocol) every 250 ms. The host
seats players in arrival order (host = P1) and, when `--net-players` are present, sends `start`: match nonce, roster
(slot → peer id), spectators, the `MatchConfig` (the host's arena / enemies / seed / PvP, every player's own fighter
and form) and the net settings. Later `hello`s get the same `start` (lost packets, late spectators) or `full`. Peers
whose content fingerprint (enemies, characters, forms, skills, equipment, XP table, arena collision) differs from the
host's are refused (they would desync).

### Link play: rooms (the game's Multiplayer menu)

The game's link-cable Multiplayer (Dojo / Trading Post, `re/buu/menu/NOTES_screens.md`) runs over the same lobby
as **room kinds** (`wf_net::lobby`): `LobbySettings.kind` = `KIND_MATCH` ("match", the battle rooms above,
default), `KIND_DOJO` ("dojo") or `KIND_TRADE` ("trade"). Every `hello` and the `start` carry the `kind` and an
opaque per-kind JSON payload: `LobbySettings.extra` goes out in our hellos (`Lobby::set_extra` changes it),
`LobbySettings.start_extra` in the host's `start`, and the kind owner reads the others' payloads with
`Lobby::peer_extras()` (peer id -> extra) and `StartInfo.extra`. A host answers a hello of another kind with `full`
("this room is a dojo room") - "Waiting for a friend..." pairs only two players who chose the same thing.
"Disconnected" = `Lobby::peer_connected(id)` false / the transport's disconnect event. A kind that decides itself
when to begin sets `auto_start: false` and calls `Lobby::request_start()`. Old peers' hellos without the fields
parse as kind "match" (serde defaults), so the battle rooms are unchanged.

### Link play: Dojo (`wf_net::dojo`)

The game's Dojo ("Battle with your friends in the Dojo!") as a `dojo` room + one rollback match:

1. **Save pick.** Each peer picks one of its local saves; `DojoSave::from_save(slot, &SaveData, &GameData)` turns it
   into what the other side sees: the party as fighters (fighter id, form, level, STR / POW / END / SPD, equipment).
   A save with no playable party member is the game's "No Saved Games".
2. **The room.** Both open a `KIND_DOJO` lobby with `auto_start: false`; their hello's `extra` is a `DojoHello`
   (save summary, pick = "Choose your character!", setup). The host puts its `DojoSetup` ("Character Mode:"
   Balanced / In-Game, "Rounds:" 1 / 3 / 5) in its hello and in `start_extra`, and `LobbySettings.base =
   dojo::base_config(seed)` (PvP, the `dojo` arena = area 0-90, no enemies, no XP). The guest takes the host's
   setup: its fighter in that mode (`dojo::fighter_setup`) and the setup echoed in its hello, then ready - the
   confirm step. The host calls `request_start()` once the guest's echo matches ("Start Fight"); the `start`
   carries the match (both fighters) and the setup.
3. **The match.** `DojoGame` (a `Game`): per round a PvP battle `Session` in the Dojo; the generator starts at
   (0, 0) and carries on across rounds. "Round N" between rounds (`between_frames`); a round ends the frame a
   fighter's HP reaches 0 (one down: the other wins it, both down: drawn) or when the timer (`round_frames`) runs out
   with both standing: drawn. The match ends once a player has more than rounds / 2 wins; drawn rounds don't count,
   so the ROM has no round limit (`max_rounds`, 0 = none, exists for tests). The rounds, score and timer are part of
   the rollback state (and its checksum), so the whole match rolls back and is desync-checked like any battle.
4. **The end.** `DojoMatch::settle(&confirmed_game, &rules, &mut save)` with the game rebuilt from the
   **confirmed** inputs (every input of every frame known): the first time it shows the match over, the ending is
   decided - "You Win!!" + "Grab your prize!", "You Lose!!" + "Go back to training!", "Draw!!" - exactly once (later
   calls change nothing). The ROM's prize is only the banner; `DojoRules::prize` (default None) can add a supply to
   the winner's save, once. `DojoMatch::disconnected()` before that = "Disconnected": no result, no prize, the save
   untouched.

**Recorded from the real game (2026-10-10).** Two emulators wired by the link cable in one process (tools/emu shim
`emu_link` / `emu_link_run_frames`: mGBA's lockstep coordinator, both cores' run loops interleaved; headless, muted):
`tools/link/dojo_record.py NAME [--frames N] [--seed S] [--mode balanced|ingame] [--rounds 1|3|5] [--pick-a N]
[--pick-b N] [--switch]` boots both with run 2's saves, goes title -> Multiplayer -> Dojo -> the setup -> the picks,
and fights on seeded closed-loop keys (`--switch`: L presses too, the In-Game skills); per logic frame of the host
(written once complete, when the next has begun - a link stall leaves no half-updated row) it writes
`data/link/dojo_NAME.json`: the keys each fighter's update read (`0x03002DA4` / `0x03002DAE`, the remote side's
arrive 6 frames late, nothing is read while "Round N" shows), the generator, the round timer and wins, both fighters'
position / facing / animation / HP / EP / regeneration count / form / gauges / controller slot, the coins on the
floor, both sides' zenie, and the fighters' records as made (In-Game Mode). `wf-net/tests/dojo_link.rs` advances the
app's `DojoGame` with those keys from the fighters' creation and compares every frame to the match's result: 23
recordings, Balanced 1- and 3-round matches over all 24 characters (Super Saiyan forms included) and In-Game
3-round matches (Goku, Vegeta) with skills, transformations and skill switching - all exact (fighters, generator, timer, wins,
zenie). Fixed on the way: the round's start (generator 0, per fighter rand(255) then its idle tick rand(255), at
(102, 220) / (202, 220) facing down; 125 updates of "Round N", `DojoRules::start_frames`); a landed punch gives no
EP; the result 300 updates after a knockdown (`KO_FRAMES`: the timer, the fighters and the fallen one's regeneration
run on, no death script); between rounds 63 updates of the old fighters (`between_frames`), the new ones made in an
update of their own; a fighter as made has the plain form controller (slot 0x03001B70: no Super Saiyan gauge or
drain, EP from the record) and no form stat bonus, until a transformation (`Player::link_plain`); the round's result
cuts a transformation, a charging beam or a Big Bang in the hand (back to idle); Instant Transmission does nothing; the In-Game fighter keeps
its save's selected skill and skill levels, a Balanced record has level 50 (XP 0); the prize (below).

**The game's rules, decoded** (static disassembly of the link code). Fight function `0x0802193A`, state block `0x03002BFC` (+0 / +1 round wins, +4 the timer's
seconds), fight context `0x0872A6B4`.

| Rule | ROM | Status | Here |
|---|---|---|---|
| Round timer: 180 s, 60 updates per second | `movs #0xB4` at `0x08021C4A`; 0x3C update counter `0x08021C4E` / `0x08021D2A` | MEASURED | `round_frames` 10800 (was 99 s) |
| A round ends the update a fighter's HP (record +0x16) is 0: one down -> the other wins, both down -> Draw!! | `0x08021CE4`..`0x08021D1A` | MEASURED | `round_end` (was: 200 frames after the KO) |
| Timer out with both standing = Draw!! (no HP comparison) | `0x08021D48`..`0x08021D4E` | MEASURED | (was: higher HP share wins) |
| A knockdown decides the round 300 updates later; then 63 updates of the old fighters, the next round's made in an update of their own, 125 updates of "Round N" | `0x12C` at `0x08021DC4` | MEASURED (link recordings) | `KO_FRAMES`, `between_frames` 63, `start_frames` 125 |
| "Round N" = string 106 + both players' wins: a drawn round repeats its number | `0x08021A90`..`0x08021AA8` | MEASURED | `st.round` |
| Match: loop while a player's wins <= rounds / 2; draws don't count, no round limit, a match cannot end drawn | `0x08021B98`..`0x08021BAE` | MEASURED | `max_rounds` 0 (was: N rounds max, equal = Draw!!) |
| RNG words `0x03004A5C` / `0x03004A60` zeroed once when the fight starts, then running on through all rounds | `0x0802193E`..`0x0802194C` | MEASURED | `RngInit::State(0, 0)`, carried (was: seed + round) |
| Arena area 0-90, fighters at (102, 220) / (202, 220) | `movs #0x5A` at `0x08008E8C` | MEASURED | `dojo` arena |
| Prize: the match-deciding knockdown runs loot script s498 with the fallen fighter's level (from its XP, `0x080135D2`) at its footprint's centre: 1 + rand(100) -> 1..8 rolls, each chance(85) or a coin roll before -> a coin (s500), else a level-band supply (s499) - in effect only the first roll can be a supply. Coins are collected by touch from their age 60, a supply with A (61 updates' hold: the clocks and the coins' age stopped, their flight going on); the zenie goes into the RAM game data, no save write (the only one of the link code is the trade's) | `0x0801088A`, scripts 498..500, `0x080231AE` | MEASURED (link recordings) | `player::link_prize`, `fieldobj::link_interact`; `prize` None |
| Character Mode byte: 1 = Balanced, 0 = In-Game | `0x0802195A`, `0x0802311A` | MEASURED | `DojoMode` |
| Balanced roster: a fixed 32 (strings 116..147: Goku, SS Goku, SS3 Goku, Teen Gohan, SS Teen Gohan, Mystic Gohan, Great Saiyaman, Gohan, SS Gohan, Trunks, SS Trunks, Vegeta, SS Vegeta, Goten, SS Goten, Gotenks, SS Gotenks, SS3 Gotenks, Vegito, SS Vegito, SS Gogeta, Hercule, Videl, Pikkon, Piccolo, Broly, Majin Buu, Super Buu, Super Buu 2, Super Buu 3, Kid Buu, Uub), the same for both players; every Balanced fighter's record has the plain form 45 and takes its sprite from the table (the last 8 have no form of their own: `FighterSetup::sprite`; Broly and Majin Buu .. Super Buu 3 are END 30 / SPD 45) | list builder `0x08023120` | MEASURED | `BALANCED_ROSTER` / `balanced_roster()` (was: the save's party) |
| Balanced record: HP 15000, EP 100, STR 67, POW 67, END 25, SPD 50 (Hercule STR 60, POW 74); one skill, 7 Energy Blast; XP 0 (level 50) | table `0x0872BFCC` (28 bytes: sprite, HP, EP, STR, POW, END, SPD), template `0x08022904`, applied by `0x0801339C` | MEASURED | `FighterSetup::fixed` (was: level 100, average growth, even points) |
| In-Game: the save's party (12-byte list at page +0x584, names string 0x9E + id), each character's whole record packed and unpacked (level, form, skills + levels, max HP / EP as current, STR..SPD, equipment) | pack `0x08022934`, unpack `0x080229B0` | MEASURED | `fixed` from the save's record (was: level + approximated points) |
| The host's mode / rounds apply to both | config over the link (`0x03002FB0`) not fully traced | INFERRED | host's `DojoSetup` |
| Which side is the host (left / right) | - | INFERRED | host = slot 0, left |

wf-core's `FighterSetup::fixed` (`FixedRecord`: HP, EP, STR..SPD, skills) replaces what level / growth / points give,
mirrored in the TS battle reference (`site/src/battle/rulesets/buu/index.ts`, `options.fixed`); without it nothing
changes.

**Tests** (`cargo test -p wf-net --test dojo`, headless, muted, two in-process peers on the virtual network):
`dojo_match_over_a_bad_network` - the room (a battle-room peer is refused: "this room is a dojo room"; both peers
build the same match; Balanced / In-Game levels), then the match over loopback, 80 ms / 120 ms latency + jitter and
5 % loss in 1, 3 and 5 rounds, random and scripted fighters: equal final state hashes, no desync, the same result
on both, the winner's prize exactly once (settle called three times + a late drop), nothing for the loser / a draw;
`dojo_disconnect_mid_match` - a peer drops mid-round: both end "Disconnected", no prize, saves untouched;
`dojo_synctest` - 4000 frames of 5-round matches with an 8-frame rollback every frame (Balanced, In-Game);
`dojo_rom_rules` - the decoded defaults, the 32 Balanced records, In-Game = the save's record. (The match tests
shorten rounds, lower Balanced HP, cap rounds at 5 and set a prize, to stay quick and check the prize-once path.)
Playable in the app: "Link play: playing it" below (the screens and the match on one connection).

### Link play: Trading Post (`wf_net::trade`, `wf_app::trade`)

The game's link trade ("Your Trade:" / "Friend's Trade:" / "Zenie" / "How many to trade?" / "Propose Trade" /
"Accept" / "Decline" / "You have accepted trade" / "TRADE COMPLETE!", strings 80..89) between two players' save
slots. Pairing: a `KIND_TRADE` room (above); once the two are connected the trade runs between their peer ids over
the same transport (its packets start with `TRD1`; no rollback, no realtime need). Each side writes only its own
save.

**Flow.** Each side picks a save slot (its inventory is what can be offered) and builds an [`Offer`]: supplies by
index with a count, equipment copies, zenie. Every 200 ms each side sends one idempotent state snapshot (`Snap`):
its offer and the offer's revision, its acceptance, and the state of every trade record it holds with this friend.
The friend's offer shows as "Friend's Trade:". "Accept" (`TradePeer::accept`) names both offers' revisions; changing
an offer (`set_offer`) bumps its revision and voids both acceptances; "Decline" withdraws. When the two acceptances
match, both sides derive the same **trade id** (SHA-1 of both peer ids, session nonces, revisions and offers) and
the commit starts. Loss, delay, duplicates and reordering only delay this (every message is state, sent again).

**Commit (two-phase, durable: [`TradeStore`]).**
1. *Prepare*: each side checks its side of the trade (the offered items are in its save, unworn, no story item;
   receiving does not pass a limit) and records the trade `Prepared` - nothing applied, the save locked for other
   trades. A check that fails records `Aborted` (with the reason) instead.
2. *Commit*: a side that is `Prepared` and sees the friend `Prepared`, `Committed` or `Done` applies the trade to its
   save **atomically with** marking it `Committed` ("TRADE COMPLETE!").
3. *Done*: a `Committed` side that sees the friend `Committed` / `Done` marks it `Done` (kept, so a late question is
   still answered "committed").

**Recovery rule.** A side records `Aborted` only for a trade it never prepared: its check failed, or the friend
reports `Prepared` for an id it does not know (a restart lost the negotiation) - it records `Aborted` *before*
answering, so it can never prepare that id later. A `Prepared` side never decides alone: it commits when the friend
reports `Prepared` / `Committed` / `Done` and aborts only when the friend reports `Aborted`. Hence both sides end
committed or both aborted, nothing is applied twice (the record's state is part of the same atomic write as the
change), and nothing is lost. The price is the classic 2PC one: a side that is `Prepared` while the friend is away
(crash, dropped connection) stays **in doubt** - the UI says "Disconnected", the slot stays locked, the trade
finishes when the friend comes back (the journal survives restarts; `TradePeer::new` resumes it). A negotiation
that never reached Prepared is just memory and ends on a disconnect / restart.

**On disk** (`wf_app::trade::FileStore`): the slot file (`saves/slotN.sav`, `Saves::slot_bytes`) and a journal
next to it (`slotN.sav.trade.json`), each written temp + rename. The commit is a write-ahead log over the two
files: (1) the journal stores the slot's post-trade bytes, (2) the slot file is replaced, (3) the journal marks the
trade Committed and drops the log. `FileStore::open` finishes a commit a crash interrupted (the slot ends with the
post-trade bytes, once).

**The game's rules** (static disassembly, and trades recorded between two linked emulators:
`tools/link/trade_record.py NAME --a PICKS --b PICKS` -> `data/link/trade_*.json`, each side's offer as the other's
"Friend's Trade:" holds it (EWRAM `0x0203CDA4`) and game 1's supplies / equipment list / zenie / equipment slots
before and after; `cargo test -p wf-net --test trade_link` applies `Inventory::after` to the before and compares -
4 trades, both sides exact):
* MEASURED from the ROM: the trade's apply step (`0x080231AE`, the receiving side, on the selected save's GameData,
  then the slot writer `0x080049F2`) adds the friend's 83 supply counts to the save's, **saturating at 255** (one
  byte each), adds the zenie, and appends each equipment entry with `0x08004448` - which **ignores an add at 250
  entries**; equipment entries are copied without the worn marks (high nibble = wearer + 1). Remove equipment
  (`0x08004468`) takes the first unworn copy, else the first worn one and sets its wearer's slot back to "-None-"
  (MEASURED: run 2's Gohan wears the save's only Cotton Gi and Leather Moccasins - both traded, his slots emptied).
  Here: a trade whose result would pass 255 of a supply or 250 equipment entries is refused at Accept / Prepare
  instead of losing the excess; worn equipment is offered and removed as the game does (`wf_app::trade::apply`).
* MEASURED from the ROM: the item picker (`0x0801F44E`) lists a supply only when bit 0 of its record
  (`0x083BD2F8` + 0x34 x id) is clear - the items table's `story_item` - so supplies 0..24 are tradable and the story
  items 25..82 are not; every equipment-list entry is listed, worn ones too; zenie only when not 0 ("How many to
  trade?": RIGHT raises the count). Same rule here (`Inventory::story`).
* INFERRED: no zenie limit beyond u32 (none found in the apply step).
* The Trading Post screens over `TradePeer` + `FileStore`: "Link play: playing it" below.

**Tests** (headless, in-process): `cargo test -p wf-net --test trade` - two peers on the virtual network:
loopback, 120 ms + 80 ms jitter + 30 % loss (8 seeds), over-limit refusal, decline / changed offer (nothing
applied), a peer gone while prepared (in doubt, settles when it returns), and **fault injection at every step**:
each side killed before / after each packet it receives (restarted from its journal 500 ms later, new session) and
the connection dropped for 3 s, on a clean and a lossy network (300 cases; the faults land before any record, with
the trade Prepared, Committed and Done) - every case: totals of every item and of zenie conserved, at most one trade
applied, both sides agree, nothing left in doubt. `cargo test -p wf-app --test trade` - the same on real slot files
with a crash after each WAL write on either side (the store re-opened from disk), the worn item kept.

**The "half-finished" stall (fixed).** Two-process runs (1 in 14) and a randomized two-app sweep (1 in 60 sessions,
at 88 ms latency and 31 % loss) ended with one side's trade Done and the other's Committed - nothing lost (the
Committed side turns Done at the next link from the journal), but the session should finish it. Cause: the TRADE
COMPLETE screen let a player leave as soon as **its own** record was Done. A side turns Done when it sees the
friend's Committed; the friend turns Done only when it sees this side's Committed / Done in a later snapshot. When
the side that finished first left (its room closing with it) before one of those snapshots arrived - a loss at the
wrong moment - the friend heard "Disconnected" while still Committed. Fix: leaving waits for **both** sides' Done -
`TradePeer::settled(id)` = my record Done and the friend's reported state Done (`friend_state`, what this session
heard); my Done snapshots keep going out every 200 ms until the friend's Done comes back. The commit rules are
unchanged (still all-or-nothing; a side still never aborts a prepared trade on its own), so a real disconnect or the
10 s limit can still leave one side Committed - resolved at the next link as before. Regression test:
`cargo test -p wf-app --test link trade_stress_both_reach_done` (12 seeds; `TRADE_STRESS=240` for the full sweep):
per seed a random latency 0..250 ms, jitter 0..120 ms, loss 0..35 %, both players leaving TRADE COMPLETE as early as
allowed; asserts both journals Done and the totals conserved. Before the fix 1 of 60 failed; after: 0 of 240, and
20 of 20 real two-process runs (`TARGET=... tools/link/repeat.sh 20 trade`).

### Link play: playing it (`wf_app::link`, native `--link`)

**Try it** (two copies on one machine, the local signalling server on `ws://127.0.0.1:3536` as for battles):
```
buus-fury --link host:ABC12            # player 1 (its own --data folder / save slots)
buus-fury --link join:ABC12            # player 2
```
Both open on the game's title. START -> Multiplayer -> **Dojo** or **Trading Post**: the game's "Waiting for a
friend..." box opens and *is* the lobby - it joins room `ABC12-DOJO` / `ABC12-TRADE` on `--signal` (the `--net-*`
network options, `--net-name`, `--net-key` apply); B (Cancel) leaves as in the game. Once the friend is in the room:
* **Select Saved Game** (the slots; "No Saved Games" when none can be used) ->
* **Dojo**: the setup page: the host changes "Character Mode:" Balanced / In-Game and "Rounds:" 1 / 3 / 5
  (left / right), the guest sees them live; **Start Fight** -> "Choose your character!" (Balanced: the game's
  roster of 32, strings 116..147 - Piccolo, Broly, the Buus and Uub too, recordings dojo_r1 .. r4; In-Game: the save's party at its level); the guest's pick confirms the host's
  setup (a changed setup sends it back to the setup page) -> the match: `DojoGame` in a rollback
  `P2PSession` on the same connection (lobby packets still handled). The fight's picture is the game's own for this
  side (`DojoGame::view(slot)`: its camera, its HUD and the friend's bar, "Round N", the timer, "You Win!!" /
  "You Lose!!" / "Draw!!", "Grab your prize!" / "Go back to training!", the fades), gated picture for picture
  against two linked emulators (wf-app `tests/dojo_pixels.rs`: recordings `data/link/dojo_px1` / `dojo_px3` with a
  `video` list, 16,150 pictures over both sides - every one exact or torn as the tear rule allows, core.md). The
  result and any prize (`DojoRules.prize`, None = the ROM: no item) come from the **confirmed** inputs only
  (`DojoMatch::settle`, once), into the save's slot file and the running title's slot. After the fight's fade-out
  (64 updates from the result) both are back on "Choose your character!" with the same setup, as in the game
  (recording dojo_px3): the lobby is open again (`Lobby::reopen`: hellos resume, a new session id), a pick counts
  for the match it was made for (`DojoHello::played`), and the next fight starts from the two picks; B leaves by the
  pages (tests/link.rs `dojo_rematch_from_the_character_page`).
* **The friend gone** (its connection closed or lost - on a page, in a fight, on the trade's pages with nothing at
  stake): the game's link error, as two linked emulators show it when one is reset (`re/buu/menu/linkcap.py`
  `reset_b`; 180 updates after the reset on a page, 63 in a fight - ours: when the connection drops): the link mode
  ends and the engine's hook script **s001** runs as a scene over what the screen shows - `hide_hud`, `fade_out(16)`
  (the page / the fight's last picture fades), `load_scene(0, 2)`, `fade_in(16)`, the box "You have been
  disconnected.", A, `fade_out(16)`, `return_to_title`: the title's first page (START / OPTIONS), not the
  Multiplayer page. `wf_core::menu::link::lost(world)` on the title app does it (`World::under`: the page's picture
  under the script's fade); `LinkSim` keeps the fight's last picture there instead when the link was lost in one
  (phase "lost"). Gated frame by frame, the scene's frames and the title after it included: menu_parity
  `link_lost_setup` / `link_lost_saves` / `link_lost_chars`. A trade in doubt keeps its page (the journal's lock).
* **Leaving with B** (Select Saved Game's Cancel): the page becomes the title's Multiplayer page (menu_parity
  `link_saves_back`), and the friend's page follows by itself whatever it shows - no link error (two linked
  emulators: `link_friend_left` / `link_friend_left_setup`; the game's side follows 9 to 60 iterations later, ours
  when it hears of it). The leaving side says so in its hello (`LinkHello::left`) and keeps its connection for 45
  more steps (`LinkSim::leaving`); the friend's flow gives `menu::link::friend_left` to its page and ends on the
  Multiplayer page too (tests/link.rs `dojo_friend_leaves_with_b`).
* **No saved game at all**: the game's Select Saved Game with the box "No Saved Games" / B Cancel over it (the wait
  box's state 2; menu_parity `link_nosaves`, both emulators with erased saves), B leaves as above.
  Left as the app's own drawing: the lobby's refusals (a full room, another version) and a missing save folder.
* **Trading Post**: the game's own pages (wf-core `menu::link::trade`, gated frame by frame against two linked
  emulators: menu_parity `link_trade_*`): the picker ("Your Trade:", the list with its arrows, "How many to
  trade?"), R **Propose Trade** -> "Waiting for a friend..." until the friend's proposal -> the proposal ("Your
  Trade:" / "Friend's Trade:", ACCEPT / DECLINE) -> "Waiting for a friend..." until its answer -> both ACCEPT:
  "TRADE COMPLETE!" and back to the Multiplayer page by itself; a DECLINE on either side brings both back to the
  picker. Under the pages (`wf_app::link`: the page's actions in, the friend's proposal / answer out) the
  two-phase commit on `saves/slotN.sav` (`FileStore`): a proposal is `TradePeer::propose` (`Snap.proposed`: an
  offer still being put together is not one), ACCEPT `accept`, DECLINE `decline`, a withdrawn proposal / answer
  `withdraw` / `unaccept`; the running title's slot is updated (the shell persists it). The trade names peers by a
  stable **link id** (`<data>/link_id`), so a reconnection with a new transport id resumes a pending trade (the
  friend's connection is the one with its link id that is *talking*). **TRADE COMPLETE! keeps answering**: after
  the page went back to the Multiplayer page the trade's connection stays up until the record is Done on both
  sides, the friend is gone, or 10 s passed (a side that left at once could leave its friend in doubt until the
  next link: found by the two-process test below).
* A drop at any point after the friend came: "**Disconnected**" (93); in a match: the ending is Disconnected, no
  prize; in a trade before both accepted: nothing applied; with a trade Prepared: "Disconnected - finishing when
  your friend is back" (the slot stays locked; the journal finishes it at the next Trading Post link with that
  friend, as above).

Any key on the result / TRADE COMPLETE / Disconnected page closes the link box with its own B: the Multiplayer page
is back. Single-player is unchanged (`LinkSim` only acts once the title's link box opens; without `--link` the box
stays the game's dead end). While a link flow runs the app reports no state hash / snapshot (replays and rewind
skip it); the match is checked by its session's per-frame hashes.

**Tests** (`cargo test -p wf-app --test link`, headless, muted, needs the ROM): two title apps in-process on the
virtual network (80 ms + 20 ms jitter + 5 % loss), driven only by scripted closed-loop pad input through the real
title (START, Multiplayer, Dojo / Trading Post) and the link screens: a Dojo match (both peers' confirmed match:
same end frame, checksum and inputs; complementary endings; the prize once in the winner's slot file and running
slot); a guest drop mid-match (Disconnected, no prize); a trade (TRADE COMPLETE on both, supplies and zenie conserved
across both slot files, the running titles updated); a guest leaving mid-trade (nothing applied); **a crash
mid-commit through the screens** (`trade_crash_between_prepare_and_commit` / `_commit_and_done`: both accepted, the
guest's app and network die when its journal reaches Prepared - it accepted last, so it prepares before the host
hears it - or Committed; the host goes on alone and shows the trade in doubt or TRADE COMPLETE; the guest's app
restarts from its folder through the title's own startup (`title_sim` on the slot read from disk, its link id),
goes back into the Trading Post, its screen shows the locked trade ("Disconnected - finishing when your friend is
back") and then TRADE COMPLETE (Prepared) / marks it Done (Committed): one trade id on both sides, both slot files
as after exactly one trade, supplies / zenie conserved, nothing applied again later); a trade list longer than five
rows (`trade_list_scrolls_with_arrows`: the prompts at the top / bottom, drawn). The scripted player is the
library's `wf_app::link::LinkBot` (on `LinkConfig::bot`, played inside the `LinkSim`).

**Two processes** (`tools/link/twoproc.sh`, headless, muted, real time, nothing public): builds `buus-fury`,
`wf-signal` and the `link_save` helper, starts `wf-signal 127.0.0.1:3591`, writes each player's save slot into its
own `--data` folder (`cargo run -p wf-app --example link_save -- write CONTENT OUT.sav PARTY SUPPLIES ZENIE`), and runs
two separate `buus-fury --headless --mute --realtime --link host|join:ROOM --link-bot dojo|trade --link-quick
--signal ws://127.0.0.1:3591` processes: `--link-bot` plays the pages itself (the same `LinkBot`), `--link-report
FILE` (default `<data>/link_report.json` with a bot) writes each flow's end, `--link-quick` the Dojo's short test
rules. Checks: the Dojo - both processes confirm the same match (end frame, checksum, a hash of all confirmed
inputs) with mirrored endings; the trade - both report TRADE COMPLETE and the items + zenie are conserved across the
two slot files on disk (`link_save read`). `ONLY=dojo|trade`, `WAIT` (s), `--no-build`; `WF_LINK_DEBUG=1` logs the
trade's traffic and journal states per frame. Results on this machine: the first run ended with the guest Prepared
and the host Committed - the host had left TRADE COMPLETE at once (fixed: the page holds, above); of the 14 trade
runs after that fix (3 with the Dojo too, which passed every time), 13 PASS and **one ended with the host Prepared
and the guest Committed** (the guest left TRADE COMPLETE by the 10 s limit; the host had stopped hearing it - cause not found:
that run had no traffic log; with `WF_LINK_DEBUG=1` the next 13 runs passed). The trade stays safe either way (the
in-doubt side finishes at the next link with that friend), but a stall there is open.
**Gaps:** the link screens' layouts are INFERRED (the game's strings / fonts / window / cursor; no recording of
them exists).

### Link play: in the browser (`crates/wf-web/src/link.rs`, `engine/web/app/link.js`)

**Play it**: open the web app at `/app/?link=host:ABC12` on one machine and `/app/?link=join:ABC12` on the other
(`&signal=wss://HOST` for the signalling server, default `ws://127.0.0.1:3536`; `&key=` its key). Each starts on the
title with link play - the same `wf_app::link` flow and screens as the native `--link` (START -> Multiplayer ->
Dojo / Trading Post, the link box is the lobby of room `ABC12-DOJO` / `ABC12-TRADE`). The browser's story save slots
(`localStorage` `wf.save.N`) are the player's saves; native and browser players meet in one room (the same
signalling and data channel setup).
* **Connection**: when the title's link box opens, the flow asks for its room: Rust queues the URL (`wf_link_poll`
  returns 1 with it) and hands the flow a queue transport; link.js opens the Matchbox signalling + WebRTC data
  channel (as net.js does for battles) and feeds that queue through the `wf_net_*` exports (they serve the link's
  queue while no online battle is prepared). When a flow ends its transport is dropped and `wf_link_poll` returns 2:
  link.js closes the WebSocket and peer connections, so the friend sees the link go (as with the native socket).
* **Saves and the trade journal - atomic as on native.** The Trading Post's `FileStore` writes through a pluggable
  `wf_app::trade::Disk` (native: the file system, temp file + rename; browser: `WebDisk`). The browser's disk keeps
  the files in memory and queues every write; each frame link.js takes the queue in order (`wf_link_disk_next`) and
  mirrors it to `localStorage` (`wf.link.f.<path>`, hex) **before** it sends the frame's outgoing packets
  (`wf_net_out`). `localStorage.setItem` is synchronous and atomic per key, so every journal / slot write is durable
  before any message that depends on it leaves the page - the write-ahead log (journal with the post-trade bytes ->
  slot -> journal Committed) keeps the native store's guarantees; a reload re-reads the mirrored files (`cfg.link.disk`)
  and `FileStore::open` finishes an interrupted commit. The committed slot also goes to the running title
  (`Out::Slot`), which app.js persists as the story slot `wf.save.N` like any save. The browser's stable link id is
  `localStorage` `wf.link.id`.
* **Test hooks** (URL): `&bot=dojo|trade` (the scripted `LinkBot`, as `--link-bot`), `&quick` (the Dojo's short test
  rules, as `--link-quick`), `&linkinfo` (`window.wfLinkInfo()`: phase, title page, setup, trade cursor / state, the
  flow's end and its report - `wf_link_info`).

**Two headless Chromium pages** (`cd engine/web/tests && npm install && node link_check.mjs [--build] [--only
dojo|trade] [--verbose]`): two separate headless, muted browser processes with their own profiles (each extracts the
ROM once: `?devrom`), a local `wf-signal` on `127.0.0.1:3593` (PORT), the game served from 127.0.0.1 - nothing public.
Each page gets a save slot written by `link_save` into its `localStorage`, then opens `?link=host|join:ROOM&bot=...
&quick&linkinfo`; the test reads `wfLinkInfo()` until both flows end. Checks: the Dojo - both pages confirm the same
match (end frame, checksum, hash of all confirmed inputs) with mirrored endings; the Trading Post - both report TRADE
COMPLETE, supplies + zenie conserved across the two saves (the trade store's slot mirrored in `localStorage`, read
with `link_save read`), the journal Done on both, and the title slot `wf.save.0` equal to the traded slot. Result on
this machine: **all checks pass** (Dojo: 111 confirmed frames, the same checksum and inputs on both pages, Won / Lost;
trade: host 5 x supply 0 + 500 zenie, guest 4 x supply 1 + 100 zenie -> 2 + 30 each way, totals conserved, both
journals Done). A browser page against a native process in one room, and native == wasm on the same inputs: next
section.

### Link play: browser <-> native (`engine/web/tests/link_mixed.mjs`)

A native app and a browser page meet in one room as two natives or two pages do (the same signalling, data channel,
lobby payloads and wire format). `cd engine/web/tests && node link_mixed.mjs [--build] [--only dojo|trade] [--roles
nh|bh]` (headless, muted, 127.0.0.1 only): a local `wf-signal` (PORT, default 3595), one `buus-fury --headless --mute
--realtime --link host|join:ROOM --link-bot ... --link-quick` process with its own `--data` folder and one headless
Chromium page `/app/?link=join|host:ROOM&bot=...&quick&linkinfo`, in **both role assignments** (native hosts / the
browser hosts). Native binaries from `CARGO_TARGET_DIR` (default `engine/target-linkmix`, built by the script);
`--build` builds the web app into the same target dir. Checks per role assignment:
* **Dojo**: the same confirmed match on both (end frame, checksum, hash of all confirmed inputs), mirrored endings,
  and **native == wasm per frame**: (1) each side's confirmed game's checksum after every frame (the link report's
  `match.replay.hashes`, native `--link-report` / the page's `wfLinkInfo().report`) - identical lists; (2) **each
  side's recording replayed on the other build**: the page's recording through the native example `cargo run -p
  wf-app --example dojo_replay -- <content> REPORT.json` (exit 0 = identical every frame) and the native recording
  through the page's `window.wfDojoReplay(replay)` (the wasm export `wf_dojo_replay`). Both call
  `wf_app::link::dojo_replay`: the `DojoGame` rebuilt from the report's `match.replay.def` (match config, setup,
  rules), advanced with the recorded confirmed inputs, every frame's checksum compared.
* **Trading Post**: TRADE COMPLETE on both; supplies + zenie conserved across the native slot file and the browser's
  saved slot (the trade store's file mirrored in `localStorage`); items changed hands.

**Result on this machine: all checks pass** (both role assignments: Dojo matches of 108 / 110 confirmed frames,
per-frame checksums identical, both cross-build replays identical on every frame; trades complete and conserved).
**Fixed on the way - the Dojo checksum was platform dependent:** `DojoGame::checksum` hashed the match state with
`DefaultHasher`, which writes a `Vec`'s length as a native `usize` (4 bytes on wasm32, 8 natively): a native and a
browser peer saw the same inputs and states but different checksums (two natives or two pages agree, so the earlier
tests could not see it). It now uses `wf_core::world::StableHasher` (usize as u64, little-endian), as the world's
state hash does. Note for replays: `dojo_replay` loads `<content>/base` when the folder has one (as the app does) -
the test-extracted folder also holds an older flat copy of the base pack, which replayed differently from the first
fight frame on.

### Replays

Every match is a [wf-replay](replays.md) recording: `NetSim::replay()` builds a `.wfr` (sim `wf-core/battle`, the
battle config + `"pvp": true` when PvP — the app's replay player applies it —, every slot's confirmed inputs,
checkpoints from the match's own confirmed hashes every 60 frames). Native peers save it to `<data>/replays/`
(`--net-record DIR`) every 30 s and when the match ends, so it shows in the F10 replay list; the browser offers
"save replay" in the status box (`wf_net_replay`). Spectators can save one too.

## 3. How to play

1. Someone runs the signaling server where everyone can reach it: on a LAN `wf-signal 0.0.0.0:3536`; across the
   internet the deploy recipe (`engine/deploy/netplay/`: wss:// via Caddy + TURN), or any machine with a public
   address. Game traffic is peer-to-peer; the server only relays the WebRTC handshake.
2. **In-game lobby** (both shells, game font): `buus-fury --online` or **F12** in game; browser `/app/?net`.
   *Host a room*: mode (co-op / PvP / PvP + enemies), players 2–4, enemy, *Menu pauses* (only me / everyone), input
   delay, *Create room*: the room screen shows the invite code (`KQZTA` or `KQZTA-7F3K2Q` with a token) and everyone
   in the room with fighter / form / ready. *Join a room* / *Watch a room*: type the code. Fighter and form can be
   changed in the room; players toggle *Ready*; the host's *Start match* starts once every player is ready.
   *Rejoin last match* appears while a rejoin ticket exists. Choices persist (`<data>/online.json`, localStorage).
3. Command line (scripts, headless): host `buus-fury --net-host --signal wss://HOST [--net-players 2..4] [--pvp |
   --pvp --pvp-enemies] [--net-pause shared] [--arena wasteland --enemy t60 --count 1 --seed 7] [--fighter goku
   --form 2]` (starts by itself when the players are in); others `--net-join CODE[-TOKEN] --signal …`; spectators
   `--net-spectate CODE` (any time, also mid-match: catches up at 8 frames per tick); `--net-rejoin` (ticket).
   Browser URL presets skip the menu: `?net=host|join|spectate|rejoin|synctest&room=CODE&signal=…`.
4. In a match **Esc** (native: the online menu, *Back to the match* / *Leave the match*; browser: the settings panel)
   does **not** stop the match: the local player stands still, or, with the room rule *everyone*, holds the shared
   pause (the world stands still for all; overlay `PAUSED BY P2`).
5. Options: `--net-delay N`, `--net-prediction N`, `--net-name`, `--net-ice auto|none|stun:…|turn:…`
   `--net-turn-user U --net-turn-pass P`, `--net-relay-only`, `--net-key KEY` (`WF_SIGNAL_KEY`), `--net-token T|none`,
   `--net-ca FILE` (`WF_NET_CA`), `--net-sfx reconcile|confirmed|predicted`, `--net-report DIR`, `--net-record DIR`,
   `--net-ticket FILE`; test-only `--net-latency MS --net-jitter MS --net-loss P --net-seed N` (applied to everything
   this peer sends), `--net-random-input SEED`, `--net-result FILE`, `--net-pause-at FRAME:N`, `--net-exit-at FRAME`.
6. Local sync test: `buus-fury --net-synctest 8 [--net-synctest-bytes] [--players N]`; browser
   `?net=synctest&synctest=8`.

Ports: signaling 3536/tcp (ws) or 443 behind TLS. Peer traffic: WebRTC (ICE / DTLS / SCTP over UDP, ephemeral
ports); TURN 3478/udp+tcp + its relay range.

Browser URL parameters: `?net[=preset]&room=CODE[-TOKEN]&signal=wss://…&key=K&players=2&pvp&pvp_enemies
&fighter=vegeta&form=26&enemy=t60&arena=wasteland&count=1&seed=1&delay=2&pause=shared&sfx=reconcile
&ice=auto|none|url,url|[json]&relay&turn_user=U&turn_pass=P&latency=100&loss=0.05`.

## 4. Results

Virtual network (`cargo test -p wf-net`, wf-core battles, random button mashing on every slot, peers started 5
ticks apart, 0 desyncs everywhere, hashes / chains equal on all peers **and equal to a straight replay of the
confirmed inputs**):

| match | net | frames | rollbacks | max depth | µs / rollback (avg / max) | µs / re-simulated frame |
|---|---|---|---|---|---|---|
| 2p vs Kid Buu | 100 ms one-way, 5 % loss | 3000 | 255 | 9 | 19 / 84 | 4.8 |
| 4p vs Janemba | 100 ms + 20 jitter, 5 % | 3000 | 1417 | 10 | 36 / 210 | 6.9 |
| 3p vs 6 Majin Fighters | 60 ms + 30 jitter, 10 % | 3000 | 725 | 8 | 21 / 198 | 6.6 |
| 2p vs Super Buu + helper | 100 ms, 5 % | 2400 | 199 | 9 | 18 / 92 | 4.3 |
| 4p vs Yakon | 150 ms, 5 % | 2400 | 1144 | 12 | 35 / 196 | 4.9 |
| 2p PvP | 100 ms, 5 % | 3000 | 252 | 9 | 16 / 85 | 4.0 |
| 4p PvP + enemies | 80 ms + 20 jitter, 5 % | 3000 | 1446 | 8 | 24 / 135 | 6.2 |

(µs = test profile, opt-level 2; per peer, summed over peers.) Toy-game tests cover 2–4 peers up to 150 ms / 20 %
loss, start skew, a 3 % fast clock (time sync: 122 skips, no stalls), a peer leaving mid-match, desync detection
(caught within 1–19 frames of the first differing state), two spectators (one joining 15 s late) and the sync test
catching state outside the snapshot.

Real WebRTC, two headless native clients through `wf-signal` on localhost (`scripts/two_native.sh 6000 100 0.05`:
100 ms added on every packet each way → RTT ~233 ms, 5 % loss each way, random inputs, release build): 6000 frames
(103 s), **same state hash and chain at state 6000**, 0 desyncs, 242 / 251 rollbacks (max depth 10 / 7), **55 µs per
rollback** on average (max 0.8 ms), ~11 µs per re-simulated frame, 20 µs per normal frame (incl. sounds), 1 stall,
6 time-sync skips, ~135 bytes per packet.

PvP over real WebRTC (`two_native.sh 1500 50 0.05 --pvp --arena htc`): same hash at 1500, the match ended with a KO
(`winner: P2`) on both peers; the saved `.wfr` verifies with `buus-fury --headless --replay …` (OK, 29 checkpoints).

Native ↔ browser (headless Chrome via chrome-devtools-axi, `--mute-audio`, everything on 127.0.0.1, Chrome's default
mDNS host candidates — webrtc-rs resolved them, no extra flags needed):

* native host (random inputs, 100 ms + 5 % added) ↔ browser joiner (100 ms + 5 % added in the wasm): 3000 frames,
  RTT ~228 ms, 0 desyncs with ~11 900 hash comparisons on each side (native and wasm hash every confirmed state
  identically), browser 134 rollbacks (max 7), native 0 (the browser's input was constant: nothing to mispredict).
* browser host (PvP, scripted keys) ↔ native joiner (random inputs, 50 ms + 5 %): 2400 frames, 0 desyncs, ~9 000
  checks each, browser 99 rollbacks (max 6), native 8; browser offered, native answered (the other handshake direction).
* browser `?net=synctest&synctest=8`: runs, no mismatch.

Sync test: all 712 emulator lockstep scenarios (`tests/synctest_lockstep.rs`, 298 236 frames, 2.35 M re-simulated,
rollback of 8 every frame, every third scenario through the serialized snapshot) and multiplayer battles (4p co-op ×8
enemies, 3p Janemba, 2p Broly, PvP 2p / 4p + enemies, 1p Kid Buu; 1500 frames each, half through byte snapshots): no
mismatch. wf-core's lockstep (616 of the reference pass set), cross-check and session suites are unchanged by the PvP
rule set.

## 5. Online pause, sounds, rejoin, PvP homing

### Online pause

A rollback match cannot wait for one player's menu. `wf_view::Sim::paused_input` (new, default `None`) lets the app
keep stepping an online `NetSim` while `App::paused` is set, with the local player's input replaced:

* room rule **local** (default): neutral input. The player stands still, the match goes on, nobody stalls or drops.
* room rule **shared** (`--net-pause shared`, the host's *Menu pauses: everyone*): the input carries
  `core_game::PAUSE_BIT` (bit 15, not a GBA button). A frame where any slot holds it is a no-op for the world on every
  peer, deterministically, since it is part of the input stream (predicted and rolled back like any input). Inputs,
  acks and hashes keep flowing. Paused frames are left out of the match's `.wfr`, so the replay verifies like any
  recording.

### Rollback-aware sounds (`CoreGame`, `--net-sfx`)

* **reconcile** (default; what rollback fighting games do): sounds of a frame play when it is first simulated and
  are remembered per frame. When a rollback re-simulates the frame, a sound that no longer happens is stopped
  (`StopSfx`, if at most 30 frames old and no later frame plays the same id). A sound that now happens and did not
  before plays late, if at most 6 frames old. Identical sounds are left alone.
* **confirmed**: only sounds of frames whose inputs are all known (never wrong, about a round trip late).
* **predicted**: the old behaviour (first simulation only).

Measured on a 2-player battle (90 ms + 20 ms jitter, 5 % loss, 206 rollbacks, 74 true sounds per peer), counted
against a straight replay of the confirmed inputs: predicted **3 wrong + 27 missing**, reconcile **0 + 0** (26 + 8
late, 4 + 1 cancelled), confirmed 0 + 0 (`tests/core_online.rs`).

### Rejoin

Each seat has a secret: its SHA-1 goes in the hello and in `start`, the secret stays in the player's **rejoin
ticket** (room, signal, token, nonce, slot, secret: `<data>/net-rejoin.json`, localStorage in the browser, removed
when the match is decided). A rejoining game (`--net-rejoin`, the menu's *Rejoin last match*, or automatically, see
below) connects to the room again and sends `rejoin` instead of hellos. The **acting host** (the connected player
with the lowest slot) checks the secret, drops the old connection of that seat if it still believes in it, and once
every player agrees on the slot's disconnect frame `d`:

1. hands the slot over (`P2PSession::grant_rejoin`): seat epoch + 1, inputs neutral from `d` to `from`, and the new
   peer's from `from` on. `from` is after every input the host has already sent, plus `rejoin_lead` (90 frames, time
   to load and catch up). The seat change travels in every input packet to peers whose status shows an older epoch,
   with the host's inputs. No peer can confirm (hash) a frame past `from` without having it, so nobody hashes a
   state that ignores the handover. Status entries carry the epoch, and stale disconnect reports of an older seat
   are ignored.
2. sends `grant` (match, seat, per-slot epochs / disconnects, the inputs of slots no connected peer serves) and the
   **snapshot** of its newest globally confirmed state as a blob (1 KB chunks over the unreliable channel, bitmap
   acks; a wf-core battle is ~67 KB).

The rejoiner loads the snapshot and **verifies its state hash against the host's** (refused otherwise). It continues
the host's hash chain from there, catches up quietly at up to 8 frames per tick with confirmed inputs, then plays.
Every later state hash is compared with the peers' as usual. Its replay starts from the snapshot (`start_state`).

**Automatic reconnect**: a player that lost every other player (`watch_connection`) probes. This happens when they
went silent (not announced by a goodbye or by the signaling server's `PeerLeft`) or when its own signaling
connection closed. It opens a new connection to the room (natively the `reconnect` factory, in the browser JS opens
one when asked through `need_transport`) and asks to rejoin. The session keeps running meanwhile and is replaced
only by a grant. So the survivor of a 2-player match, which cannot tell whose network died, stays the host the other
comes back to.

**Host migration**: the host role only matters for the lobby, spectators and rejoins. When the host leaves, the
connected player with the lowest slot acts as host: it answers late hellos, serves spectators (they accept the
confirmed inputs from any peer of the match and send their acks to every player now and then) and grants rejoins.

### PvP homing

With the PvP rule set the Kamikaze Ghost (the game's only homing shot) chases the nearest target among the live
enemies **and the opposing players** (alive, hittable, not being eaten: `player::pvp_targetable`), by body-box
centre, enemies first on ties (`shots::pvp_homing_targets`). Before this it found nothing in an enemy-less PvP match
and exploded. Instant Transmission's target pick also offers the opposing players on camera. Both only apply with
`opts.pvp`, so single-player / co-op hashes and the lockstep suites are unchanged.

## 6. Internet play

* **ICE**: `ice::IceConfig` (STUN + TURN, credentials, relay-only). `--net-ice auto` (default for a non-local
  signaling server) fetches `GET <signal origin>/ice[?key=]`. wf-signal answers `{"iceServers": [stun…, {turn…,
  username, credential}], "ttl"}` with **TURN REST API** credentials: username `<expiry>:wf`, password
  base64(HMAC-SHA1(secret, username)), what coturn's `use-auth-secret` and `wf-turn --secret` check. If that fails,
  Google's public STUN is used. Natively matchbox takes one `RTCIceServer`, so all URLs are merged under the first
  credentials (a warning if they differ). The browser passes the list as is.
* **Relay only** (`--net-relay-only`, `?relay`): the browser sets `iceTransportPolicy: "relay"`. Natively our
  signaling client drops every non-relay ICE candidate both ways, and strips them from browsers' SDPs, so only
  relayed paths exist: the situation two symmetric NATs force. Used to test TURN on one machine.
* **wss://**: our native signaling client (`wsclient.rs`, tokio-tungstenite + rustls/ring) trusts the public CA
  roots plus `--net-ca FILE` / `WF_NET_CA` (self-signed tests). On the server side, a reverse proxy (Caddy) or
  `wf-signal --tls ADDR --tls-cert --tls-key`.
* **Room tokens / server key**: a host on a non-local server gets a 6-character token, so the invite is
  `CODE-TOKEN` and the URL `/wf-CODE?token=T[&key=K]`. wf-signal fixes a room's token with its first peer and
  refuses others (`--require-token` refuses token-less rooms). `--key` closes the server and `/ice` to clients
  without `?key=`. It also caps peers per room (8) and in total.
* **wf-turn**: a small UDP TURN relay on webrtc-rs `turn` (REST or static users), for local tests and small
  deployments. coturn is the recipe's default.
* The signaling server's `/ice` is CORS-enabled, so a game page on another origin can use it.

Real-process scenarios on one machine (`scripts/internet_sim.sh`, release build, every packet +60 ms and 3 % loss
each way, random inputs; both peers' state hash compared at the end):

| scenario | result |
|---|---|
| **relay**: wf-turn (REST secret) + wf-signal `/ice`, both natives `--net-ice auto --net-relay-only` | same hash at 1500, both clients authenticated at the TURN server |
| **wss**: wf-signal `--tls` (throwaway CA + localhost cert, `--net-ca`), `--key` + `--require-token` | same hash at 1200; `/ice` without key → 403; a client with a wrong token refused |
| **rejoin**: joiner vanishes at frame 900 (`--net-exit-at`), restarted 2 s later with `--net-rejoin` | snapshot of state 1051 (67 KB), handover at 1143, same hash at 2400; both `.wfr` (the rejoiner's from its snapshot) verify OK |
| **pause**: room rule *everyone*, joiner holds its menu 120 frames | same hash at 1500; session frame 1657, world frame 1537, 0 stalls |

Browser (headless Chrome, `chrome-devtools-axi`, wasm build) ↔ native:

* the in-game lobby on the web canvas (game font): *Host a room* → *Create room* → a native `--net-join CODE`
  shows up in the room list (fighter, form, ready) → *Start match*: running, 6 456 hash checks on the native side,
  0 desyncs; the browser played sounds with 17 late / 1 cancelled by reconciliation.
* browser `?net=host&relay&ice=auto` (`iceTransportPolicy: relay`, credentials from `/ice`) ↔ native
  `--net-ice auto --net-relay-only` through wf-turn: 1200 frames confirmed, 6 462 checks, 0 desyncs.

Virtual-network tests (`cargo test -p wf-net`): toy-game rejoin after a disconnect (60 / 120 / 20 ms, up to 10 %
loss), the host leaving (the next player serves the spectator and grants the host's rejoin), a tampered snapshot
refused. wf-core battles (`tests/core_online.rs`): rejoin after a crash with the ticket (3 players), in-place rejoin
after an outage in a 2-player PvP match (the survivor keeps its session, the cut-off player reconnects), shared and
local pause (replays verify with the paused frames left out), the sounds comparison above, and PvP ghost homing.

## 7. Limitations / next

* **Hosting is not set up**: the recipe is files only (`engine/deploy/netplay/`). Until a server runs, internet
  play needs someone to run `wf-signal` (+ TURN) on a reachable address.
* TURN over TLS (`turns:` 5349, for UDP-blocking networks) needs a certificate for coturn (documented, off by
  default). wf-signal only mints coturn-style REST credentials. A provider with its own credential API (Cloudflare,
  Twilio) means passing its ICE JSON to the clients.
* Rejoin: one acting host decides each handover. In a network partition two players could both believe they are the
  acting host and hand the same seat out differently (a desync would show it). The rejoiner's snapshot is the
  newest confirmed state, so with a very slow transfer the others stall for a moment at `from + max_prediction`.
* A browser tab in the background stops `requestAnimationFrame`: the others drop it after 5 s, and it rejoins by
  itself when visible again (its signaling connection usually survives; otherwise it reconnects).
* Spectators still join from frame 0 (catching up at 8x), not from a snapshot.
* Sounds: a sound added by a rollback more than 6 frames late is skipped. Music changes are reconciled only by
  playing a newly predicted song.
* Two native peers sharing one data directory save the same replay file name (harmless: identical content).
* The windowed native shell has no on-screen keyboard, so the lobby's text fields (name, code, server) need a
  keyboard.
