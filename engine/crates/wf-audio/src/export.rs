//! Content export: `audio/index.json`, `audio/driver.json`, `audio/instruments.json`,
//! `audio/songs/<id>.json`, `audio/sfx/<id>.json`, `audio/samples/<id>.wav`. Field reference: docs/audio.md.

use serde_json::{json, Map, Value};
use wf_rom::{content::Content, hx};

use crate::bank::{Entry, SoundBank};
use crate::driver::{self, Driver, EffectKind};
use crate::wav;

const NAMES: &str = include_str!("../data/names.json");

pub fn sample_id(i: usize) -> String {
    format!("sample_{i:03}")
}
pub fn song_id(i: usize) -> String {
    format!("song_{i:02}")
}
pub fn sfx_id(i: usize) -> String {
    format!("sfx_{i:02}")
}
pub fn inst_id(i: usize) -> String {
    format!("inst_{i:02}")
}

const NOTE_NAMES: [&str; 12] = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];

/// Note number -> name (note 60 = "C5" = the sample's own pitch).
pub fn note_name(n: u8) -> String {
    if n == 0xFF {
        return "off".into();
    }
    format!("{}{}", NOTE_NAMES[n as usize % 12], n / 12)
}

fn loop_json(e: &Entry) -> Value {
    match e.loop_mode & 3 {
        0 => Value::Null,
        m => json!({"mode": if m & 2 != 0 { "pingpong" } else { "forward" }, "start": e.loop_start, "end": e.length}),
    }
}

fn entry_json(bank: &SoundBank, e: &Entry) -> Value {
    json!({
        "sample": sample_id(e.sample),
        "file": format!("audio/samples/{}.wav", sample_id(e.sample)),
        "length": e.length,
        "rate_hz": e.rate,
        "loop": loop_json(e),
        "loop_mode_raw": e.loop_mode,
        "rom": hx(e.rom),
        "pcm_rom": hx(bank.samples[e.sample].rom),
    })
}

