// Event 910 (mod: new-area). The hermit of the Hidden Grove (90-2).
// First visit: a gift (a Senzu Bean) and story flag 1101; afterwards he heals the party.
// box 28 = portrait 20 (Elder Kai); box = 8 + portrait index.
fn event() {
    [
        #{ node: 0, code: "n0_", entry: "main", next: 1 },
        #{ node: 1, say: "Oh? Nobody has found this grove in a hundred years. You must have sharp eyes, young man.", box: 28, next: 2 },
        #{ node: 2, say: "Take this. A traveller should never go without one.", box: 28, next: 3 },
        #{ node: 3, code: "n3_", entry: "main", next: 6 },
        #{ node: 4, say: "Rest a while. The grove's air will mend your wounds.", box: 28, next: 5 },
        #{ node: 5, code: "n5_", entry: "main", next: 6 },
        #{ node: 6, end: 0 },
    ]
}

// node 0: queries run before any action, so the flag decides the branch
fn n0_main() {
    if get_flag(1101) != 0 {
        return 4;
    }
}

// node 3: the gift (item-get pose, 60 frames) and the flag
fn n3_main() {
    give_item(21);
    set_flag(1101);
}

// node 5: full HP / EP for everyone in the party
fn n5_main() {
    heal_party();
    play_sfx(45);
}
