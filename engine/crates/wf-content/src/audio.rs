//! Rebuild `wf_audio::bank::SoundBank` from the `audio/` content files (docs/audio.md §5), so the runtime plays
//! (modded) content with the exact driver and never needs the ROM.

use serde_json::Value;
use wf_audio::bank::{Cell, Entry, Pattern, Sample, Song, SoundBank, Tables};
use wf_audio::driver::EffectKind;

use crate::{Error, Packs, Result};

fn hex(v: &Value) -> u32 {
    v.as_str().and_then(|s| u32::from_str_radix(s.trim_start_matches("0x"), 16).ok()).unwrap_or(0)
}

fn kind(name: &str) -> EffectKind {
    use EffectKind::*;
    match name {
        "set_speed" => SetSpeed,
        "pattern_jump" => PatternJump,
        "volume_slide" => VolumeSlide,
        "pitch_slide_down" => PitchSlideDown,
        "pitch_slide_up" => PitchSlideUp,
        "tone_portamento" => TonePortamento,
        "vibrato" => Vibrato,
        "vibrato_volume_slide" => VibratoVolumeSlide,
        "portamento_volume_slide" => PortamentoVolumeSlide,
        "sample_offset" => SampleOffset,
        "retrigger" => Retrigger,
        "set_tempo" => SetTempo,
        "fine_vibrato" => FineVibrato,
        "set_pan" => SetPan,
        "note_delay" => NoteDelay,
        _ => None,
    }
}

fn nums<T: TryFrom<i64>>(v: &Value) -> Vec<T> {
    v.as_array().map(|a| a.iter().filter_map(|x| x.as_i64().and_then(|i| T::try_from(i).ok())).collect()).unwrap_or_default()
}

pub fn load_bank(packs: &Packs) -> Result<SoundBank> {
    let index: Value = packs.json("audio/index.json")?;
    let driver: Value = packs.json(index["driver"].as_str().unwrap_or("audio/driver.json"))?;
    // samples: ids in index order; WAV 8-bit -> i8
    let mut sample_ids: Vec<String> = Vec::new();
    let mut samples = Vec::new();
    for s in index["samples"].as_array().into_iter().flatten() {
        let id = s["id"].as_str().unwrap_or("").to_string();
        let wav = packs.read(&format!("audio/samples/{id}.wav")).ok_or_else(|| Error(format!("missing audio/samples/{id}.wav")))?;
        let w = wf_audio::wav::read(&wav).ok_or_else(|| Error(format!("audio/samples/{id}.wav: bad WAV")))?;
        let pcm: Vec<i8> = w.samples.iter().step_by(w.channels.max(1) as usize).map(|&x| (x >> 8) as i8).collect();
        samples.push(Sample { rom: hex(&s["rom"]), pcm });
        sample_ids.push(id);
    }
    let mut entry = |e: &Value| -> Result<Entry> {
        let sid = e["sample"].as_str().unwrap_or("");
        let sample = match sample_ids.iter().position(|x| x == sid) {
            Some(i) => i,
            None => {
                // a mod may add a sample that is not in the index
                let wav = packs.read(&format!("audio/samples/{sid}.wav")).ok_or_else(|| Error(format!("missing sample {sid}")))?;
                let w = wf_audio::wav::read(&wav).ok_or_else(|| Error(format!("sample {sid}: bad WAV")))?;
                samples.push(Sample { rom: 0, pcm: w.samples.iter().map(|&x| (x >> 8) as i8).collect() });
                sample_ids.push(sid.to_string());
                samples.len() - 1
            }
        };
        let lp = &e["loop"];
        let loop_mode = e["loop_mode_raw"].as_u64().map(|v| v as u8).unwrap_or(match lp["mode"].as_str() {
            Some("forward") => 1,
            Some("pingpong") => 3,
            _ => 0,
        });
        Ok(Entry {
            rom: hex(&e["rom"]),
            sample,
            loop_mode,
            loop_start: lp["start"].as_u64().unwrap_or(0) as u16,
            length: e["length"].as_u64().unwrap_or(samples[sample].pcm.len() as u64) as u16,
            rate: e["rate_hz"].as_u64().unwrap_or(16000) as u16,
        })
    };
    let inst_json: Value = packs.json(index["instruments"].as_str().unwrap_or("audio/instruments.json"))?;
    let instruments = inst_json["instruments"].as_array().into_iter().flatten().map(&mut entry).collect::<Result<Vec<_>>>()?;
    let mut sfx = Vec::new();
    for s in index["sfx"].as_array().into_iter().flatten() {
        let f = s["file"].as_str().map(String::from).unwrap_or_else(|| format!("audio/sfx/{}.json", s["id"].as_str().unwrap_or("")));
        sfx.push(entry(&packs.json::<Value>(&f)?)?);
    }
    let mut songs = Vec::new();
    for s in index["songs"].as_array().into_iter().flatten() {
        let f = s["file"].as_str().map(String::from).unwrap_or_else(|| format!("audio/songs/{}.json", s["id"].as_str().unwrap_or("")));
        let j: Value = packs.json(&f)?;
        let patterns = j["patterns"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|p| Pattern {
                rom: hex(&p["rom"]),
                rows: p["rows"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(|row| {
                        row.as_array()
                            .into_iter()
                            .flatten()
                            .map(|c| Cell {
                                channel: c["ch"].as_u64().unwrap_or(0) as u8,
                                flags: c["flags"].as_u64().unwrap_or(0) as u8,
                                note: c["note"].as_u64().unwrap_or(0) as u8,
                                instrument: c["inst"].as_u64().unwrap_or(0) as u8,
                                volume: c["vol"].as_u64().unwrap_or(0) as u8,
                                effect: c["fx"].as_u64().unwrap_or(0) as u8,
                                param: c["param"].as_u64().unwrap_or(0) as u8,
                            })
                            .collect()
                    })
                    .collect(),
            })
            .collect();
        songs.push(Song {
            rom: hex(&j["rom"]),
            tempo: j["tempo"].as_u64().unwrap_or(0) as u8,
            speed: j["speed"].as_u64().unwrap_or(0) as u8,
            orders: nums(&j["orders"]),
            patterns,
        });
    }
    let tables = Tables {
        pitch: nums(&driver["pitch_table"]),
        slide_up: nums(&driver["slide_up"]),
        slide_up_fine: nums(&driver["slide_up_fine"]),
        slide_down: nums(&driver["slide_down"]),
        slide_down_fine: nums(&driver["slide_down_fine"]),
        sine: nums(&driver["vibrato_sine"]),
        effects: driver["effects"].as_array().into_iter().flatten().map(|v| kind(v.as_str().unwrap_or(""))).collect(),
    };
    Ok(SoundBank { samples, instruments, sfx, songs, tables, warnings: Vec::new() })
}

/// Song ids of the arena music table and other curated triggers (`audio/index.json`).
pub fn arena_song(packs: &Packs, arena: &str) -> Option<u16> {
    let index: Value = packs.json("audio/index.json").ok()?;
    index["arena_music"][arena].as_u64().map(|v| v as u16)
}
