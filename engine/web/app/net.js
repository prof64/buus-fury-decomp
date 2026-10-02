// Online multiplayer in the browser (engine/docs/netcode.md). The lobby and the rollback session run in the game
// wasm (wf_net_* exports, crates/wf-web/src/net.rs); this module is the transport: the Matchbox signaling protocol
// over a WebSocket and one RTCDataChannel per peer configured exactly like matchbox_socket's unreliable channel
// (negotiated id 0, unordered, no retransmits), so browser and native (buus-fury --net-*) peers meet in one room.
//
// URL: ?net (form) or ?net=host|join|spectate|synctest&room=CODE&signal=ws://host:3536&players=2&pvp&fighter=vegeta
// &form=26&enemy=t60&arena=wasteland&count=1&seed=1&delay=2&latency=0&loss=0&name=...

const dec = new TextDecoder();
const enc = new TextEncoder();

const FIGHTERS = [
  ['goku', 'Goku', 2], ['gohan', 'Gohan', 10], ['goten', 'Goten', 19], ['trunks', 'Trunks', 21], ['vegeta', 'Vegeta', 26],
  ['gotenks', 'Gotenks', 31], ['vegito', 'Vegito', 39], ['gogeta', 'Gogeta', 41],
];
const ENEMIES = [
  ['t68', 'Majin Fighter', 'forest'], ['t60', 'Kid Buu', 'wasteland'], ['t51', 'Janemba', 'crater'], ['t26+~t29', 'Super Buu', 'crater'],
  ['t129', 'Broly', 'crater'], ['t145', 'Yakon', 'yakon'], ['t10', 'Volcano boss', 'volcano'], ['t18', 'Demon World boss', 'demon'],
];

const css = `
#netcard{position:fixed;inset:0;display:flex;align-items:center;justify-content:center;background:rgba(0,0,0,.6);z-index:20;padding:16px}
#netcard .card{width:min(560px,100%)}#netcard label{display:flex;justify-content:space-between;align-items:center;gap:12px;padding:5px 0;border-bottom:1px solid var(--line)}
#netcard input,#netcard select{font:inherit;background:#26252b;color:var(--fg);border:1px solid var(--line);border-radius:6px;padding:3px 8px;max-width:60%}
#netcard .code{font:700 34px ui-monospace,monospace;letter-spacing:.2em;color:var(--accent)}
#netstat{position:fixed;left:50%;transform:translateX(-50%);top:10px;z-index:6;font:12px/1.4 ui-monospace,monospace;background:rgba(0,0,0,.6);padding:5px 8px;border-radius:6px;white-space:pre;pointer-events:auto}
#netstat .bad{color:var(--bad);font-weight:700}#netstat a{color:var(--accent)}`;

function el(html) {
  const t = document.createElement('template');
  t.innerHTML = html.trim();
  return t.content.firstChild;
}

