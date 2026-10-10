// Event 930 (mod: rhai-story-event). Talking to Chiaotzu in the Southwest Forest (8-3).
// box = 8 + portrait index: 19 = portrait 11 (Chiaotzu), 55 = portrait 47 (Goku).
// Flags (mods use 900..1143): 1130 = the challenge was accepted, 1131 = won (the Dodon Ray is learned).
fn event() {
    [
        #{ node: 0, code: "n0_", entry: "main", next: 1 },
        #{ node: 1, say: "One of Babidi's soldiers is hiding in these trees. Beat him and I'll teach you my Dodon Ray!", box: 19, next: 2 },
        #{ node: 2, choice: "Take on the soldier?\n-Yes\n-Not now", box: 19, next: 3 },
        #{ node: 3, code: "n3_", entry: "main", next: 4 },
        #{ node: 4, say: "Bring it on!", box: 55, next: 5 },
        #{ node: 5, code: "n5_", entry: "main", next: 9 },
        #{ node: 6, say: "Come back when you're ready. I'll be here.", box: 19, next: 9 },
        #{ node: 7, say: "Point a finger, focus your ki... Press L to pick the Dodon Ray, then B!", box: 19, next: 9 },
        #{ node: 8, say: "He's still out there - be careful!", box: 19, next: 9 },
        #{ node: 9, end: 0 },
    ]
}

// node 0: already won -> the tip; the soldier already out -> the warning
fn n0_main() {
    if get_flag(1131) != 0 {
        return 7;
    }
    if get_flag(1130) != 0 {
        return 8;
    }
}

// node 3: the choice made in node 2 (0 = the first option)
fn n3_main() {
    if get_menu_result() != 0 {
        return 6;
    }
}

// node 5: the soldier jumps out (the area's object with list index 200; its on_defeat is s931)
fn n5_main() {
    set_flag(1130);
    play_sfx(45);
    spawn_map_object(200);
}
