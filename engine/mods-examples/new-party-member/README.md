# new-party-member — Tien joins the story party

A story character (index 10) with his own record, growth, form and equipment (`docs/modding.md` §5b). He stands in
the Southwest Forest (8-3); talking to him adds him to the party, and a choice lets him take the lead at once (later:
the character menu or `set_character(10)`).

| file | what |
|---|---|
| `characters/tien.json` | character 10: new-game record, skills `[8, 7]` (Energy Punch, Energy Blast), `equip`, `in_party: false`, growth (`raw_u32`, more HP / less EP than Goku), forms `[56]` |
| `forms/56_tien.json` | `$extends` the game's Tien form (51) with `char_index` 10, `flags` 16 (the XP bit, which the game's side-character forms lack) and its own bonus: the game's Tien sprite (255) |
| `world/areas/8-3+tien.json` | the NPC (script sprite id 264 = sprite 255 + 9), placed until flag 1110 is set |
| `scripts/events/s920.rhai` | the talk: `join_party(10)`, flag 1110, a Yes / No choice → `set_character(10)`; portrait box 135 (Tien, portrait 127) |

Ids: characters 9+ (the battle-only `new-character` example uses 9), forms 54+ (this one 56), events 600+, flags
900..1143.

## Try it

```
buus-fury --mod engine/mods-examples/new-party-member --story 8-3
```

## Test

`cargo test -p wf-app --test mods new_party_member` — the story records grow to 11, Tien is placed, the talk adds
him to the party and switches the leader (form 56, sprite 255, skills, 150 HP, equipment); a level-up uses his
growth and the record is written back; his skill starts; the frame renders.