pub fn write(bank: &SoundBank, out: &mut Content) {
    let names: Value = serde_json::from_str(NAMES).unwrap_or(Value::Null);

    // ---- samples: rate / loop of the first entry that uses each sample, for the WAV header
    let mut first: Vec<Option<&Entry>> = vec![None; bank.samples.len()];
    for e in bank.instruments.iter().chain(bank.sfx.iter()) {
        if first[e.sample].is_none() {
            first[e.sample] = Some(e);
        }
    }
    let mut samples_idx = Vec::new();
    for (i, s) in bank.samples.iter().enumerate() {
        let e = first[i];
        let rate = e.map(|e| e.rate as u32).unwrap_or(16000).max(1);
        let lp = e.filter(|e| e.loop_mode & 1 != 0).map(|e| (e.loop_start as u32, e.length as u32));
        out.put(format!("audio/samples/{}.wav", sample_id(i)), wav::mono_s8(&s.pcm, rate, lp));
        samples_idx.push(json!({"id": sample_id(i), "length": s.pcm.len(), "rom": hx(s.rom), "wav_rate_hz": rate}));
    }

    // ---- instruments
    let insts: Vec<Value> = bank
        .instruments
        .iter()
        .enumerate()
        .map(|(i, e)| {
            let mut v = entry_json(bank, e);
            v["id"] = json!(inst_id(i));
            v["index"] = json!(i);
            v
        })
        .collect();
    out.put_json(
        "audio/instruments.json",
        &json!({"note": "song instruments = sample entries; a note plays the sample at rate_hz * pitch(note) (note 60 = 1.0)",
                "instruments": insts}),
    );

    // ---- SFX
    let mut sfx_index = Vec::new();
    for (i, e) in bank.sfx.iter().enumerate() {
        let n = &names["sfx"][i.to_string()];
        let name = n["name"].as_str().map(String::from).unwrap_or_else(|| format!("Sound {i:02}"));
        let mut v = entry_json(bank, e);
        v["id"] = json!(sfx_id(i));
        v["index"] = json!(i);
        v["name"] = json!(name);
        v["trigger"] = n.get("trigger").cloned().unwrap_or(json!(""));
        v["default_volume"] = json!(n["game_volume"].as_u64().unwrap_or(64));
        v["duration_s"] = json!(if e.loop_mode & 1 != 0 { Value::Null } else { json!(round(e.length as f64 / e.rate.max(1) as f64, 4)) });
        out.put_json(format!("audio/sfx/{}.json", sfx_id(i)), &v);
        sfx_index.push(json!({"id": sfx_id(i), "index": i, "name": name, "file": format!("audio/sfx/{}.json", sfx_id(i)),
                              "loops": e.loop_mode & 1 != 0}));
    }

    // ---- songs
    let effect_names: Map<String, Value> =
        bank.tables.effects.iter().enumerate().filter(|(_, k)| !matches!(k, EffectKind::None)).map(|(i, k)| (i.to_string(), json!(k.name()))).collect();
    let mut song_index = Vec::new();
    for (i, s) in bank.songs.iter().enumerate() {
        let n = &names["songs"][i.to_string()];
        let name = n["name"].as_str().map(String::from).unwrap_or_else(|| format!("Song {i:02}"));
        let (log, lp) = Driver::song_structure(bank, i, 20000);
        let loop_v = match lp {
            Some((a, b)) => json!({
                "start_order": log[a].order, "start_row": log[a].row,
                "start_sample": log[a].sample, "end_sample": log[b].sample,
                "start_s": round(log[a].sample as f64 / driver::MIX_RATE, 5),
                "end_s": round(log[b].sample as f64 / driver::MIX_RATE, 5),
            }),
            None => Value::Null,
        };
        let channels: std::collections::BTreeSet<u8> = s.patterns.iter().flat_map(|p| p.rows.iter().flatten().map(|c| c.channel)).collect();
        let patterns: Vec<Value> = s
            .patterns
            .iter()
            .enumerate()
            .map(|(pi, p)| {
                let rows: Vec<Value> = p
                    .rows
                    .iter()
                    .map(|r| {
                        Value::Array(
                            r.iter()
                                .map(|c| {
                                    let mut m = Map::new();
                                    m.insert("ch".into(), json!(c.channel));
                                    m.insert("flags".into(), json!(c.flags));
                                    if c.flags & 1 != 0 {
                                        m.insert("note".into(), json!(c.note));
                                        m.insert("n".into(), json!(note_name(c.note)));
                                    }
                                    if c.flags & 2 != 0 {
                                        m.insert("inst".into(), json!(c.instrument));
                                    }
                                    if c.flags & 4 != 0 {
                                        m.insert("vol".into(), json!(c.volume));
                                    }
                                    if c.flags & 8 != 0 {
                                        m.insert("fx".into(), json!(c.effect));
                                        m.insert("param".into(), json!(c.param));
                                    }
                                    Value::Object(m)
                                })
                                .collect(),
                        )
                    })
                    .collect();
                json!({"index": pi, "rom": hx(p.rom), "rows": rows})
            })
            .collect();
        let spt = if s.tempo != 0 { 40000 / s.tempo as u32 } else { 0 };
        let doc = json!({
            "id": song_id(i),
            "index": i,
            "name": name,
            "context": n.get("context").cloned().unwrap_or(json!("")),
            "areas": n.get("areas").cloned().unwrap_or(json!([])),
            "tempo": s.tempo,
            "samples_per_tick": spt,
            "tick_hz": round(if spt != 0 { driver::MIX_RATE / spt as f64 } else { 0.0 }, 4),
            "speed": s.speed,
            "channels": channels.iter().collect::<Vec<_>>(),
            "orders": s.orders,
            "loop": loop_v,
            "ends": n.get("ends").cloned().unwrap_or(json!(false)),
            "effect_names": effect_names,
            "patterns": patterns,
            "rom": hx(s.rom),
        });
        out.put_json(format!("audio/songs/{}.json", song_id(i)), &doc);
        song_index.push(json!({"id": song_id(i), "index": i, "name": name, "file": format!("audio/songs/{}.json", song_id(i)),
                               "confidence": n.get("confidence").cloned().unwrap_or(Value::Null)}));
    }

    // ---- driver tables / constants
    let t = &bank.tables;
    out.put_json(
        "audio/driver.json",
        &json!({
            "doc": "engine/docs/audio.md",
            "mix_rate_hz": driver::MIX_RATE,
            "output": "stereo signed 8-bit (FIFO A = left, FIFO B = right)",
            "music_tracks": driver::N_TRACKS,
            "sfx_voices": driver::N_VOICES,
            "sfx_policy": "first free voice of 16, otherwise the request is dropped",
            "music_master_volume": driver::MUSIC_MASTER,
            "sfx_master_volume": driver::SFX_MASTER,
            "samples_per_tick": "floor(40000 / tempo)",
            "pitch_step": "step (18.14 fixed) = (((pitch * rate_hz) >> 14) * 0x1063 + 0x7FF) >> 12",
            "voice_gain": "L = (master * volume * (64 - pan)) >> 10, R = (master * volume * (64 + pan)) >> 10",
            "output_stage": "per sample w = sum(gain_packed * pcm) mod 2^32 with gain_packed = L | R << 16; left = clamp(i16(w & 0xFFFF) >> 6), right = clamp(i32(w) >> 22) to -128..127",
            "cell_flags": {"0x01": "note", "0x02": "instrument", "0x04": "volume", "0x08": "effect + param",
                           "0x10": "re-trigger the channel's last note", "0x20": "keep the current sample (no instrument reload)",
                           "0x40": "re-apply the channel's last volume", "0x80": "re-apply the channel's last effect"},
            "effects": t.effects.iter().map(|k| k.name()).collect::<Vec<_>>(),
            "pitch_table": t.pitch,
            "slide_up": t.slide_up,
            "slide_up_fine": t.slide_up_fine,
            "slide_down": t.slide_down,
            "slide_down_fine": t.slide_down_fine,
            "vibrato_sine": t.sine,
        }),
    );

    out.put_json(
        "audio/index.json",
        &json!({
            "format_doc": "engine/docs/audio.md",
            "driver": "audio/driver.json",
            "instruments": "audio/instruments.json",
            "songs": song_index,
            "sfx": sfx_index,
            "samples": samples_idx,
            "arena_music": names.get("arena_music").cloned().unwrap_or(Value::Null),
            "boss_music": names.get("boss_music").cloned().unwrap_or(Value::Null),
            "events": names.get("events").cloned().unwrap_or(Value::Null),
            "names_source": names.get("source").cloned().unwrap_or(Value::Null),
        }),
    );
}

fn round(x: f64, d: i32) -> f64 {
    let m = 10f64.powi(d);
    (x * m).round() / m
}
