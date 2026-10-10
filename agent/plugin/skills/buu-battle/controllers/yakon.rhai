// Yakon: energy heals him except in the ~60-frame pause after his 6-punch combo (AI state 0x323), where it does
// full damage. Select Kamehameha, block his combo, and fire the beam the moment he enters 0x323: line up on his
// row, face him, hold B for the whole window (release when it ends - a beam outside the window heals him).
// When he summons Mini-Yakons (state 0x325 ignores every hit until they are gone) punch them down.
let p = obs.player;
let yak = ();
let mini = ();
for e in obs.enemies {
    if e.name == "Yakon" { yak = e; } else if mini == () { mini = e; }
}
if yak == () && mini == () { return ""; }
let window = yak != () && yak.state == "0x323";
if p.state == "skill" { return if window { "B" } else { "" }; }
if !p.can_act { return if "danger" in obs { "R" } else { "" }; }
// minis first (nearest one): plain melee
if mini != () && (yak == () || yak.state == "0x325") {
    if "melee" in mini {
        if mini.melee == "in_range" { mem.a = !(mem.a == true); return if mem.a { "A" } else { "" }; }
        return mini.melee.sub_string(5).to_upper();
    }
    let side = if p.pos[0] < mini.pos[0] { -1 } else { 1 };
    return toward(p.pos, [mini.pos[0] + side * 14, mini.pos[1]]);
}
let dy = yak.pos[1] - p.pos[1];
let dx = yak.pos[0] - p.pos[0];
// select Kamehameha (L cycles; a press acts on the frame it goes down)
if p.skill.name != "Kamehameha" { mem.l = !(mem.l == true); return if mem.l { "L" } else { "" }; }
if window && p.skill.usable {
    if dy.abs() > 3 { return if dy > 0 { "DOWN" } else { "UP" }; }
    let want = if dx > 0 { "right" } else { "left" };
    if p.facing != want { return want.to_upper(); }
    return "B";
}
// his combo / anything about to hit: block
if "danger" in obs && obs.danger.hit_in <= 8 { return "R"; }
// not enough EP for the beam: punch him (x0.05 damage, but every damaging punch gives EP back)
if !p.skill.usable {
    if "melee" in yak {
        if yak.melee == "in_range" { mem.a = !(mem.a == true); return if mem.a { "A" } else { "" }; }
        return yak.melee.sub_string(5).to_upper();
    }
    let side = if dx > 0 { -1 } else { 1 };
    return toward(p.pos, [yak.pos[0] + side * 14, yak.pos[1]]);
}
// otherwise wait on his row at beam distance (60..110 px) so the window is not spent walking
let v = if dy > 3 && obs.walls.down > 0 { "DOWN" } else if dy < -3 && obs.walls.up > 0 { "UP" } else { "" };
let h = if dx.abs() < 60 { if dx > 0 { "LEFT" } else { "RIGHT" } } else if dx.abs() > 110 || v == "" && dy.abs() > 3 { if dx > 0 { "RIGHT" } else { "LEFT" } } else { "" };
if v != "" { return v; }
h
