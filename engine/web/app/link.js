// Link play in the browser: the title's Multiplayer -> Dojo / Trading Post over online play (crates/wf-web/src/link.rs,
// engine/docs/netcode.md "Link play: in the browser"). `?link=host:ROOM` or `?link=join:ROOM` starts the title with
// link play; `&signal=ws://127.0.0.1:3536` the signalling server (default that), `&key=` its key; tests: `&bot=dojo`
// / `&bot=trade` (the scripted player of the native --link-bot), `&quick` (the Dojo's short test rules, as
// --link-quick) and `&linkinfo` (window.wfLinkInfo()).
//
// Per frame, after wf_app_frame (`pump`): (1) the trade store's disk writes are mirrored to localStorage
// (`wf.link.f.<path>`, hex) - each setItem is synchronous and atomic per key - BEFORE (3) the frame's outgoing packets
// leave, so every journal / slot write is durable before a message that depends on it; (2) a room the flow asked for
// (wf_link_poll) is opened: the same Matchbox signalling + WebRTC data channel as online battles (net.js), feeding
// the queue through the wf_net_* exports.

const enc = new TextEncoder();
const dec = new TextDecoder();
const hex = (b) => Array.from(b, (v) => v.toString(16).padStart(2, '0')).join('');
const FILE = 'wf.link.f.';

/// the `link` part of wf_app_init's config from the URL, or null when the URL asks for no link play
export function linkConfig(qs) {
  const v = qs.get('link');
  if (!v) return null;
  const [role, room] = v.split(':');
  let id = localStorage.getItem('wf.link.id');
  if (!id) {
    id = Array.from(crypto.getRandomValues(new Uint8Array(8)), (b) => b.toString(16).padStart(2, '0')).join('');
    localStorage.setItem('wf.link.id', id);
  }
  const disk = {};
  for (let i = 0; i < localStorage.length; i++) {
    const k = localStorage.key(i);
    if (k && k.startsWith(FILE)) disk[k.slice(FILE.length)] = localStorage.getItem(k);
  }
  return {
    host: role === 'host',
    room: (room || '').toUpperCase(),
    signal: qs.get('signal') || 'ws://127.0.0.1:3536',
    key: qs.get('key') || '',
    link_id: id,
    name: qs.get('name') || role,
    disk,
    info: qs.has('linkinfo'),
    bot: qs.get('bot') || undefined,
    quick: qs.has('quick'),
  };
}

/// the per-frame pump (call after wf_app_frame). `api` = { x, mem(), withStr, str }
export function startLink(api) {
  const { x, withStr, str } = api;
  const peers = new Map();
  let ws = null;
  const withBytes = (b, f) => {
    const p = x.wf_alloc(b.length);
    new Uint8Array(api.mem().buffer, p, b.length).set(b);
    const r = f(p, b.length);
    x.wf_dealloc(p, b.length);
    return r;
  };
  function close() {
    for (const p of peers.values()) p.pc.close();
    peers.clear();
    if (ws) {
      ws.onclose = null;
      ws.close();
    }
    ws = null;
  }
  // (as net.js connect: Matchbox PeerEvent / PeerRequest JSON, one negotiated unreliable data channel per pair)
  function connect(url) {
    close();
    const sock = new WebSocket(url);
    ws = sock;
    const signal = (receiver, data) => sock.readyState === 1 && sock.send(JSON.stringify({ Signal: { receiver, data } }));
    const keep = setInterval(() => (sock.readyState === 1 ? sock.send('"KeepAlive"') : sock.readyState > 1 && clearInterval(keep)), 10000);
    const iceDone = (pc) =>
      new Promise((r) => {
        if (pc.iceGatheringState === 'complete') return r();
        const f = () => pc.iceGatheringState === 'complete' && (pc.removeEventListener('icegatheringstatechange', f), r());
        pc.addEventListener('icegatheringstatechange', f);
        setTimeout(r, 3000);
      });
    function peer(id) {
      let p = peers.get(id);
      if (p) return p;
      const pc = new RTCPeerConnection();
      const dc = pc.createDataChannel('matchbox_socket_0', { negotiated: true, id: 0, ordered: false, maxRetransmits: 0 });
      dc.binaryType = 'arraybuffer';
      p = { pc, dc, pending: [], remote: false };
      peers.set(id, p);
      dc.onopen = () => withStr(id, (a, b) => x.wf_net_peer(a, b, 1));
      dc.onclose = () => withStr(id, (a, b) => x.wf_net_peer(a, b, 0));
      dc.onmessage = (e) => withStr(id, (ip, il) => withBytes(new Uint8Array(e.data), (dp, dl) => x.wf_net_recv(ip, il, dp, dl)));
      return p;
    }
    const addCandidates = async (p) => {
      for (const c of p.pending.splice(0)) if (c && c !== 'null') await p.pc.addIceCandidate(JSON.parse(c)).catch(() => {});
    };
    sock.onmessage = async (ev) => {
      const m = JSON.parse(ev.data);
      if (m.IdAssigned) withStr(m.IdAssigned, (p, n) => x.wf_net_set_id(p, n));
      else if (m.NewPeer) {
        const p = peer(m.NewPeer);
        await p.pc.setLocalDescription(await p.pc.createOffer());
        await iceDone(p.pc);
        signal(m.NewPeer, { Offer: p.pc.localDescription.sdp });
      } else if (m.PeerLeft) {
        const p = peers.get(m.PeerLeft);
        if (p) {
          p.pc.close();
          peers.delete(m.PeerLeft);
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
    let opened = false;
    sock.onopen = () => (opened = true);
    sock.onclose = () => {
      const why = opened ? 'the signaling server closed the connection' : `cannot reach the signaling server ${url}`;
      withStr(why, (p, n) => x.wf_net_closed(p, n));
    };
  }
  const link = {
    pump() {
      // (1) the disk first: durable before any packet of this frame leaves
      for (let n = x.wf_link_disk_next(); n; n = x.wf_link_disk_next()) {
        const b = new Uint8Array(api.mem().buffer, x.wf_str_ptr(), n).slice();
        const key = dec.decode(b.subarray(1, 1 + b[0]));
        localStorage.setItem(FILE + key, hex(b.subarray(1 + b[0])));
      }
      // (2) a room to open / the flow's connection dropped (close it: the friend sees the link go)
      const r = x.wf_link_poll();
      if (r === 1) connect(str());
      else if (r === 2) close();
      // (3) the frame's packets
      for (let n = x.wf_net_out(); n; n = x.wf_net_out()) {
        const b = new Uint8Array(api.mem().buffer, x.wf_str_ptr(), n);
        const id = dec.decode(b.subarray(1, 1 + b[0]));
        const p = peers.get(id);
        if (p && p.dc.readyState === 'open') {
          try {
            p.dc.send(b.slice(1 + b[0]));
          } catch {}
        }
      }
    },
    info() {
      x.wf_link_info();
      try {
        return JSON.parse(str());
      } catch {
        return null;
      }
    },
    close,
  };
  window.wfLink = link;
  window.wfLinkInfo = () => link.info();
  return link;
}
