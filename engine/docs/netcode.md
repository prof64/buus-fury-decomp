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