/// Ask the user (or the URL) what to do. Resolves to the config for wf_net_lobby.
function form(qs) {
  const preset = qs.get('net');
  const num = (k, d) => (qs.has(k) ? Number(qs.get(k)) : d);
  const def = {
    signal: qs.get('signal') || `ws://${location.hostname || '127.0.0.1'}:3536`,
    room: (qs.get('room') || '').toUpperCase(),
    players: num('players', 2),
    pvp: qs.has('pvp'),
    pvp_enemies: qs.has('pvp_enemies'),
    fighter: qs.get('fighter') || 'goku',
    form: num('form', undefined),
    level: num('level', undefined),
    enemy: qs.get('enemy') || 't60',
    arena: qs.get('arena') || undefined,
    count: num('count', 1),
    seed: num('seed', 1),
    delay: num('delay', 2),
    prediction: num('prediction', 12),
    latency: num('latency', 0),
    jitter: num('jitter', 0),
    loss: num('loss', 0),
    net_seed: num('net_seed', 1),
    name: qs.get('name') || '',
    synctest: num('synctest', 8),
    stun: qs.get('stun') || '',
  };
  if (!def.arena) def.arena = (ENEMIES.find((e) => e[0] === def.enemy) || ENEMIES[0])[2];
  if (['host', 'join', 'spectate', 'synctest'].includes(preset)) return Promise.resolve({ ...def, mode: preset });
  return new Promise((resolve) => {
    const c = el(`<div id="netcard"><div class="card"><h1>Play online</h1>
      <p class="muted">Create a room and share its code, or join one. Everyone needs the same ROM (and mods).</p>
      <label>Room <span><button data-m="host">Create room</button> <input id="nroom" placeholder="CODE" size="7" value="${def.room}"> <button data-m="join">Join</button> <button data-m="spectate">Watch</button></span></label>
      <label>Name <input id="nname" value="${def.name}" placeholder="optional"></label>
      <label>Fighter <select id="nfighter">${FIGHTERS.map(([id, n, f]) => `<option value="${id}" data-form="${f}"${id === def.fighter ? ' selected' : ''}>${n}</option>`).join('')}</select></label>
      <label>Form <input id="nform" type="number" min="0" max="53" value="${def.form ?? ''}" placeholder="default"></label>
      <h2>Room settings (host)</h2>
      <label>Mode <select id="nmode"><option value="coop">Co-op vs enemies</option><option value="pvp"${def.pvp ? ' selected' : ''}>PvP</option><option value="pvpe">PvP + enemies</option></select></label>
      <label>Players <select id="nplayers">${[2, 3, 4].map((n) => `<option${n === def.players ? ' selected' : ''}>${n}</option>`).join('')}</select></label>
      <label>Enemy <select id="nenemy">${ENEMIES.map(([id, n]) => `<option value="${id}"${id === def.enemy ? ' selected' : ''}>${n}</option>`).join('')}</select></label>
      <label>Input delay (frames) <input id="ndelay" type="number" min="0" max="8" value="${def.delay}"></label>
      <label>Signaling server <input id="nsignal" value="${def.signal}"></label>
      <p class="muted">Run one with <kbd>cargo run --release -p wf-net --features server --bin wf-signal</kbd>.</p>
      </div></div>`);
    document.body.append(c);
    c.querySelectorAll('button[data-m]').forEach((b) => {
      b.onclick = () => {
        const v = (id) => c.querySelector(id).value;
        const mode = b.dataset.m;
        if (mode !== 'host' && !v('#nroom')) return c.querySelector('#nroom').focus();
        const enemy = v('#nenemy');
        const m = v('#nmode');
        c.remove();
        resolve({
          ...def, mode, room: v('#nroom').toUpperCase(), name: v('#nname'), fighter: v('#nfighter'),
          form: v('#nform') === '' ? undefined : Number(v('#nform')), players: Number(v('#nplayers')), enemy,
          arena: (ENEMIES.find((e) => e[0] === enemy) || ENEMIES[0])[2], pvp: m !== 'coop', pvp_enemies: m === 'pvpe',
          delay: Number(v('#ndelay')), signal: v('#nsignal'),
        });
      };
    });
  });
}

