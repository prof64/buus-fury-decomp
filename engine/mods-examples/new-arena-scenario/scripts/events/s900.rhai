// Event 900 (mod: new-arena-scenario). Talking to the announcer in the Southwest Forest (8-3).
// Speakers: box 10 = portrait 2 (Announcer), box 55 = portrait 47 (Goku); box = 8 + portrait index.
fn event() {
    [
        #{ node: 0, code: "n0_", entry: "main", next: 1 },
        #{ node: 1, say: "Step right up! The Tournament of Fury is open: three champions, back to back, no breaks. Beat them all and the prize is yours!", box: 10, next: 2 },
        #{ node: 2, choice: "Enter the Tournament of Fury?\n-Yes\n-No", box: 10, next: 3 },
        #{ node: 3, code: "n3_", entry: "main", next: 4 },
        #{ node: 4, say: "Let's go! I've been itching for a good fight.", box: 55, next: 5 },
        #{ node: 5, code: "n5_", entry: "main", next: 6 },
        #{ node: 6, end: 0 },
        #{ node: 7, say: "Come back when you're ready!", box: 10, next: 6 },
        #{ node: 8, say: "Well, if it isn't our champion! Back for a rematch?", box: 10, next: 2 },
    ]
}

// node 0: a champion (flag 1100, set by s904) gets a different greeting
fn n0_main() {
    if get_flag(1100) != 0 {
        return 8;
    }
}

// node 3: the choice window's result (0 = Yes)
fn n3_main() {
    if get_menu_result() != 0 {
        return 7;
    }
}

// node 5: into the arena (world 90, area 1, variant 1, player at x 96, y 216)
fn n5_main() {
    warp_fade(90, 1, 1, 96, 216);
}
