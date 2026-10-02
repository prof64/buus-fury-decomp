// AudioWorklet: a second instance of wf_web.wasm running the game's sound driver (wf-audio) on the audio thread.
// The main thread posts {type:'init', module, packs} once (packs = [[ [path, bytes], ... ], ...], base first, only
// the audio/ files + pack.json), then {type:'events', words: Int32Array} every frame (AudioEvent quadruples).
// AudioWorkletGlobalScope has no TextDecoder, so paths are decoded by hand (they are ASCII).

class WfAudio extends AudioWorkletProcessor {
  constructor() {
    super();
    this.x = null;
    this.port.onmessage = (e) => this.onMessage(e.data);
    this.port.onmessageerror = () => this.port.postMessage({ type: 'error', message: 'message could not be deserialized' });
    this.port.postMessage({ type: 'stage', stage: 'processor up' });
  }

  async onMessage(m) {
    try {
      await this.handle(m);
    } catch (e) {
      this.port.postMessage({ type: 'error', message: String(e && e.message || e) });
    }
  }

  async handle(m) {
    if (m.type === 'init') {
      const packs = m.packs.map((list) => new Map(list));
      let mem = null;
      const path = (p, n) => {
        const b = new Uint8Array(mem.buffer, p, n);
        let s = '';
        for (let i = 0; i < n; i++) s += String.fromCharCode(b[i]);
        return s;
      };
      const host = {
        pack_count: () => packs.length,
        file_size: (pk, p, n) => {
          const f = packs[pk]?.get(path(p, n));
          return f ? f.length : -1;
        },
        file_read: (pk, p, n, dst) => {
          const f = packs[pk].get(path(p, n));
          new Uint8Array(mem.buffer, dst, f.length).set(f);
        },
        log: () => {},
        now_ms: () => currentTime * 1000,
      };
      this.port.postMessage({ type: 'stage', stage: 'instantiating' });
      let inst = await WebAssembly.instantiate(m.wasm, { host });
      inst = inst.instance || inst;
      this.port.postMessage({ type: 'stage', stage: 'loading bank' });
      mem = inst.exports.memory;
      const st = inst.exports.wf_audio_init(sampleRate);
      if (st !== 0) {
        const b = new Uint8Array(mem.buffer, inst.exports.wf_str_ptr(), inst.exports.wf_str_len());
        this.port.postMessage({ type: 'error', message: String.fromCharCode(...b) });
        return;
      }
      this.x = inst.exports;
      this.mem = mem;
      this.x.wf_audio_set(m.gain ?? 1, m.linear === false ? 0 : 1);
      this.port.postMessage({ type: 'ready', sampleRate });
    } else if (m.type === 'events' && this.x) {
      const w = m.words;
      for (let i = 0; i + 3 < w.length; i += 4) this.x.wf_audio_event(w[i], w[i + 1], w[i + 2], w[i + 3]);
    } else if (m.type === 'set' && this.x) {
      this.x.wf_audio_set(m.gain, m.linear ? 1 : 0);
    }
  }

  process(_inputs, outputs) {
    const out = outputs[0];
    if (!this.x || !out || out.length < 2) return true;
    const n = out[0].length;
    const p = this.x.wf_audio_render(n);
    const s = new Float32Array(this.mem.buffer, p, n * 2);
    const l = out[0];
    const r = out[1];
    for (let i = 0; i < n; i++) {
      l[i] = s[2 * i];
      r[i] = s[2 * i + 1];
    }
    return true;
  }
}

registerProcessor('wf-audio', WfAudio);