/// Run the lobby until the match starts. `api` = { x, mem(), withStr, str, show }. Resolves to the net handle
/// (slot(), pump(), status) or null when refused / failed (message shown).
export async function startNet(api, qs) {
  const { x, withStr, str } = api;
  document.head.append(el(`<style>${css}</style>`));
  const cfg = await form(qs);
  const withBytes = (b, f) => {
    const p = x.wf_alloc(b.length);
    new Uint8Array(api.mem().buffer, p, b.length).set(b);
    const r = f(p, b.length);
    x.wf_dealloc(p, b.length);
    return r;
  };
  if (withStr(JSON.stringify(cfg), (p, n) => x.wf_net_lobby(p, n)) !== 0) {
    alert(`Online: ${str()}`);
    return null;
  }
  const room = str();
  const statusBox = el('<div id="netstat"></div>');
  document.body.append(statusBox);
  let last = {};
  const net = {
    room,
    cfg,
    peers: new Map(),
    slot: () => Math.max(0, x.wf_net_slot()),
    status: () => last,
    pump() {
      for (let n = x.wf_net_out(); n; n = x.wf_net_out()) {
        const b = new Uint8Array(api.mem().buffer, x.wf_str_ptr(), n);
        const id = dec.decode(b.subarray(1, 1 + b[0]));
        const p = net.peers.get(id);
        if (p && p.dc.readyState === 'open') {
          try {
            p.dc.send(b.slice(1 + b[0]));
          } catch {}
        }
      }
    },
    report() {
      x.wf_net_report();
      return str();
    },
    /// the match as a .wfr replay (wf-replay format; plays in the Replays panel / buus-fury --replay)
    replay(upto = -1) {
      const n = x.wf_net_replay(upto);
      return n ? new Uint8Array(api.mem().buffer, x.wf_str_ptr(), n).slice() : null;
    },
  };
  window.wfNet = net;
  let lastDraw = 0;
  net.draw = () => {
    const now = performance.now();
    if (now - lastDraw < 250) return;
    lastDraw = now;
    x.wf_net_status();
    try {
      last = JSON.parse(str());
    } catch {}
    window.wfNetStatus = last;
    const s = last;
    const rtt = (s.rtt || []).map(([, r]) => Math.round(r)).join('/');
    let t;
    if (s.phase === 'running' || s.phase === 'spectating' || s.phase === 'synctest')
      t = `${s.phase === 'running' ? `P${(s.local_slot ?? 0) + 1}` : s.phase.toUpperCase()}  room ${room}  rtt ${rtt || '-'} ms  frame ${s.frame}\nrollbacks ${s.stats?.rollbacks ?? 0} (max ${s.stats?.max_rollback ?? 0})  stalls ${s.stats?.stalls ?? 0}  checks ${s.stats?.hash_checks ?? 0}`;
    else t = `ROOM ${room}  ${s.phase || 'connecting'}  ${s.players_present || 1}/${s.want_players || 2} players\n${(s.roster || []).map(([n, r]) => `${n} (${r})`).join(', ')}`;
    statusBox.textContent = t;
    if (s.desync) {
      const d = el(`<div class="bad">DESYNC ${s.desync} </div>`);
      const a = el('<a href="#">download report</a>');
      a.onclick = (e) => {
        e.preventDefault();
        const url = URL.createObjectURL(new Blob([net.report()], { type: 'application/json' }));
        Object.assign(document.createElement('a'), { href: url, download: `desync-${room}-f${s.frame}.json` }).click();
        const r = net.replay();
        if (r) Object.assign(document.createElement('a'), { href: URL.createObjectURL(new Blob([r])), download: `desync-${room}-f${s.frame}.wfr` }).click();
      };
      d.append(a);
      statusBox.append(d);
    }
    for (const p of s.disconnected || []) statusBox.append(el(`<div class="bad">${p} disconnected</div>`));
    if (s.phase === 'running' || s.phase === 'spectating') {
      const a = el('<a href="#">save replay</a>');
      a.onclick = (e) => {
        e.preventDefault();
        const r = net.replay();
        if (r) Object.assign(document.createElement('a'), { href: URL.createObjectURL(new Blob([r])), download: `online-${room}.wfr` }).click();
      };
      statusBox.append(document.createTextNode('\n'), a);
    }
    if (s.outcome && s.outcome !== 'running') statusBox.append(el(`<div class="bad">${s.winner != null ? `PLAYER ${s.winner + 1} WINS` : s.outcome.toUpperCase()}</div>`));
  };
  if (cfg.mode === 'synctest') {
    net.draw();
    return net;
  }

  // ---- signaling (Matchbox protocol: JSON PeerEvent / PeerRequest) + WebRTC
  const ice = cfg.stun ? [{ urls: cfg.stun }] : [];
  const ws = new WebSocket(`${cfg.signal.replace(/\/$/, '')}/wf-${room}`);
  const signal = (receiver, data) => ws.readyState === 1 && ws.send(JSON.stringify({ Signal: { receiver, data } }));
  setInterval(() => ws.readyState === 1 && ws.send('"KeepAlive"'), 10000);
  const iceDone = (pc) =>
    new Promise((r) => {
      if (pc.iceGatheringState === 'complete') return r();
      const f = () => pc.iceGatheringState === 'complete' && (pc.removeEventListener('icegatheringstatechange', f), r());
      pc.addEventListener('icegatheringstatechange', f);
      setTimeout(r, 3000);
    });
  function peer(id) {
    let p = net.peers.get(id);
    if (p) return p;
    const pc = new RTCPeerConnection({ iceServers: ice });
    const dc = pc.createDataChannel('matchbox_socket_0', { negotiated: true, id: 0, ordered: false, maxRetransmits: 0 });
    dc.binaryType = 'arraybuffer';
    p = { pc, dc, pending: [], remote: false };
    net.peers.set(id, p);
    dc.onopen = () => withStr(id, (a, b) => x.wf_net_peer(a, b, 1));
    dc.onclose = () => withStr(id, (a, b) => x.wf_net_peer(a, b, 0));
    dc.onmessage = (e) => withStr(id, (ip, il) => withBytes(new Uint8Array(e.data), (dp, dl) => x.wf_net_recv(ip, il, dp, dl)));
    pc.oniceconnectionstatechange = () => {};
    return p;
  }
  const addCandidates = async (p) => {
    for (const c of p.pending.splice(0)) if (c && c !== 'null') await p.pc.addIceCandidate(JSON.parse(c)).catch(() => {});
  };
  ws.onmessage = async (ev) => {
    const m = JSON.parse(ev.data);
    if (m.IdAssigned) withStr(m.IdAssigned, (p, n) => x.wf_net_set_id(p, n));
    else if (m.NewPeer) {
      // we are the offerer for this pair
      const p = peer(m.NewPeer);
      await p.pc.setLocalDescription(await p.pc.createOffer());
      await iceDone(p.pc);
      signal(m.NewPeer, { Offer: p.pc.localDescription.sdp });
    } else if (m.PeerLeft) {
      const p = net.peers.get(m.PeerLeft);
      if (p) {
        p.pc.close();
        net.peers.delete(m.PeerLeft);
        withStr(m.PeerLeft, (a, b) => x.wf_net_peer(a, b, 0));
      }
    } else if (m.Signal) {
      const { sender, data } = m.Signal;
      const p = peer(sender);
      if (data.Offer) {
        await p.pc.setRemoteDescription({ type: 'offer', sdp: data.Offer });
        p.remote = true;
        await addCandidates(p);
        await p.pc.setLocalDescription(await p.pc.createAnswer());
        await iceDone(p.pc);
        signal(sender, { Answer: p.pc.localDescription.sdp });
      } else if (data.Answer) {
        await p.pc.setRemoteDescription({ type: 'answer', sdp: data.Answer });
        p.remote = true;
        await addCandidates(p);
      } else if (data.IceCandidate) {
        p.pending.push(data.IceCandidate);
        if (p.remote) await addCandidates(p);
      }
    }
  };
  ws.onerror = () => (statusBox.textContent = `Cannot reach the signaling server ${cfg.signal}`);

  // ---- lobby loop until the host starts the match
  const ok = await new Promise((resolve) => {
    const t = setInterval(() => {
      const r = x.wf_net_poll();
      net.pump();
      lastDraw = 0;
      net.draw();
      if (r === 1 || r === 2) {
        clearInterval(t);
        resolve(r === 1);
      }
    }, 16);
  });
  if (!ok) {
    statusBox.textContent = `Refused: ${last.outcome || 'unknown'}`;
    return null;
  }
  return net;
}
