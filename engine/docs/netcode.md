# Netcode: online multiplayer over the deterministic core

`engine/crates/wf-net` adds online play to the remake: GGPO-style **rollback** over `wf-core`'s battle `Session`,
peer-to-peer over **WebRTC data channels**, rooms by code through a **Matchbox** signaling server, co-op (2–4 players
vs any enemies / bosses) and **PvP**, spectators, per-frame desync detection with reports, a local **sync test**, and
every match saved as a **wf-replay** recording. Native (`buus-fury --net-*`) and browser (`/app/?net`) peers play in the
same room.

```
cargo build --release -p wf-native -p wf-net --features wf-net/server --bin buus-fury --bin wf-signal
target/release/wf-signal                                   # signaling server, ws://127.0.0.1:3536
target/release/buus-fury --net-host --net-players 2        # prints the room code, e.g. KQZTA
target/release/buus-fury --net-join KQZTA --fighter vegeta --form 26
cargo test -p wf-net                                       # session logic, wf-core over a bad virtual network, sync tests
engine/crates/wf-net/scripts/two_native.sh 6000 100 0.05   # two headless clients over real WebRTC, 100 ms + 5 % loss
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
         session.rs     P2PSession<G>: rollback, prediction, time sync, hashes, disconnects, spectator feed
         spectator.rs   SpectatorSession<G>: confirmed inputs from the host, catch-up
         synctest.rs    SyncTestSession<G>: roll back N frames every frame, compare checksums (+ byte snapshots)
         proto.rs       wire format (LE binary; lobby messages are JSON)
         lobby.rs       hello / start / full; room codes
         transport.rs   Transport trait; SimNet (virtual network), Conditioner (latency/jitter/loss), QueueTransport
         matchbox.rs    MatchboxTransport (native WebRTC, feature `webrtc`)
         netsim.rs      NetLobby, NetSim (wf_view::Sim for the frontends), SyncSim, NetStatus, desync report, replay
         cli.rs         --net-* options, blocking lobby, headless runner
         bin/wf-signal  Matchbox full-mesh signaling server (feature `server`)
wf-native  --net-* → NetSim / SyncSim as the App's Sim; overlay; camera of the local slot   (main.rs, few lines)
wf-web     net.rs: wf_net_* exports (lobby, packet queues, status, report, replay); wf_app_init {"net": true}
web/app    net.js: Matchbox signaling over WebSocket + RTCPeerConnection / RTCDataChannel; lobby form; overlay
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

### Replays

Every match is a [wf-replay](replays.md) recording: `NetSim::replay()` builds a `.wfr` (sim `wf-core/battle`, the
battle config + `"pvp": true` when PvP — the app's replay player applies it —, every slot's confirmed inputs,
checkpoints from the match's own confirmed hashes every 60 frames). Native peers save it to `<data>/replays/`
(`--net-record DIR`) every 30 s and when the match ends, so it shows in the F10 replay list; the browser offers
"save replay" in the status box (`wf_net_replay`). Spectators can save one too.

## 3. How to play

1. Someone runs the signaling server where everyone can reach it: `wf-signal 0.0.0.0:3536` (game traffic is
   peer-to-peer; the server only relays the WebRTC handshake). Browsers on `https` pages need `wss://` (a TLS proxy).
2. Host: `buus-fury --net-host --signal ws://HOST:3536 [--net-players 2..4] [--pvp | --pvp --pvp-enemies]
   [--arena wasteland --enemy t60 --count 1 --seed 7] [--fighter goku --form 2]` — prints the room code; the window
   opens when everyone is in. Browser: `/app/?net` → "Create room".
3. Others: `buus-fury --net-join CODE --signal ws://HOST:3536 --fighter vegeta --form 26`, or `/app/?net` → code → Join.
4. Spectators: `--net-spectate CODE` / "Watch" (any time, also mid-match: catches up at 8 frames per tick).
5. Options: `--net-delay N` (input delay frames), `--net-prediction N`, `--net-name`, `--net-stun stun:host:port`
   (default: none for a localhost signaling server, Google's public STUN otherwise; `none` disables),
   `--net-report DIR`, test-only `--net-latency MS --net-jitter MS --net-loss P --net-seed N`
   (applied to everything this peer sends), `--net-random-input SEED`, `--net-result FILE`.
6. Local sync test: `buus-fury --net-synctest 8 [--net-synctest-bytes] [--players N]` plays normally while rolling
   back 8 frames every frame; a nondeterminism shows as DESYNC on the overlay and on stderr. Browser:
   `?net=synctest&synctest=8`.

Ports: signaling `3536/tcp` (WebSocket). Peer traffic: WebRTC (ICE / DTLS / SCTP over UDP, ephemeral ports chosen
by ICE). Behind symmetric NATs a TURN server would be needed (`--net-stun` takes any ICE URL, but credentials are
not exposed yet).

Browser URL parameters (skip the form): `?net=host|join|spectate|synctest&room=CODE&signal=ws://…:3536&players=2
&pvp&pvp_enemies&fighter=vegeta&form=26&enemy=t60&arena=wasteland&count=1&seed=1&delay=2&latency=100&loss=0.05`.

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

## 5. Limitations / next

* **Pausing** (Esc / settings) in an online match stalls the local simulation; the others wait at the prediction
  window and drop the peer after 5 s. No in-match pause protocol yet. Rewind / save states are off for online sims.
* **Sounds during rollbacks**: sounds of re-simulated frames are dropped and the predicted frame's sounds were
  already played, so a mispredicted hit can play a sound that "didn't happen" or miss one (GGPO-typical). Music is
  unaffected.
* **No rejoin** after a disconnect (the slot stays neutral); no host migration (the host only matters for the lobby
  and for feeding spectators — if it leaves, players continue, spectators stop).
* PvP: homing projectiles still home on enemies only; no PvP-specific balance (damage is the enemy-attack pipeline).
* Browser ↔ native was tested on one machine (mDNS host candidates resolved); across the internet it relies on STUN
  and would need TURN behind symmetric NATs.
* Two native peers sharing one data directory save the same replay file name (harmless: identical content).
* TURN credentials, `wss` signaling, and a lobby UI in the native shell (it uses stderr + command-line options) are not
  done.
